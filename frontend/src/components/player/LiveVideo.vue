<script setup lang="ts">
import mpegts from 'mpegts.js'
import videojs from 'video.js'
import 'video.js/dist/video-js.css'

import { onBeforeUnmount, onMounted, ref, watch } from 'vue'

/** The channel preview: HLS through video.js, or an FLV stream through mpegts.js. */
const props = defineProps<{ src: string; muted: boolean }>()
const emit = defineEmits<{ failed: [] }>()

const video = ref<HTMLVideoElement>()
let hlsPlayer: ReturnType<typeof videojs> | null = null
let flvPlayer: mpegts.Player | null = null
let retry: ReturnType<typeof setTimeout> | undefined

function isFlv(src: string) {
    return src.split('?')[0]?.toLowerCase().endsWith('.flv') ?? false
}

function stop() {
    clearTimeout(retry)

    if (flvPlayer) {
        flvPlayer.unload()
        flvPlayer.detachMediaElement()
        flvPlayer.destroy()
        flvPlayer = null
    }

    if (hlsPlayer) {
        hlsPlayer.pause()
        hlsPlayer.reset()
    }
}

function scheduleRetry() {
    clearTimeout(retry)
    emit('failed')
    retry = setTimeout(start, 4000)
}

function start() {
    stop()

    if (!video.value || !props.src) {
        return
    }

    if (isFlv(props.src) && mpegts.getFeatureList().mseLivePlayback) {
        flvPlayer = mpegts.createPlayer(
            { type: 'flv', isLive: true, url: props.src },
            { lazyLoadMaxDuration: 3 * 60, liveBufferLatencyChasing: true },
        )
        flvPlayer.attachMediaElement(video.value)
        flvPlayer.load()
        flvPlayer.on(mpegts.Events.ERROR, scheduleRetry)
        video.value.muted = props.muted
        void video.value.play()?.catch(() => undefined)
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
    () => props.src,
    () => start(),
)

watch(
    () => props.muted,
    (muted) => {
        if (hlsPlayer) {
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
        <video ref="video" class="video-js w-full h-full object-contain bg-black" playsinline muted />
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
