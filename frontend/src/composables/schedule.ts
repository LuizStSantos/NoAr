import { useI18n } from 'vue-i18n'

/** Weekday bits used by `repeat_days`, Sunday first. */
export const WEEKDAY_BITS = [1, 2, 4, 8, 16, 32, 64]
export const EVERY_DAY = 127
export const MONDAY_TO_FRIDAY = 62

/** Colors of the scheduler, also used by the legend. */
export const SCHEDULE_COLORS = {
    after_clip: '#436e9c',
    interrupt: '#f28c1b',
    live: '#ad7fa8',
}

export function eventColor(event: Pick<ScheduleEvent, 'kind' | 'entry_mode'>): string {
    if (event.kind === 'live') {
        return SCHEDULE_COLORS.live
    }

    return event.entry_mode === 'interrupt' ? SCHEDULE_COLORS.interrupt : SCHEDULE_COLORS.after_clip
}

/** Seconds since midnight of `HH:MM[:SS]`. */
export function timeToSeconds(time: string): number {
    const [h = '0', m = '0', s = '0'] = time.split(':')

    return parseInt(h) * 3600 + parseInt(m) * 60 + parseFloat(s)
}

/** `HH:MM:SS` of seconds since midnight. */
export function secondsToTime(seconds: number): string {
    const total = Math.max(0, Math.floor(seconds)) % 86400
    const h = Math.floor(total / 3600)
    const m = Math.floor((total % 3600) / 60)
    const s = total % 60

    return [h, m, s].map((part) => String(part).padStart(2, '0')).join(':')
}

/** `3:30`, `1:02:05` for lengths. */
export function formatLength(seconds: number): string {
    const total = Math.max(0, Math.round(seconds))
    const h = Math.floor(total / 3600)
    const m = Math.floor((total % 3600) / 60)
    const s = String(total % 60).padStart(2, '0')

    return h > 0 ? `${h}:${String(m).padStart(2, '0')}:${s}` : `${m}:${s}`
}

/** Local date and time parts of `YYYY-MM-DDTHH:MM:SS` or `YYYY-MM-DD HH:MM:SS.sss+zz:zz`. */
export function splitDateTime(value: string): { date: string; time: string } {
    return { date: value.slice(0, 10), time: value.slice(11, 19) }
}

/** Seconds of `value` counted from midnight of `day`, so times on the next day exceed 86400. */
export function secondsFromDay(value: string, day: string): number {
    const { date, time } = splitDateTime(value)
    const dayOffset = Math.round((Date.parse(`${date}T00:00:00Z`) - Date.parse(`${day}T00:00:00Z`)) / 86400000)

    return dayOffset * 86400 + timeToSeconds(time)
}

export function addDays(date: string, days: number): string {
    const value = new Date(`${date}T12:00:00Z`)
    value.setUTCDate(value.getUTCDate() + days)

    return value.toISOString().slice(0, 10)
}

export function fileName(path: string): string {
    return path.split('/').pop() ?? path
}

export function fileStem(path: string): string {
    return fileName(path).replace(/\.[^.]+$/, '')
}

export function parentFolder(path: string): string {
    const parts = path.split('/').filter(Boolean)

    return parts.length > 1 ? (parts[parts.length - 2] ?? '') : ''
}

export function useScheduleText() {
    const { t } = useI18n()

    function weekdayLetters(): string[] {
        return t('schedule.weekdayLetters').split(',')
    }

    function repeatLabel(bits: number): string {
        if (bits === 0) {
            return t('schedule.once')
        }

        if (bits === EVERY_DAY) {
            return t('schedule.everyDay')
        }

        if (bits === MONDAY_TO_FRIDAY) {
            return t('schedule.mondayToFriday')
        }

        const names = t('schedule.weekdayShort').split(',')

        return WEEKDAY_BITS.filter((bit) => bits & bit)
            .map((bit) => names[WEEKDAY_BITS.indexOf(bit)])
            .join(', ')
    }

    function entryLabel(event: Pick<ScheduleEvent, 'kind' | 'entry_mode'>): string {
        if (event.kind === 'live') {
            return event.entry_mode === 'interrupt' ? t('schedule.liveOnTime') : t('schedule.afterClip')
        }

        return event.entry_mode === 'interrupt' ? t('schedule.interrupt') : t('schedule.afterClip')
    }

    function afterLabel(event: Pick<ScheduleEvent, 'kind' | 'after_mode' | 'entry_mode'>): string {
        if (event.after_mode === 'clock') {
            return t('schedule.afterClock')
        }

        return event.entry_mode === 'interrupt' && event.kind === 'media'
            ? t('schedule.afterResume')
            : t('schedule.afterContinue')
    }

    return { weekdayLetters, repeatLabel, entryLabel, afterLabel }
}
