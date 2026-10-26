<script setup lang="ts">
import mpegts from 'mpegts.js'
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import LiveVideo from '@/components/player/LiveVideo.vue'
import { useOnAir } from '@/composables/onAir'
import { storageLabel, uptimeLabel } from '@/composables/player'
import { useAuth } from '@/stores/auth'
import { useConfig } from '@/stores/config'
import { usePlaylist } from '@/stores/playlist'
import { useSchedule } from '@/stores/schedule'

const { t } = useI18n()
const authStore = useAuth()
const configStore = useConfig()
const playlistStore = usePlaylist()
const scheduleStore = useSchedule()
const { currentTitle } = useOnAir()

const muted = ref(true)

const channel = computed(() => configStore.channels[configStore.i])
/** Optional outside address (public HLS, CDN) set in the channel settings. */
const addressUrl = computed(() => channel.value?.preview_url?.trim() ?? '')
const directUrl = computed(() => (channel.value ? `/api/preview/${channel.value.id}` : ''))

type PreviewSource = 'direct' | 'address'

function savedSource(id: number | undefined): PreviewSource {
    try {
        return localStorage.getItem(`noar.preview.${id}`) === 'address' ? 'address' : 'direct'
    } catch {
        return 'direct'
    }
}

const source = ref<PreviewSource>(savedSource(channel.value?.id))
const useAddress = computed(() => source.value === 'address' && addressUrl.value !== '')
/** Browsers without live Media Source playback (older iPhones) cannot play the direct stream. */
const directSupported = mpegts.getFeatureList().mseLivePlayback
const direct = computed(() => !useAddress.value && directSupported && configStore.playout.output?.mode !== 'desktop')
/** NoAr's own HLS output, the fallback where the direct stream does not play. */
const hlsUrl = computed(() => {
    const output = configStore.playout.output

    if (!channel.value || output?.mode !== 'hls') {
        return ''
    }

    return `/public/${channel.value.id}/live/${output.hls_playlist_name?.trim() || 'stream'}.m3u8`
})
/** Live sources only have something to show while the playout runs. */
const previewUrl = computed(() => {
    if (useAddress.value) {
        return addressUrl.value
    }

    if (!playlistStore.playoutIsRunning) {
        return ''
    }

    if (direct.value) {
        return directUrl.value
    }

    return directSupported ? '' : addressUrl.value || hlsUrl.value
})
const placeholder = computed(() => {
    if (configStore.playout.output?.mode === 'desktop') {
        return t('onair.noPreview')
    }

    if (!playlistStore.playoutIsRunning) {
        return t('onair.previewWhenRunning')
    }

    return t('onair.previewUnsupported')
})

/** No picture yet after this long: say what the preview waits for. */
const WAIT_NOTICE_MS = 5000

const previewHasPicture = ref(false)
const previewWaiting = ref(false)
const previewError = ref('')
let waitTimer: ReturnType<typeof setTimeout> | undefined

/** An SRT output in listener mode only starts once a receiver connects. */
const srtListener = computed(() => {
    const output = configStore.playout.output

    return (
        output?.mode === 'stream' && output.stream_type === 'srt' && /[?&]mode=listener\b/.test(output.stream_url ?? '')
    )
})

const previewNotice = computed(() => {
    if (previewHasPicture.value) {
        return ''
    }

    if (previewError.value) {
        return t('onair.previewFailed', { reason: previewError.value })
    }

    if (previewWaiting.value) {
        return srtListener.value ? t('onair.previewWaitingSrtListener') : t('onair.previewWaiting')
    }

    return ''
})

function resetPreviewState() {
    clearTimeout(waitTimer)
    previewHasPicture.value = false
    previewWaiting.value = false
    previewError.value = ''
    waitTimer = setTimeout(() => (previewWaiting.value = true), WAIT_NOTICE_MS)
}

function previewPlaying() {
    clearTimeout(waitTimer)
    previewHasPicture.value = true
    previewError.value = ''
}

function previewFailed(reason: string) {
    previewHasPicture.value = false
    previewError.value = reason || t('onair.previewNoReason')
}

watch(() => `${source.value}:${previewUrl.value}`, resetPreviewState, { immediate: true })
onBeforeUnmount(() => clearTimeout(waitTimer))

function chooseSource(value: PreviewSource) {
    source.value = value

    try {
        localStorage.setItem(`noar.preview.${channel.value?.id}`, value)
    } catch {
        // Only remembered for this visit.
    }
}

watch(
    () => channel.value?.id,
    (id) => (source.value = savedSource(id)),
)

/** What feeds the output right now. */
const sourceLabel = computed(() => {
    const active = scheduleStore.status.active

    if (active) {
        return active.kind === 'live' ? t('onair.sourceScheduledLive') : t('onair.sourceScheduled')
    }

    if (playlistStore.ingestRuns) {
        return playlistStore.ingestListenerName
            ? t('onair.sourceIngestNamed', { name: playlistStore.ingestListenerName })
            : t('onair.sourceIngest')
    }

    return configStore.playout.processing?.mode === 'folder' ? t('onair.sourceFolder') : t('onair.sourcePlaylist')
})

/** `HLS · 1080p50 · 4,5 Mb/s` from the output settings. */
const outputLabel = computed(() => {
    const output = configStore.playout.output

    if (!output) {
        return ''
    }

    const kind = output.mode === 'hls' ? 'HLS' : output.mode === 'stream' ? output.stream_type.toUpperCase() : 'Desktop'
    const fps = Number.isInteger(output.fps) ? output.fps : output.fps.toFixed(2)
    const parts = [kind, `${output.height}p${fps}`]
    const bitrate = output.video_options?.['b:v'] ?? output.video_options?.maxrate

    if (bitrate) {
        const match = /^(\d+(?:\.\d+)?)([kKmM]?)$/.exec(bitrate.trim())

        if (match) {
            // Encoder settings give plain numbers in kb/s.
            const value = Number(match[1]) * (match[2]?.toLowerCase() === 'm' ? 1 : 0.001)

            parts.push(`${value.toLocaleString(undefined, { maximumFractionDigits: 1 })} Mb/s`)
        }
    }

    return parts.join(' · ')
})

const clipLabel = computed(() => {
    const source = scheduleStore.status.active?.item || playlistStore.current.source || ''

    return playlistStore.ingestRuns ? '' : storageLabel(source, channel.value?.storage ?? '')
})

const title = computed(() => {
    if (scheduleStore.status.active) {
        return scheduleStore.status.active.title
    }

    if (playlistStore.ingestRuns) {
        return playlistStore.ingestListenerName || t('onair.sourceIngest')
    }

    return playlistStore.playoutIsRunning ? currentTitle.value : t('onair.notRunning')
})

const listeners = computed(() => configStore.playout.ingest?.listeners?.filter((input) => input.enabled) ?? [])
const xmltvUrl = computed(() => (channel.value ? `/xmltv/${channel.value.id}/guide.xml` : ''))
</script>
<template>
    <section :aria-label="t('onair.output')" class="flex flex-col gap-2.5">
        <div class="relative aspect-video bg-[#0b0c0d] rounded-box overflow-hidden border border-line">
            <LiveVideo
                v-if="previewUrl"
                :key="`${source}:${previewUrl}`"
                :src="previewUrl"
                :direct="direct"
                :token="authStore.jwtToken"
                :muted="muted"
                @playing="previewPlaying"
                @failed="previewFailed"
            />
            <div
                v-if="previewUrl && previewNotice"
                role="status"
                class="absolute inset-0 flex flex-col items-center justify-center gap-2 text-[#c9c7c3] text-sm p-6 text-center pointer-events-none"
            >
                <i class="bi bi-hourglass-split text-2xl" aria-hidden="true" />
                <span class="max-w-[46ch]">{{ previewNotice }}</span>
            </div>
            <div
                v-else
                class="absolute inset-0 flex flex-col items-center justify-center gap-2 text-[#8e8c88] text-sm p-6 text-center"
            >
                <i class="bi bi-camera-video-off text-2xl" aria-hidden="true" />
                {{ placeholder }}
            </div>

            <div class="absolute top-3.5 left-3.5 flex gap-2">
                <span
                    v-if="playlistStore.playoutIsRunning"
                    class="bg-on-air text-white font-semibold text-xs tracking-wider px-2.5 py-1 rounded-md"
                >
                    ● {{ t('nav.onAir') }}
                </span>
                <span
                    v-else
                    class="bg-[#3c3e44] text-[#e8e6e3] font-semibold text-xs tracking-wider px-2.5 py-1 rounded-md"
                >
                    {{ t('nav.stopped') }}
                </span>
                <span class="bg-[rgba(23,24,26,0.86)] text-[#e8e6e3] text-xs px-2.5 py-1 rounded-md">{{
                    sourceLabel
                }}</span>
            </div>
            <div class="absolute top-3.5 right-3.5 flex items-center gap-2">
                <span
                    v-if="outputLabel"
                    class="bg-[rgba(23,24,26,0.86)] text-[#c9c7c3] font-mono text-xs px-2.5 py-1 rounded-md hidden sm:inline"
                >
                    {{ outputLabel }}
                </span>
                <div
                    v-if="addressUrl"
                    role="radiogroup"
                    :aria-label="t('onair.previewSource')"
                    class="flex bg-[rgba(23,24,26,0.86)] rounded-md p-0.5 text-xs"
                >
                    <button
                        v-for="option in ['direct', 'address'] as const"
                        :key="option"
                        type="button"
                        role="radio"
                        :aria-checked="source === option"
                        :title="option === 'direct' ? t('onair.previewDirectHint') : addressUrl"
                        class="px-2 py-0.5 rounded cursor-pointer"
                        :class="source === option ? 'bg-[#3c3e44] text-[#e8e6e3]' : 'text-[#a9a7a3]'"
                        @click="chooseSource(option)"
                    >
                        {{ option === 'direct' ? t('onair.previewDirect') : t('onair.previewAddress') }}
                    </button>
                </div>
            </div>

            <div
                class="absolute inset-x-0 bottom-0 bg-[rgba(11,12,13,0.82)] px-4 py-3 flex items-center gap-3.5 text-[#e8e6e3]"
            >
                <button
                    class="bg-[#2c2e33] rounded-full w-10 h-10 flex-none flex items-center justify-center cursor-pointer disabled:opacity-40"
                    :aria-label="muted ? t('onair.listen') : t('onair.mute')"
                    :aria-pressed="!muted"
                    :disabled="!previewUrl"
                    @click="muted = !muted"
                >
                    <i class="bi" :class="muted ? 'bi-volume-mute' : 'bi-volume-up'" aria-hidden="true" />
                </button>
                <div class="min-w-0">
                    <div class="font-semibold text-[15px] truncate">{{ title }}</div>
                    <div v-if="clipLabel" class="text-[#a9a7a3] text-[13px] truncate">{{ clipLabel }}</div>
                </div>
            </div>
        </div>

        <div class="flex flex-wrap gap-x-4.5 gap-y-2 text-muted text-[13px]">
            <span v-if="playlistStore.playoutIsRunning" class="inline-flex items-center gap-1.5">
                <span class="dot bg-ok" aria-hidden="true" />
                {{
                    playlistStore.uptime !== null
                        ? t('onair.runningFor', { time: uptimeLabel(playlistStore.uptime) })
                        : t('onair.running')
                }}
            </span>
            <span v-else class="inline-flex items-center gap-1.5">
                <span class="dot bg-subtle" aria-hidden="true" />{{ t('onair.notRunning') }}
            </span>
            <span v-if="playlistStore.ingestRuns" class="text-on-air-text">
                {{
                    t('onair.ingestOnAir', {
                        name: playlistStore.ingestListenerName || playlistStore.ingestListenerBackend || '',
                    })
                }}
            </span>
            <span v-else-if="listeners.length">{{ t('onair.ingestWaiting', listeners.length) }}</span>
            <span v-else>{{ t('onair.ingestNone') }}</span>
            <a v-if="xmltvUrl" :href="xmltvUrl" target="_blank" rel="noopener" class="link-accent">
                {{ t('onair.xmltvLink') }}
            </a>
        </div>
    </section>
</template>
