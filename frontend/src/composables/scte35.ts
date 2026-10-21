import { computed, onMounted, onUnmounted, ref, watch, type Ref } from 'vue'

import { authFetch } from '@/composables/authFetch'
import { useAuth } from '@/stores/auth'
import { useConfig } from '@/stores/config'

export type BreakState = 'scheduled' | 'announced' | 'on_air' | 'ended' | 'cancelled'

export interface BreakStatus {
    event_id: number
    source: 'commercials' | 'manual'
    state: BreakState
    /** Seconds. */
    duration: number
    /** Unix milliseconds. */
    starts_at: number
    ends_at: number
    out_messages: number
    in_messages: number
    out_key_frame: boolean | null
    in_key_frame: boolean | null
}

export interface Scte35Status {
    /** On in the settings. */
    enabled: boolean
    /** The output on air carries it. */
    active: boolean
    pid: number
    preroll: number
    min_break: number
    max_break: number
    error: string | null
    breaks: BreakStatus[]
    recent: BreakStatus[]
}

/** Messages sent per splice point. */
export const SCTE35_COPIES = 3

/** The SCTE-35 breaks of a channel, kept up to date while the page is open. */
export function useScte35(channelId: Ref<number | undefined>) {
    const authStore = useAuth()
    const configStore = useConfig()

    const status = ref<Scte35Status | null>(null)
    const now = ref(Date.now())
    const busy = ref(false)
    let timer: ReturnType<typeof setInterval> | undefined

    const onAir = computed(() => status.value?.breaks.find((item) => item.state === 'on_air') ?? null)
    const next = computed(
        () => status.value?.breaks.find((item) => item.state === 'scheduled' || item.state === 'announced') ?? null,
    )

    function headers() {
        return { ...configStore.contentType, ...authStore.authHeader }
    }

    async function load() {
        if (!channelId.value) {
            return
        }

        try {
            status.value = await authFetch<Scte35Status>(`/api/control/${channelId.value}/scte35`, {
                headers: headers(),
            })
        } catch {
            status.value = null
        }
    }

    async function insertBreak(duration: number) {
        busy.value = true

        try {
            await authFetch(`/api/control/${channelId.value}/scte35/break`, {
                method: 'POST',
                headers: headers(),
                body: JSON.stringify({ duration }),
            })
            await load()
        } finally {
            busy.value = false
        }
    }

    async function cancelBreak(eventId: number) {
        busy.value = true

        try {
            status.value = await authFetch<Scte35Status>(`/api/control/${channelId.value}/scte35/break/${eventId}`, {
                method: 'DELETE',
                headers: headers(),
            })
        } finally {
            busy.value = false
        }
    }

    onMounted(() => {
        load()
        timer = setInterval(() => {
            now.value = Date.now()

            if (document.visibilityState === 'visible') {
                load()
            }
        }, 1000)
    })

    onUnmounted(() => clearInterval(timer))
    watch(channelId, load)

    return { status, now, busy, onAir, next, load, insertBreak, cancelBreak }
}

/** Seconds from `now` to `at` (Unix ms), never negative. */
export function secondsUntil(at: number, now: number): number {
    return Math.max(0, Math.round((at - now) / 1000))
}

/** `m:ss`, or `h:mm:ss` from an hour. */
export function clockLength(seconds: number): string {
    const total = Math.max(0, Math.round(seconds))
    const hours = Math.floor(total / 3600)
    const minutes = Math.floor((total % 3600) / 60)
    const rest = String(total % 60).padStart(2, '0')

    return hours ? `${hours}:${String(minutes).padStart(2, '0')}:${rest}` : `${minutes}:${rest}`
}
