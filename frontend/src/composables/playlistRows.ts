import { computed, reactive, type Ref } from 'vue'
import { useI18n } from 'vue-i18n'

import type { useOnAir } from '@/composables/onAir'
import { clockShort } from '@/composables/player'
import { secondsFromDay, splitDateTime } from '@/composables/schedule'
import { useConfig } from '@/stores/config'
import { usePlaylist } from '@/stores/playlist'
import { useProgrammes, type ProgrammeGroup } from '@/stores/programmes'
import { useSchedule } from '@/stores/schedule'

interface PlaylistRowContext {
    view: Ref<'program' | 'flat'>
    editing: Ref<{ uid: string; item: PlaylistItem } | null>
    recent: Ref<string[]>
    onAir: Pick<ReturnType<typeof useOnAir>, 'onAirIndex' | 'viewingOnAir' | 'listNow' | 'playlistNow'>
}

/** Read-only row/day-map projection; only expansion and pagination are owned here. */
export function usePlaylistRows({ view, editing, recent, onAir }: PlaylistRowContext) {
    const { t, locale } = useI18n()
    const configStore = useConfig()
    const playlistStore = usePlaylist()
    const programmes = useProgrammes()
    const scheduleStore = useSchedule()
    const { onAirIndex, viewingOnAir, listNow, playlistNow } = onAir
    const infinite = computed(() => Boolean(configStore.playout.playlist?.infinit))
    const dayStart = computed(() => configStore.playout.playlist?.startInSec ?? 0)
    const PAGE = 10
    const MORE = 20
    // Small programmes show every clip, rather than looking incomplete.
    const WHOLE_GROUP = 40
    const toggled = reactive(new Map<string, boolean>())
    const reveal = reactive(new Map<string, { before: number; after: number }>())

    interface LiveRow {
        key: string
        title: string
        start: number
        end: number
        protocol: string
        status: RunStatus | null
    }

    interface GroupChip {
        label: string
        kind: 'live' | 'event'
    }

    const dayEvents = computed(() => {
        const date = playlistStore.listDate

        if (scheduleStore.date !== date) {
            return []
        }

        return scheduleStore.occurrences
            .map((occurrence) => ({
                occurrence,
                start: secondsFromDay(occurrence.start, date),
                end: secondsFromDay(occurrence.end, date),
            }))
            .filter((event) => event.start >= dayStart.value)
    })

    const lives = computed<LiveRow[]>(() =>
        dayEvents.value
            .filter((event) => event.occurrence.event.kind === 'live')
            .map((event) => ({
                key: `${event.occurrence.event.id}-${event.occurrence.start}`,
                title: event.occurrence.event.title,
                start: event.start,
                end: event.end,
                protocol: protocolOf(event.occurrence.event.sources[0] ?? ''),
                status: event.occurrence.status,
            })),
    )

    function protocolOf(path: string): string {
        const live = scheduleStore.lives.find((item) => item.path === path)

        return live?.protocol?.toUpperCase() ?? ''
    }

    /** Scheduled clips that land inside a program; lives get their own row. */
    function chipsOf(group: ProgrammeGroup): GroupChip[] {
        return dayEvents.value
            .filter((event) => event.occurrence.event.kind !== 'live')
            .filter((event) => group.start < event.start && event.start < group.end)
            .map((event) => ({
                kind: 'event',
                label: t('onair.eventChip', {
                    title: event.occurrence.event.title,
                    time: splitDateTime(event.occurrence.start).time.slice(0, 5),
                    mode:
                        event.occurrence.event.entry_mode === 'interrupt'
                            ? t('onair.eventInterrupts')
                            : t('onair.eventAfterClip'),
                }),
            }))
    }

    /** Groups get an id that survives edits inside them: key plus occurrence count. */
    const keyedGroups = computed(() => {
        const seen = new Map<string, number>()

        return programmes.groups.map((group) => {
            const count = seen.get(group.key) ?? 0
            seen.set(group.key, count + 1)

            return { group, id: `${group.key}#${count}` }
        })
    })

    function groupStatus(group: ProgrammeGroup): 'past' | 'current' | 'future' {
        if (onAirIndex.value >= 0) {
            if (group.last < onAirIndex.value) {
                return 'past'
            }

            return group.first <= onAirIndex.value ? 'current' : 'future'
        }

        if (viewingOnAir.value && !infinite.value && playlistStore.playoutIsRunning) {
            if (group.end <= listNow.value) {
                return 'past'
            }

            return group.start <= listNow.value ? 'current' : 'future'
        }

        return 'future'
    }

    function containsUid(group: ProgrammeGroup, uids: string[]): number {
        for (let index = group.last; index >= group.first; index--) {
            if (uids.includes(playlistStore.playlist[index]?.uid ?? '')) {
                return index
            }
        }

        return -1
    }

    /** The row a group's window is centred on. */
    function anchorOf(group: ProgrammeGroup): number {
        if (editing.value) {
            const index = containsUid(group, [editing.value.uid])

            if (index >= 0) {
                return index
            }
        }

        const added = containsUid(group, [...playlistStore.addedUids, ...recent.value])

        if (added >= 0) {
            return added
        }

        if (group.first <= onAirIndex.value && onAirIndex.value <= group.last) {
            return onAirIndex.value
        }

        return group.first
    }

    function isExpanded(id: string, group: ProgrammeGroup, position: number): boolean {
        const choice = toggled.get(id)

        if (choice !== undefined) {
            return choice
        }

        if (groupStatus(group) === 'current') {
            return true
        }

        // Programs with the clip being edited, or clips just added or changed.
        const touched = [...playlistStore.addedUids, ...recent.value, ...(editing.value ? [editing.value.uid] : [])]

        if (containsUid(group, touched) >= 0) {
            return true
        }

        // Nothing on air in this list: open the first program.
        return onAirIndex.value < 0 && position === 0 && !viewingOnAir.value
    }

    function toggleGroup(id: string, expanded: boolean) {
        toggled.set(id, !expanded)
    }

    type Row =
        | { type: 'group'; id: string; group: ProgrammeGroup; status: string; expanded: boolean; chips: GroupChip[] }
        | { type: 'clip'; index: number; item: PlaylistItem; past: boolean }
        | { type: 'editor'; index: number }
        | { type: 'more'; id: string; direction: 'before' | 'after'; count: number }
        | { type: 'live'; live: LiveRow }

    /** The running list: clips around the one on air (or the first), with lives in between. */
    function flatRows(): Row[] {
        const list: Row[] = []
        const pendingLives = [...lives.value].sort((a, b) => a.start - b.start)
        const items = playlistStore.playlist
        const extra = reveal.get('flat') ?? { before: 0, after: 0 }
        const touched = [...playlistStore.addedUids, ...recent.value, ...(editing.value ? [editing.value.uid] : [])]
        const touchedIndex = items.findIndex((item) => touched.includes(item.uid))
        const anchor = touchedIndex >= 0 ? touchedIndex : Math.max(onAirIndex.value, 0)
        const from = Math.max(0, anchor - 3 - extra.before)
        const to = Math.min(items.length - 1, anchor + PAGE * 3 + extra.after)

        if (from > 0) {
            list.push({ type: 'more', id: 'flat', direction: 'before', count: Math.min(MORE, from) })
        }

        // Lives before the shown clips stay hidden with them.
        const firstBegin = items[from]?.begin ?? 0

        while (from > 0 && pendingLives[0] && pendingLives[0].start < firstBegin) {
            pendingLives.shift()
        }

        for (let index = from; index <= to; index++) {
            const item = items[index]

            if (!item) {
                continue
            }

            while (pendingLives[0] && pendingLives[0].start <= (item.begin ?? 0)) {
                list.push({ type: 'live', live: pendingLives.shift()! })
            }

            if (editing.value?.uid === item.uid) {
                list.push({ type: 'editor', index })
            } else {
                list.push({ type: 'clip', index, item, past: onAirIndex.value >= 0 && index < onAirIndex.value })
            }
        }

        if (to < items.length - 1) {
            list.push({ type: 'more', id: 'flat', direction: 'after', count: Math.min(MORE, items.length - 1 - to) })
        } else {
            list.push(...pendingLives.map((live) => ({ type: 'live' as const, live })))
        }

        return list
    }

    const rows = computed<Row[]>(() => {
        if (view.value === 'flat') {
            return flatRows()
        }

        const list: Row[] = []
        const pendingLives = [...lives.value].sort((a, b) => a.start - b.start)

        keyedGroups.value.forEach(({ group, id }, position) => {
            while (pendingLives[0] && pendingLives[0].start <= group.start) {
                list.push({ type: 'live', live: pendingLives.shift()! })
            }

            const status = groupStatus(group)
            const expanded = isExpanded(id, group, position)

            list.push({ type: 'group', id, group, status, expanded, chips: chipsOf(group) })

            if (!expanded) {
                return
            }

            const extra = reveal.get(id) ?? { before: 0, after: 0 }
            const anchor = anchorOf(group)
            // A program of a few clips shows them all: hiding some looked like
            // they were missing.
            const whole = group.last - group.first < WHOLE_GROUP
            const from = whole ? group.first : Math.max(group.first, anchor - 1 - extra.before)
            const to = whole ? group.last : Math.min(group.last, Math.max(anchor, from) + PAGE - 1 + extra.after)

            if (from > group.first) {
                list.push({ type: 'more', id, direction: 'before', count: Math.min(MORE, from - group.first) })
            }

            for (let index = from; index <= to; index++) {
                const item = playlistStore.playlist[index]

                if (!item) {
                    continue
                }

                if (editing.value?.uid === item.uid) {
                    list.push({ type: 'editor', index })
                } else {
                    list.push({ type: 'clip', index, item, past: onAirIndex.value >= 0 && index < onAirIndex.value })
                }
            }

            if (to < group.last) {
                list.push({ type: 'more', id, direction: 'after', count: Math.min(MORE, group.last - to) })
            }
        })

        for (const live of pendingLives) {
            list.push({ type: 'live', live })
        }

        return list
    })

    function setAllGroups(open: boolean) {
        for (const { id } of keyedGroups.value) {
            toggled.set(id, open)
        }
    }

    /** The day as a strip: programs, lives and scheduled clips by their time. */
    const dayMap = computed(() => {
        const start = dayStart.value
        const length = configStore.playout.playlist?.lengthInSec || 86400
        const percent = (seconds: number) => Math.min(100, Math.max(0, ((seconds - start) / length) * 100))
        const blocks = keyedGroups.value.map(({ group, id }) => ({
            id,
            title: group.title,
            left: percent(group.start),
            width: Math.max(0.15, percent(group.end) - percent(group.start)),
            status: groupStatus(group),
            range: `${clockShort(group.start)}–${clockShort(group.end)}`,
        }))
        const liveBlocks = lives.value.map((live) => ({
            key: live.key,
            title: live.title,
            left: percent(live.start),
            width: Math.max(0.3, percent(live.end) - percent(live.start)),
            range: `${clockShort(live.start)}–${clockShort(live.end)}`,
        }))
        const markers = dayEvents.value
            .filter((event) => event.occurrence.event.kind !== 'live')
            .map((event) => ({
                key: `${event.occurrence.event.id}-${event.occurrence.start}`,
                title: `${event.occurrence.event.title} · ${clockShort(event.start)}`,
                left: percent(event.start),
            }))
        const ticks = Array.from({ length: 9 }, (_, index) => clockShort(start + (length / 8) * index))
        const now = viewingOnAir.value && playlistStore.playoutIsRunning ? percent(playlistNow.value) : null

        return { blocks, liveBlocks, markers, ticks, now }
    })

    function showMore(id: string, direction: 'before' | 'after') {
        const current = reveal.get(id) ?? { before: 0, after: 0 }

        reveal.set(id, { ...current, [direction]: current[direction] + MORE })
    }

    function groupSummary(group: ProgrammeGroup, status: string): string {
        const clips = group.last - group.first + 1 - group.inserts - group.breaks
        const parts = [
            `${clockShort(group.start)}–${clockShort(group.end)}`,
            t('onair.clipCount', { total: clips.toLocaleString(locale.value) }, clips),
        ]

        if (group.inserts) {
            parts.push(t('onair.insertCount', { count: group.inserts }, group.inserts))
        }

        if (group.breaks) {
            parts.push(t('onair.breakCount', { count: group.breaks }, group.breaks))
        }

        if (status === 'past') {
            parts.push(t('onair.aired'))
        }

        return `· ${parts.join(' · ')}`
    }

    function resetGroups() {
        toggled.clear()
        reveal.clear()
    }

    function openGroup(id: string) {
        view.value = 'program'
        toggled.set(id, true)
    }

    return { rows, dayMap, toggleGroup, setAllGroups, openGroup, showMore, groupSummary, resetGroups }
}

export type PlaylistDayMap = ReturnType<typeof usePlaylistRows>['dayMap']['value']
