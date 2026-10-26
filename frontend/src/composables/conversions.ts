import { computed, onBeforeUnmount, ref, type Ref } from 'vue'

import { authFetch } from '@/composables/authFetch'
import { useAuth } from '@/stores/auth'

/** An uploaded video being converted to the output format. */
export interface Conversion {
    /** Storage path, as uploaded. */
    path: string
    /** Storage path of the result, when the extension changed. */
    converted: string | null
    state: 'queued' | 'converting' | 'done' | 'skipped' | 'failed'
    progress: number
    error: string | null
    updated: number
}

/**
 * The channel's conversions, asked for every few seconds while one waits or
 * runs. `finished` is called when one ends, so the folder can be reloaded.
 */
export function useConversions(channelId: Ref<number>, finished: () => void) {
    const authStore = useAuth()
    const list = ref<Conversion[]>([])
    let timer: ReturnType<typeof setTimeout> | undefined
    let stopped = false

    /** By storage path, the converted file's too (a WebM becomes an .mp4). */
    const byPath = computed(() => {
        const map = new Map<string, Conversion>()

        for (const item of list.value) {
            map.set(item.path.replace(/^\/+/, ''), item)

            if (item.converted) {
                map.set(item.converted.replace(/^\/+/, ''), item)
            }
        }

        return map
    })

    function running(items: Conversion[]) {
        return items.some((item) => item.state === 'queued' || item.state === 'converting')
    }

    async function poll() {
        clearTimeout(timer)

        if (stopped || !channelId.value) {
            return
        }

        try {
            const before = list.value
            const next = await authFetch<Conversion[]>(`/api/file/${channelId.value}/conversions`, {
                headers: authStore.authHeader,
            })
            const ended = next.some(
                (item) =>
                    !['queued', 'converting'].includes(item.state) &&
                    before.some((old) => old.path === item.path && ['queued', 'converting'].includes(old.state)),
            )

            list.value = next

            if (ended) {
                finished()
            }
        } catch {
            // Tried again on the next upload or visit.
        }

        if (running(list.value)) {
            timer = setTimeout(poll, 3000)
        }
    }

    onBeforeUnmount(() => {
        stopped = true
        clearTimeout(timer)
    })

    return { list, byPath, poll }
}
