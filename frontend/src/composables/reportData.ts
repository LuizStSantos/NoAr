import { computed, onBeforeUnmount, onMounted, ref, watch, type ComputedRef } from 'vue'

import { authFetch } from '@/composables/authFetch'

interface ReportDataOptions {
    channelId: () => number | undefined
    range: ComputedRef<[string, string]>
    rangeError: ComputedRef<string>
    onError: (error: unknown) => void
}

/** How often a range including the current time is refreshed. */
const REFRESH_SECONDS = 60

/** Own report requests and polling; filters and exports remain in the view. */
export function useReportData(options: ReportDataOptions) {
    const report = ref<Report | null>(null)
    const loading = ref(false)
    let timer: ReturnType<typeof setInterval> | undefined
    // A later range/channel request owns the result; stale responses are ignored.
    let request = 0

    const bounds = computed<[number, number]>(() => {
        const days = report.value?.days ?? []

        return [days[0]?.start ?? 0, days[days.length - 1]?.end ?? 0]
    })
    const includesNow = computed(() => {
        const now = report.value?.now ?? Date.now()

        return now >= bounds.value[0] && now < bounds.value[1]
    })

    async function load(quiet = false) {
        if (!options.channelId() || options.rangeError.value) {
            return
        }

        const ticket = ++request
        const [first, last] = options.range.value

        loading.value = !quiet

        try {
            const query = new URLSearchParams({ from: first, to: last })
            const data = await authFetch<Report>(`/api/report/${options.channelId()}?${query}`)

            if (ticket === request) {
                report.value = data
            }
        } catch (error) {
            if (ticket === request && !quiet) {
                report.value = null
                options.onError(error)
            }
        } finally {
            if (ticket === request) {
                loading.value = false
            }
        }
    }

    watch([options.range, options.channelId], () => load())

    onMounted(() => {
        load()
        timer = setInterval(() => {
            if (includesNow.value && !document.hidden) {
                load(true)
            }
        }, REFRESH_SECONDS * 1000)
    })

    onBeforeUnmount(() => clearInterval(timer))

    return { report, loading, bounds, includesNow }
}
