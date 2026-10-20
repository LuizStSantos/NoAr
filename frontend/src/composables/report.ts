import dayjs from 'dayjs'
import timezone from 'dayjs/plugin/timezone.js'
import utc from 'dayjs/plugin/utc.js'

dayjs.extend(utc)
dayjs.extend(timezone)

/** Kinds the type filter offers; gaps always show. */
export const REPORT_KINDS: Exclude<ReportKind, 'gap'>[] = ['program', 'commercial', 'live', 'filler', 'ingest']

/**
 * Longest clip without a sheet that counts as an insert (an ident, a promo)
 * rather than a programme, as in the guide.
 */
const INSERT_MAX_SECONDS = 300

/** Which part of the time: all of it, what already aired, or what is to come. */
export type ReportPart = 'all' | 'aired' | 'planned'

export interface ReportFilter {
    part: ReportPart
    kinds: ReportKind[]
    search: string
    picks: ReportPick[]
}

type Translate = (key: string, values?: Record<string, unknown>, plural?: number) => string

/** Already aired (or on air), as opposed to still to come. */
export function isPast(item: ReportItem): boolean {
    return item.state !== 'planned'
}

/** Seconds an item lasts; for the item on air, its planned length. */
export function itemSeconds(item: ReportItem): number {
    return Math.max(0, item.end - item.start) / 1000
}

export function matchesPick(item: ReportItem, pick: ReportPick): boolean {
    if (item.kind === 'gap') {
        return false
    }

    switch (pick.type) {
        case 'file':
        case 'live':
            return item.source === pick.value
        case 'folder':
            return item.source.startsWith(`${pick.value}/`)
        case 'event':
            return String(item.event_id) === pick.value
    }
}

export function filterItems(items: ReportItem[], filter: ReportFilter): ReportItem[] {
    const needle = filter.search.trim().toLowerCase()

    return items.filter((item) => {
        if (filter.part === 'aired' && !isPast(item)) {
            return false
        }

        if (filter.part === 'planned' && isPast(item)) {
            return false
        }

        if (filter.picks.length) {
            return filter.picks.some((pick) => matchesPick(item, pick))
        }

        if (item.kind === 'gap') {
            return !needle
        }

        if (!filter.kinds.includes(item.kind)) {
            return false
        }

        return (
            !needle ||
            [item.title, item.sub_title, item.source, item.event_title]
                .filter(Boolean)
                .join(' ')
                .toLowerCase()
                .includes(needle)
        )
    })
}

export interface Totals {
    count: number
    seconds: number
}

export interface ReportSummary {
    aired: Totals
    planned: Totals
    commercials: Totals & { aired: number; planned: number }
    lives: Totals & { names: string[] }
    cut: number
    failed: number
    /** Items placed by the playlist (or folder) and by the Scheduler. */
    fromPlaylist: number
    fromSchedule: number
}

export function summarize(items: ReportItem[], now: number): ReportSummary {
    const summary: ReportSummary = {
        aired: { count: 0, seconds: 0 },
        planned: { count: 0, seconds: 0 },
        commercials: { count: 0, seconds: 0, aired: 0, planned: 0 },
        lives: { count: 0, seconds: 0, names: [] },
        cut: 0,
        failed: 0,
        fromPlaylist: 0,
        fromSchedule: 0,
    }

    for (const item of items) {
        if (item.kind === 'gap') {
            continue
        }

        const seconds = itemSeconds(item)

        if (item.state === 'on_air') {
            const aired = Math.max(0, now - item.start) / 1000

            summary.aired.count += 1
            summary.aired.seconds += aired
            summary.planned.seconds += Math.max(0, seconds - aired)
        } else if (isPast(item)) {
            summary.aired.count += 1
            summary.aired.seconds += seconds
        } else {
            summary.planned.count += 1
            summary.planned.seconds += seconds
        }

        if (item.kind === 'commercial') {
            summary.commercials.count += 1
            summary.commercials.seconds += seconds

            if (isPast(item)) {
                summary.commercials.aired += 1
            } else {
                summary.commercials.planned += 1
            }
        }

        if (item.kind === 'live') {
            const name = displayTitle(item)

            summary.lives.seconds += seconds

            if (!summary.lives.names.includes(name)) {
                summary.lives.names.push(name)
            }
        }

        if (item.status === 'cut') {
            summary.cut += 1
        }

        if (item.status === 'failed') {
            summary.failed += 1
        }

        if (item.origin === 'schedule') {
            summary.fromSchedule += 1
        } else if (item.origin === 'playlist' || item.origin === 'folder') {
            summary.fromPlaylist += 1
        }
    }

    summary.lives.count = items.filter(
        (item, index) =>
            item.kind === 'live' &&
            // A live that lost its signal airs in several parts: count it once.
            !items.slice(0, index).some((earlier) => earlier.kind === 'live' && sameOccurrence(earlier, item)),
    ).length

    return summary
}

function sameOccurrence(a: ReportItem, b: ReportItem): boolean {
    return a.event_id !== null && a.event_id === b.event_id && a.scheduled_at === b.scheduled_at
}

export interface DaySummary {
    day: ReportDay
    count: number
    seconds: number
    /** Recorded as aired, or on air now. */
    aired: number
    /** Past items with no record, only the plan. */
    unrecorded: number
    planned: number
    cut: number
    failed: number
}

export function summarizeDays(days: ReportDay[], items: ReportItem[]): DaySummary[] {
    return days.map((day) => {
        const own = items.filter((item) => item.day === day.date && item.kind !== 'gap')
        const count = (test: (item: ReportItem) => boolean) => own.filter(test).length

        return {
            day,
            count: own.length,
            seconds: own.reduce((sum, item) => sum + itemSeconds(item), 0),
            aired: count((item) => item.state === 'aired' || item.state === 'on_air'),
            unrecorded: count((item) => item.state === 'unrecorded'),
            planned: count((item) => item.state === 'planned'),
            cut: count((item) => item.status === 'cut'),
            failed: count((item) => item.status === 'failed'),
        }
    })
}

/** A day of a period in words: how its items went, or are planned to go. */
export function dayText(day: DaySummary, t: Translate): string {
    if (!day.count) {
        return t('report.days.empty')
    }

    if (!day.day.playlist && day.planned === day.count) {
        return t('report.days.noPlaylist')
    }

    if (day.aired === day.count && !day.cut && !day.failed) {
        return t('report.days.allWhole')
    }

    const whole = day.aired - day.cut - day.failed
    const parts = [
        whole && (day.cut || day.failed) ? t('report.days.whole', { count: whole }, whole) : '',
        whole && !day.cut && !day.failed ? t('report.days.aired', { count: whole }, whole) : '',
        day.cut ? t('report.days.cut', { count: day.cut }, day.cut) : '',
        day.failed ? t('report.days.failed', { count: day.failed }, day.failed) : '',
        day.unrecorded ? t('report.days.unrecorded', { count: day.unrecorded }, day.unrecorded) : '',
        day.planned ? t('report.days.planned', { count: day.planned }, day.planned) : '',
    ]

    return parts.filter(Boolean).join(' · ')
}

/**
 * The programme on air before each item of the day (a commercial's break
 * sits in it), from the full list of items.
 */
export function previousProgrammes(items: ReportItem[]): Map<ReportItem, ReportItem> {
    const previous = new Map<ReportItem, ReportItem>()
    let programme: ReportItem | undefined

    for (const item of items) {
        if (programme && programme.day !== item.day) {
            programme = undefined
        }

        if (programme) {
            previous.set(item, programme)
        }

        if (isProgramme(item)) {
            programme = item
        }
    }

    return previous
}

/**
 * A programme, as opposed to what airs inside or between programmes: a
 * live, ingest, or a clip with a sheet or long enough to be a programme
 * (not a station ident).
 */
function isProgramme(item: ReportItem): boolean {
    switch (item.kind) {
        case 'live':
        case 'ingest':
            return true
        case 'program':
            return item.program !== null || itemSeconds(item) >= INSERT_MAX_SECONDS
        default:
            return false
    }
}

/** Title with the episode, as the guide shows it. */
export function displayTitle(item: ReportItem): string {
    return item.sub_title ? `${item.title} — ${item.sub_title}` : item.title
}

export function fileName(path: string): string {
    return path.split('/').pop() ?? path
}

export function parentFolder(path: string): string {
    return path.includes('/') ? path.slice(0, path.lastIndexOf('/')) : ''
}

export function clock(millis: number, zone: string): string {
    return dayjs(millis).tz(zone).format('HH:mm:ss')
}

/** `1:37:30`, `24:10`, `0:30`. */
export function duration(seconds: number): string {
    const total = Math.max(0, Math.round(seconds))
    const hours = Math.floor(total / 3600)
    const minutes = Math.floor((total % 3600) / 60)
    const rest = String(total % 60).padStart(2, '0')

    return hours ? `${hours}:${String(minutes).padStart(2, '0')}:${rest}` : `${minutes}:${rest}`
}

/** `HH:MM:SS` for spreadsheets, which add it up as a time. */
export function fullDuration(seconds: number): string {
    const total = Math.max(0, Math.round(seconds))
    const pad = (value: number) => String(value).padStart(2, '0')

    return `${pad(Math.floor(total / 3600))}:${pad(Math.floor((total % 3600) / 60))}:${pad(total % 60)}`
}

/** `Ter, 20 out` (`Tue, Oct 20`) in the interface's language. */
export function dayLabel(date: string, locale: string, withYear = false): string {
    const parts = new Intl.DateTimeFormat(locale, {
        weekday: 'short',
        day: '2-digit',
        month: 'short',
        year: withYear ? 'numeric' : undefined,
        timeZone: 'UTC',
    }).formatToParts(new Date(`${date}T12:00:00Z`))
    const part = (type: Intl.DateTimeFormatPartTypes) =>
        (parts.find((entry) => entry.type === type)?.value ?? '').replace(/\.$/, '')
    const weekday = part('weekday')
    const day = part('day')
    const month = part('month')
    const year = withYear ? ` ${part('year')}` : ''
    const monthFirst =
        parts.findIndex((entry) => entry.type === 'month') < parts.findIndex((entry) => entry.type === 'day')
    const label = monthFirst ? `${weekday}, ${month} ${day}${year}` : `${weekday}, ${day} ${month}${year}`

    return label.charAt(0).toUpperCase() + label.slice(1)
}

/** `20/10/2026` or `10/20/2026`, as the language writes dates. */
export function shortDate(date: string, locale: string): string {
    return new Intl.DateTimeFormat(locale, { timeZone: 'UTC' }).format(new Date(`${date}T12:00:00Z`))
}

export type StatusTone = 'ok' | 'warn' | 'error' | 'on-air' | 'muted'

export interface StatusText {
    label: string
    /** A second line: why it ended early, or when an event was due. */
    note: string
    tone: StatusTone
}

/** How an item went, or is planned to go, in words. */
export function statusText(item: ReportItem, t: Translate, zone: string, now: number): StatusText {
    const aired = duration(itemSeconds(item))
    const planned = duration(item.planned_seconds)
    const due =
        item.scheduled_at !== null && Math.abs(item.start - item.scheduled_at) > 1500
            ? clock(item.scheduled_at, zone)
            : ''

    if (item.kind === 'gap') {
        return { label: t('report.status.gap'), note: '', tone: 'muted' }
    }

    switch (item.state) {
        case 'planned':
            return {
                label: t('report.status.planned'),
                note: due ? t('report.note.dueAfterClip', { time: due }) : '',
                tone: 'muted',
            }
        case 'unrecorded':
            return { label: t('report.status.unrecorded'), note: '', tone: 'muted' }
        case 'on_air':
            return {
                label: t('report.status.onAir'),
                note: t('report.note.progress', {
                    elapsed: duration(Math.max(0, now - item.start) / 1000),
                    total: duration(itemSeconds(item)),
                }),
                tone: 'on-air',
            }
    }

    switch (item.status) {
        case 'cut':
            return { label: t('report.status.cut'), note: cutNote(item, t, aired, planned), tone: 'warn' }
        case 'failed':
            return { label: t('report.status.failed'), note: t('report.note.failed'), tone: 'error' }
        case 'stopped':
            return {
                label: t('report.status.stopped'),
                note: item.reason === 'interrupted' ? t('report.note.interrupted') : '',
                tone: 'warn',
            }
        default:
            return {
                label: t('report.status.completed'),
                note: due ? t('report.note.waited', { time: due }) : '',
                tone: 'ok',
            }
    }
}

function cutNote(item: ReportItem, t: Translate, aired: string, planned: string): string {
    const part = item.planned_seconds > 0 && item.kind !== 'filler' ? t('report.note.partOf', { aired, planned }) : ''
    const why = (() => {
        switch (item.reason) {
            case 'event':
                return t('report.note.event', { name: item.detail ?? '' })
            case 'ingest':
                return t('report.note.ingest', { name: item.detail ?? '' })
            case 'skipped':
                return t('report.note.skipped')
            case 'signal_lost':
                return t('report.note.signalLost')
            default:
                return ''
        }
    })()

    return [part, why].filter(Boolean).join(' · ')
}

/** Kind of an item in words. */
export function kindText(item: ReportItem, t: Translate): string {
    return item.kind === 'gap' ? '' : t(`report.kind.${item.kind}`)
}

/** Where an item came from in words: the playlist, an event, ingest. */
export function originText(item: ReportItem, t: Translate): string {
    switch (item.origin) {
        case 'schedule':
            return item.event_title ? t('report.origin.event', { name: item.event_title }) : t('report.origin.schedule')
        case 'folder':
            return t('report.origin.folder')
        case 'ingest':
            return t('report.origin.ingest')
        case 'playlist':
            return t('report.origin.playlist')
        default:
            return ''
    }
}

/** The item's name in the list. */
export function itemTitle(item: ReportItem, t: Translate): string {
    switch (item.kind) {
        case 'gap':
            return t('report.gapTitle')
        case 'filler':
            return t('report.fillerTitle')
        default:
            return displayTitle(item) || fileName(item.source)
    }
}

/** The line under the name: the file, or what a filler stood in for. */
export function itemDetail(item: ReportItem, t: Translate): string {
    switch (item.kind) {
        case 'gap':
            return t('report.gapDetail')
        case 'filler':
            return item.replaces ? t('report.insteadOf', { name: item.replaces }) : ''
        case 'ingest':
            return ''
        default:
            return item.source
    }
}

export interface PickOption extends ReportPick {
    /** Second line: the storage path or what it holds. */
    detail: string
    count: number
    seconds: number
}

export interface PickGroups {
    programs: PickOption[]
    commercials: PickOption[]
    lives: PickOption[]
    events: PickOption[]
}

/** What the report can be narrowed to, from what the items hold. */
export function pickGroups(items: ReportItem[], folderLabel: (folder: string) => string): PickGroups {
    const groups = new Map<string, PickOption>()
    const add = (key: string, option: Omit<PickOption, 'count' | 'seconds'>, item: ReportItem) => {
        const found = groups.get(key) ?? { ...option, count: 0, seconds: 0 }

        found.count += 1
        found.seconds += itemSeconds(item)
        groups.set(key, found)
    }

    for (const item of items) {
        if (!item.source || item.kind === 'gap' || item.kind === 'filler' || item.kind === 'ingest') {
            continue
        }

        if (item.kind === 'program') {
            const folder = item.program ?? parentFolder(item.source)

            if (folder) {
                const label = item.program ? item.title : fileName(folder)

                add(
                    `program:${folder}`,
                    { type: 'folder', value: folder, label, group: 'program', detail: folder },
                    item,
                )
            }
        }

        if (item.kind === 'commercial') {
            const folder = parentFolder(item.source)

            add(
                `commercial:${item.source}`,
                {
                    type: 'file',
                    value: item.source,
                    label: displayTitle(item) || fileName(item.source),
                    group: 'commercial',
                    detail: item.source,
                },
                item,
            )

            if (folder) {
                add(
                    `folder:${folder}`,
                    { type: 'folder', value: folder, label: folderLabel(folder), group: 'folder', detail: folder },
                    item,
                )
            }
        }

        if (item.kind === 'live') {
            add(
                `live:${item.source}`,
                {
                    type: 'live',
                    value: item.source,
                    label: displayTitle(item),
                    group: 'live',
                    detail: parentFolder(item.source),
                },
                item,
            )
        }

        if (item.event_id !== null) {
            add(
                `event:${item.event_id}`,
                {
                    type: 'event',
                    value: String(item.event_id),
                    label: item.event_title ?? '',
                    group: 'event',
                    detail: '',
                },
                item,
            )
        }
    }

    const options = [...groups.values()]
    const byName = (a: PickOption, b: PickOption) => a.label.localeCompare(b.label)

    return {
        programs: options.filter((option) => option.group === 'program').sort(byName),
        commercials: options
            .filter((option) => option.group === 'commercial' || option.group === 'folder')
            .sort((a, b) => (a.group === b.group ? byName(a, b) : a.group === 'commercial' ? -1 : 1)),
        lives: options.filter((option) => option.group === 'live').sort(byName),
        events: options.filter((option) => option.group === 'event').sort(byName),
    }
}
