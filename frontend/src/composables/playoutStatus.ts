import { computed, ref, watch } from 'vue'
import { useRoute } from 'vue-router'
import { useEventSource } from '@vueuse/core'

import { useAuth } from '@/stores/auth'
import { useConfig } from '@/stores/config'
import { useIndex } from '@/stores/index'
import { usePlaylist } from '@/stores/playlist'

/** How long a channel on air must report "not running" before it shows as stopped. */
const STOPPED_AFTER_MS = 1500

/**
 * Keeps one status stream of the selected channel open while someone is
 * logged in. The header and the player both read it from the playlist store.
 */
export function usePlayoutStatusStream() {
    const authStore = useAuth()
    const configStore = useConfig()
    const indexStore = useIndex()
    const playlistStore = usePlaylist()
    const route = useRoute()

    const streamUrl = ref<string>()

    /** Loudness metering costs server time: only ask for it where it is shown. */
    const audioMeter = computed(() => route.path.startsWith('/player') || route.path.startsWith('/configure/audio'))
    let errorCounter = 0
    /** When the stream first said "not running" in a row, or 0. */
    let stoppedSince = 0

    function connect() {
        const id = configStore.channels[configStore.i]?.id

        streamUrl.value =
            authStore.isLogin && id && authStore.uuid
                ? `/data/event/${id}?endpoint=playout${audioMeter.value ? '&audio_meter=true' : ''}&uuid=${authStore.uuid}`
                : undefined
    }

    const { status, data, error } = useEventSource(streamUrl, [], {
        autoReconnect: {
            retries: -1,
            delay: 1000,
            onFailed() {
                indexStore.sseConnected = false
            },
        },
    })

    // Only a new login or another channel starts over. Every page reloads the
    // channel list (same values, new objects): comparing an array made anew
    // each time would count that as a change, blank the status ("stopped" for
    // a moment, then on air again) and reopen the stream.
    watch(
        () => `${authStore.isLogin}|${configStore.channels[configStore.i]?.id ?? ''}`,
        () => {
            playlistStore.resetStatus()
            playlistStore.playoutIsRunning = false
            playlistStore.statusKnown = false
            stoppedSince = 0
            connect()
        },
        { immediate: true },
    )

    watch(audioMeter, connect)

    // The first uuid arrives after the router guard has loaded the channels.
    watch(
        () => authStore.uuid,
        () => {
            if (!streamUrl.value) {
                connect()
            }
        },
    )

    watch([status, error], async () => {
        if (status.value === 'OPEN') {
            indexStore.sseConnected = true
            errorCounter = 0
        } else {
            indexStore.sseConnected = false
            errorCounter += 1

            // A uuid is only valid for a while; get a new one after repeated errors.
            if (errorCounter > 11 && authStore.isLogin) {
                errorCounter = 0
                await authStore.obtainUuid()
                connect()
            }
        }
    })

    watch(data, () => {
        if (!data.value || data.value === 'connected') {
            return
        }

        let status: PlayoutStatus | null = null

        try {
            status = JSON.parse(data.value) as PlayoutStatus
        } catch {
            // The stream sends "not running" while the playout is stopped.
        }

        if (status) {
            stoppedSince = 0
            playlistStore.setStatus(status)
            return
        }

        // A channel on air turns "stopped" only once it says so for a while:
        // one odd message (a restart of the stream, a phone waking up) must
        // not make the badges blink between on air and stopped.
        const now = Date.now()
        stoppedSince ||= now

        if (!playlistStore.playoutIsRunning || now - stoppedSince >= STOPPED_AFTER_MS) {
            playlistStore.playoutIsRunning = false
            playlistStore.statusKnown = true
            playlistStore.resetStatus()
        }
    })
}
