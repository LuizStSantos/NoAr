import { onMounted, onBeforeUnmount, ref, watch } from 'vue'
import { useConfig } from '@/stores/config'
import { type TemplateLayer, useGraphicsApi } from '@/composables/graphicsTemplates'

export const CLOCK_FORMATS = ['HH:mm:ss', 'HH:mm', 'hh:mm:ss A', 'DD/MM/YYYY', 'DD/MM/YYYY HH:mm:ss'] as const
interface Snapshot {
    unix: number
    at: number
    timezone: string | null
    offset: number
    fetched: number
}
const snapshots = new Map<number, Snapshot>()
const pending = new Map<number, Promise<void>>()

/** Estimate server time with a monotonic browser clock, independently of the user's wall clock. */
export function useGraphicsClock() {
    const config = useConfig()
    const api = useGraphicsApi()
    const tick = ref(performance.now())
    let timer: ReturnType<typeof setInterval> | undefined
    let alive = true

    async function refresh() {
        const id = config.channels[config.i]?.id
        if (!id) {
            return
        }
        const current = snapshots.get(id)
        if (current && performance.now() - current.fetched < 30000) {
            return
        }
        let request = pending.get(id)
        if (!request) {
            const start = performance.now()
            request = api
                .clock()
                .then((clock) => {
                    const end = performance.now()
                    snapshots.set(id, {
                        unix: clock.unix_ms + (end - start) / 2,
                        at: end,
                        timezone: clock.timezone,
                        offset: clock.offset_seconds * 1000,
                        fetched: end,
                    })
                })
                .catch(() => {
                    /* Keep the last server sample during a transient read failure. */
                })
                .finally(() => pending.delete(id))
            pending.set(id, request)
        }
        await request
        if (alive) {
            tick.value = performance.now()
        }
    }

    onMounted(() => {
        void refresh()
        timer = setInterval(() => {
            tick.value = performance.now()
            void refresh()
        }, 250)
    })
    onBeforeUnmount(() => {
        alive = false
        clearInterval(timer)
    })
    watch(
        () => config.channels[config.i]?.id,
        () => {
            void refresh()
        },
    )

    function clockValue(layer: TemplateLayer): string {
        const id = config.channels[config.i]?.id
        const sample = id ? snapshots.get(id) : undefined
        if (!sample) {
            return '—'
        }
        const zone = layer.clock_timezone || sample.timezone
        const time = sample.unix + (tick.value - sample.at) + (zone ? 0 : sample.offset)
        try {
            const parts = new Intl.DateTimeFormat('en-GB', {
                timeZone: zone || 'UTC',
                year: 'numeric',
                month: '2-digit',
                day: '2-digit',
                hour: '2-digit',
                minute: '2-digit',
                second: '2-digit',
                hourCycle: 'h23',
            }).formatToParts(new Date(time))
            const part = (name: string) => parts.find((item) => item.type === name)?.value || ''
            const hour = Number(part('hour'))
            const format = layer.clock_format || 'HH:mm:ss'
            if (format === 'HH:mm') {
                return part('hour') + ':' + part('minute')
            }
            if (format === 'hh:mm:ss A') {
                return (
                    String(hour % 12 || 12).padStart(2, '0') +
                    ':' +
                    part('minute') +
                    ':' +
                    part('second') +
                    ' ' +
                    (hour < 12 ? 'AM' : 'PM')
                )
            }
            const date = part('day') + '/' + part('month') + '/' + part('year')
            if (format === 'DD/MM/YYYY') {
                return date
            }
            const clock = part('hour') + ':' + part('minute') + ':' + part('second')
            return format === 'DD/MM/YYYY HH:mm:ss' ? date + ' ' + clock : clock
        } catch {
            return '—'
        }
    }

    return { clockValue }
}
