<script setup lang="ts">
import mpegts from 'mpegts.js'
import videojs from 'video.js'
import 'video.js/dist/video-js.css'

import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'

/**
 * The channel preview. `direct` is NoAr's own live MPEG-TS copy of the output
 * (about a second behind the air); otherwise the address is HLS through
 * video.js, or an FLV stream.
 */
const props = defineProps<{ src: string; muted: boolean; direct?: boolean; token?: string }>()
const emit = defineEmits<{ failed: [] }>()

/** Seconds without new video before a live stream is reopened. */
const STALL_SECONDS = 6

const video = ref<HTMLVideoElement>()
/** Played through mpegts.js (direct preview or FLV) rather than video.js. */
const streamed = computed(() => Boolean(props.direct) || isFlv(props.src))
let hlsPlayer: ReturnType<typeof videojs> | null = null
let streamPlayer: mpegts.Player | null = null
let retry: ReturnType<typeof setTimeout> | undefined
let watchdog: ReturnType<typeof setInterval> | undefined
let lastTime = 0
let stalledFor = 0

function isFlv(src: string) {
    return src.split('?')[0]?.toLowerCase().endsWith('.flv') ?? false
}

function stop() {
    clearTimeout(retry)
    clearInterval(watchdog)

    if (streamPlayer) {
        streamPlayer.unload()
        streamPlayer.detachMediaElement()
        streamPlayer.destroy()
        streamPlayer = null
    }

    if (hlsPlayer) {
        hlsPlayer.pause()
        hlsPlayer.reset()
    }
}

function scheduleRetry() {
    clearTimeout(retry)
    emit('failed')
    retry = setTimeout(start, 3000)
}

/** A live stream that stops moving (a restart, a lost connection) is reopened. */
function watchStream() {
    lastTime = 0
    stalledFor = 0
    watchdog = setInterval(() => {
        const time = video.value?.currentTime ?? 0

        stalledFor = time > lastTime ? 0 : stalledFor + 1
        lastTime = time

        if (stalledFor >= STALL_SECONDS) {
            start()
        }
    }, 1000)
}

function startStream(type: 'mpegts' | 'flv') {
    if (!video.value) {
        return
    }

    const url = new URL(props.src, window.location.href).href
    const headers: Record<string, string> = props.token ? { Authorization: `Bearer ${props.token}` } : {}

    streamPlayer = mpegts.createPlayer(
        { type, isLive: true, url },
        {
            enableStashBuffer: false,
            lazyLoad: false,
            liveBufferLatencyChasing: true,
            liveBufferLatencyMaxLatency: 1.5,
            liveBufferLatencyMinRemain: 0.3,
            headers,
        },
    )
    streamPlayer.attachMediaElement(video.value)
    streamPlayer.load()
    streamPlayer.on(mpegts.Events.ERROR, scheduleRetry)
    video.value.muted = props.muted
    void video.value.play()?.catch(() => undefined)
    watchStream()
}

function start() {
    stop()

    if (!video.value || !props.src) {
        return
    }

    const streams = mpegts.getFeatureList().mseLivePlayback

    if (props.direct && streams) {
        startStream('mpegts')
        return
    }

    if (isFlv(props.src) && streams) {
        startStream('flv')
        return
    }

    if (!hlsPlayer) {
        hlsPlayer = videojs(video.value, {
            controls: false,
            autoplay: 'muted',
            preload: 'auto',
            fill: true,
            liveui: false,
            suppressNotSupportedError: true,
        })
        hlsPlayer.on('error', scheduleRetry)
    }

    hlsPlayer.src({ src: props.src, type: 'application/x-mpegURL' })
    hlsPlayer.muted(props.muted)
    void hlsPlayer.play()?.catch(() => undefined)
}

onMounted(start)

onBeforeUnmount(() => {
    stop()
    hlsPlayer?.dispose()
    hlsPlayer = null
})

watch(
    () => [props.src, props.direct],
    () => start(),
)

watch(
    () => props.muted,
    (muted) => {
        if (hlsPlayer && !streamPlayer) {
            hlsPlayer.muted(muted)
            void hlsPlayer.play()?.catch(() => undefined)
        } else if (video.value) {
            video.value.muted = muted
        }
    },
)
</script>
<template>
    <div class="absolute inset-0">
        <!-- video.js sizes its own element; a stream player uses the plain one. -->
        <video
            ref="video"
            class="w-full h-full object-contain bg-black"
            :class="streamed ? '' : 'video-js'"
            playsinline
            muted
        />
    </div>
</template>
<style>
.video-js .vjs-tech {
    object-fit: contain;
}

.video-js .vjs-error-display,
.video-js .vjs-loading-spinner,
.video-js .vjs-big-play-button {
    display: none;
}
</style>
