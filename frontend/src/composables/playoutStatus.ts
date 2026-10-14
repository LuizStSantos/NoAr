import { computed, ref, watch } from 'vue'
import { useRoute } from 'vue-router'
import { useEventSource } from '@vueuse/core'

import { useAuth } from '@/stores/auth'
import { useConfig } from '@/stores/config'
import { useIndex } from '@/stores/index'
import { usePlaylist } from '@/stores/playlist'

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

    watch(
        () => [authStore.isLogin, configStore.channels[configStore.i]?.id],
        () => {
            playlistStore.resetStatus()
            playlistStore.playoutIsRunning = false
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

        try {
            playlistStore.setStatus(JSON.parse(data.value) as PlayoutStatus)
        } catch {
            // The stream sends "not running" while the playout is stopped.
            playlistStore.playoutIsRunning = false
            playlistStore.resetStatus()
        }
    })
}
