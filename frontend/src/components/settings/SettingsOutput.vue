<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useClipboard } from '@vueuse/core'

import KeyValueEditor from '@/components/settings/KeyValueEditor.vue'
import RadioCards from '@/components/settings/RadioCards.vue'
import SaveBar from '@/components/settings/SaveBar.vue'
import SectionCard from '@/components/settings/SectionCard.vue'
import UnitInput from '@/components/settings/UnitInput.vue'
import { useCodecLabels } from '@/composables/codecs'
import { usePlayoutPage } from '@/composables/settingsPage'

const { t } = useI18n()
const { ready, changes, alsoSaves, restarts, saveLabel, save, discard, settings, configStore } =
    usePlayoutPage('output')
const { copy, copied } = useClipboard()
const { codecLabel, settingLabel } = useCodecLabels()

const output = computed(() => configStore.playout.output)
const channel = computed(() => configStore.channels[configStore.i])

const RESOLUTIONS = [
    [3840, 2160],
    [2560, 1440],
    [1920, 1080],
    [1280, 720],
    [1024, 576],
    [960, 540],
    [854, 480],
    [720, 576],
    [720, 480],
    [640, 360],
]
const FRAME_RATES = [23.976, 24, 25, 29.97, 30, 50, 59.94, 60]

function modeOf(name: string | undefined): 'desktop' | 'hls' | 'stream' {
    return name === 'desktop' || name === 'hls' ? name : 'stream'
}

/** Destinations this channel can use; the desktop window only shows when it is in use. */
const destinations = computed(() =>
    configStore.outputs
        .filter((item) => item.name !== 'desktop' || output.value.mode === 'desktop')
        .map((item) => ({
            value: String(item.id),
            label: t(`settings.output.mode.${modeOf(item.name)}`),
            description: t(`settings.output.modeHint.${modeOf(item.name)}`),
        })),
)

function parseOptions(value: string | null | undefined): Record<string, string> {
    try {
        return JSON.parse(value || '{}')
    } catch {
        return {}
    }
}

/** Switching the destination loads the settings saved for it. */
const outputId = computed({
    get: () => String(output.value.id),
    set: (value: string) => {
        const selected = configStore.outputs.find((item) => item.id === Number(value))

        if (!selected) {
            return
        }

        Object.assign(configStore.playout.output, {
            id: selected.id,
            mode: modeOf(selected.name),
            stream_url: selected.stream_url,
            stream_type: selected.stream_type ?? 'rtmp',
            stream_format: selected.stream_format ?? '',
            hls_playlist_name: selected.hls_playlist_name ?? 'stream',
            hls_segment_duration: selected.hls_segment_duration ?? 6,
            hls_list_size: selected.hls_list_size ?? 600,
            desktop_fullscreen: selected.desktop_fullscreen,
            width: selected.width,
            height: selected.height,
            fps: selected.fps,
            video_codec: selected.video_codec ?? 'libx264',
            video_options: parseOptions(selected.video_options),
            protocol_options: parseOptions(selected.protocol_options),
            muxer_options: parseOptions(selected.muxer_options),
            metadata_options: parseOptions(selected.metadata_options),
            audio_codec: selected.audio_codec ?? 'aac',
            audio_options: parseOptions(selected.audio_options),
            audio_bitrate: selected.audio_bitrate ?? 128,
            hls_variants: (selected.hls_variants ?? '')
                .split(';')
                .map((variant) => variant.trim())
                .filter(Boolean),
        })
    },
})

const playerUrl = computed(
    () =>
        `${window.location.protocol}//${window.location.host}/public/${channel.value?.id}/live/${output.value.hls_playlist_name || 'stream'}.m3u8`,
)

// ---------------------------------------------------------------------------
// Video

const resolution = computed({
    get: () => {
        if (customResolution.value) {
            return 'custom'
        }

        const match = RESOLUTIONS.find(([w, h]) => w === output.value.width && h === output.value.height)

        return match ? `${match[0]}x${match[1]}` : 'custom'
    },
    set: (value: string) => {
        if (value === 'custom') {
            customResolution.value = true
            return
        }

        customResolution.value = false
        const [width, height] = value.split('x').map(Number)
        output.value.width = width ?? output.value.width
        output.value.height = height ?? output.value.height
    },
})
const customResolution = ref(false)

const frameRate = computed({
    get: () =>
        FRAME_RATES.includes(Number(output.value.fps)) && !customFps.value ? String(output.value.fps) : 'custom',
    set: (value: string) => {
        customFps.value = value === 'custom'

        if (value !== 'custom') {
            output.value.fps = Number(value)
        }
    },
})
const customFps = ref(false)

const codecOptions = computed<OutputCodecOptions>(() =>
    output.value.mode === 'stream'
        ? configStore.outputCodecs[output.value.stream_type ?? 'rtmp']
        : configStore.outputCodecs.hls,
)

const videoSettings = computed(
    () => codecOptions.value.video.find((codec) => codec.name === output.value.video_codec)?.settings ?? [],
)
const audioUsesBitrate = computed(
    () => codecOptions.value.audio.find((codec) => codec.name === output.value.audio_codec)?.uses_bitrate ?? true,
)

function settingVisible(setting: EncoderSetting): boolean {
    const condition = setting.visible_when

    return !condition || output.value.video_options[condition.key] === condition.value
}

function setVideoOption(key: string, value: string) {
    output.value.video_options = { ...output.value.video_options, [key]: value }
}

/** Keep only the options the selected encoder knows, with its defaults filled in. */
function normalizeVideoOptions() {
    const options: Record<string, string> = {}

    for (const setting of videoSettings.value) {
        options[setting.key] = output.value.video_options?.[setting.key] ?? setting.default
    }

    if (JSON.stringify(options) !== JSON.stringify(output.value.video_options)) {
        output.value.video_options = options
    }
}

watch(
    () => [ready.value, output.value?.mode, output.value?.stream_type, output.value?.video_codec, codecOptions.value],
    () => {
        if (!ready.value) {
            return
        }

        const video = codecOptions.value.video
        const audio = codecOptions.value.audio

        if (video.length && !video.some((codec) => codec.name === output.value.video_codec)) {
            output.value.video_codec = video[0]?.name ?? output.value.video_codec
        }

        if (audio.length && !audio.some((codec) => codec.name === output.value.audio_codec)) {
            output.value.audio_codec = audio[0]?.name ?? output.value.audio_codec
            output.value.audio_options = {}
        }

        normalizeVideoOptions()
    },
    { immediate: true },
)

// ---------------------------------------------------------------------------
// Adaptive HLS renditions: `NAME:WIDTHxHEIGHT:VIDEO_BITRATE[:AUDIO_BITRATE]`

interface Variant {
    name: string
    size: string
    video: string
    audio: string
}

const savedVariants = ref<string[]>([])

const variants = computed<Variant[]>(() =>
    output.value.hls_variants.map((spec) => {
        const [name = '', size = '', video = '', audio = ''] = spec.split(':')

        return { name, size, video, audio }
    }),
)

function writeVariants(rows: Variant[]) {
    output.value.hls_variants = rows.map((row) =>
        [row.name.trim(), row.size.trim(), row.video.trim(), row.audio.trim()]
            .filter((part, index) => index < 3 || part)
            .join(':'),
    )
}

function updateVariant(index: number, field: keyof Variant, value: string) {
    const rows = variants.value.map((row) => ({ ...row }))
    const row = rows[index]

    if (row) {
        row[field] = value
        writeVariants(rows)
    }
}

function addVariant() {
    const used = new Set(variants.value.map((row) => row.name))
    const preset = [
        { name: '720p', size: '1280x720', video: '2500k', audio: '128k' },
        { name: '480p', size: '854x480', video: '1000k', audio: '96k' },
        { name: '360p', size: '640x360', video: '600k', audio: '64k' },
    ].find((item) => !used.has(item.name)) ?? { name: '', size: '1280x720', video: '2500k', audio: '128k' }

    writeVariants([...variants.value, preset])
}

function removeVariant(index: number) {
    const rows = [...variants.value]
    rows.splice(index, 1)
    writeVariants(rows)
}

const adaptive = computed({
    get: () => output.value.hls_variants.length > 0,
    set: (on: boolean) => {
        if (on) {
            output.value.hls_variants = savedVariants.value.length ? savedVariants.value : []

            if (!output.value.hls_variants.length) {
                addVariant()
            }
        } else {
            savedVariants.value = output.value.hls_variants
            output.value.hls_variants = []
        }
    },
})

const showAdvanced = ref(false)
</script>
<template>
    <template v-if="ready">
        <div class="flex flex-wrap items-end justify-between gap-3">
            <div>
                <h1 class="m-0 text-xl font-semibold">{{ t('settings.sections.output') }}</h1>
                <p class="mt-1 mb-0 text-muted">{{ t('settings.output.intro', { name: channel?.name }) }}</p>
            </div>
            <span v-if="restarts" class="inline-flex items-center gap-2 text-[13px] text-accent-text">
                <i class="bi bi-arrow-repeat" aria-hidden="true" />{{ t('settings.restartsOnSave') }}
            </span>
        </div>

        <SectionCard :title="t('settings.output.destination')">
            <RadioCards v-model="outputId" :options="destinations" :label="t('settings.output.destination')" />

            <div v-if="output.mode === 'hls'" class="grid grid-cols-1 sm:grid-cols-3 gap-3.5">
                <label class="field-label">
                    {{ t('settings.output.playlistName') }}
                    <input v-model.trim="output.hls_playlist_name" class="field-input font-mono" spellcheck="false" />
                </label>
                <label class="field-label">
                    {{ t('settings.output.segmentDuration') }}
                    <UnitInput v-model.number="output.hls_segment_duration" unit="s" type="number" min="1" />
                </label>
                <label class="field-label">
                    {{ t('settings.output.listSize') }}
                    <input v-model.number="output.hls_list_size" type="number" min="0" class="field-input font-mono" />
                </label>
            </div>
            <div v-if="output.mode === 'hls'" class="flex flex-col gap-1.5">
                <span class="text-xs text-muted">{{ t('settings.output.playerUrl') }}</span>
                <div
                    class="flex flex-wrap items-center gap-2 bg-field border border-line-strong rounded-field px-3 py-1.5"
                >
                    <code class="font-mono text-[13px] flex-auto break-all">{{ playerUrl }}</code>
                    <button class="btn-quiet min-h-8" @click="copy(playerUrl)">
                        {{ copied ? t('settings.copied') : t('settings.copy') }}
                    </button>
                </div>
            </div>

            <div v-if="output.mode === 'stream'" class="grid grid-cols-1 md:grid-cols-[auto_minmax(0,1fr)] gap-3.5">
                <div class="field-label">
                    {{ t('settings.output.protocol') }}
                    <div
                        role="radiogroup"
                        :aria-label="t('settings.output.protocol')"
                        class="flex border border-line-strong rounded-field overflow-hidden min-h-10"
                    >
                        <label
                            v-for="type in ['rtmp', 'srt', 'udp', 'custom'] as const"
                            :key="type"
                            class="flex items-center justify-center px-3.5 cursor-pointer text-[14px]"
                            :class="output.stream_type === type ? 'bg-base-300 text-base-content' : 'text-muted'"
                        >
                            <input v-model="output.stream_type" type="radio" :value="type" class="sr-only" />
                            {{ type === 'custom' ? t('settings.output.custom') : type.toUpperCase() }}
                        </label>
                    </div>
                </div>
                <label class="field-label">
                    {{ output.stream_type === 'custom' ? t('settings.output.target') : t('settings.output.streamUrl') }}
                    <input
                        v-model.trim="output.stream_url"
                        class="field-input font-mono text-[13px]"
                        spellcheck="false"
                        :placeholder="
                            output.stream_type === 'srt'
                                ? 'srt://server:9000?streamid=…'
                                : output.stream_type === 'udp'
                                  ? 'udp://239.0.0.1:1234'
                                  : 'rtmp://server/live/key'
                        "
                    />
                </label>
                <label v-if="output.stream_type === 'custom'" class="field-label">
                    {{ t('settings.output.format') }}
                    <input v-model.trim="output.stream_format" class="field-input font-mono" placeholder="mpegts" />
                </label>
            </div>

            <label v-if="output.mode === 'desktop'" class="flex items-center gap-3 min-h-10 cursor-pointer">
                <input v-model="output.desktop_fullscreen" type="checkbox" role="switch" class="toggle toggle-accent" />
                {{ t('settings.output.fullscreen') }}
            </label>
        </SectionCard>

        <SectionCard :title="t('settings.output.video')">
            <div class="grid grid-cols-1 sm:grid-cols-2 xl:grid-cols-4 gap-3.5">
                <label class="field-label">
                    {{ t('settings.output.resolution') }}
                    <select v-model="resolution" class="field-input">
                        <option v-for="[w, h] in RESOLUTIONS" :key="`${w}x${h}`" :value="`${w}x${h}`">
                            {{ w }} × {{ h }}
                        </option>
                        <option value="custom">{{ t('settings.output.customValue') }}</option>
                    </select>
                </label>
                <label class="field-label">
                    {{ t('settings.output.fps') }}
                    <select v-model="frameRate" class="field-input">
                        <option v-for="rate in FRAME_RATES" :key="rate" :value="String(rate)">{{ rate }}</option>
                        <option value="custom">{{ t('settings.output.customValue') }}</option>
                    </select>
                </label>
                <label v-if="output.mode !== 'desktop'" class="field-label sm:col-span-2">
                    {{ t('settings.output.encoder') }}
                    <select v-model="output.video_codec" class="field-input">
                        <option v-for="codec in codecOptions.video" :key="codec.name" :value="codec.name">
                            {{ codecLabel(codec) }}
                        </option>
                    </select>
                </label>
                <template v-if="resolution === 'custom'">
                    <label class="field-label">
                        {{ t('settings.output.width') }}
                        <UnitInput v-model.number="output.width" unit="px" type="number" min="16" step="2" />
                    </label>
                    <label class="field-label">
                        {{ t('settings.output.height') }}
                        <UnitInput v-model.number="output.height" unit="px" type="number" min="16" step="2" />
                    </label>
                </template>
                <label v-if="frameRate === 'custom'" class="field-label">
                    {{ t('settings.output.fpsCustom') }}
                    <input
                        v-model.number="output.fps"
                        type="number"
                        min="1"
                        step="0.001"
                        class="field-input font-mono"
                    />
                </label>
            </div>

            <div v-if="output.mode !== 'desktop' && videoSettings.length" class="flex flex-col gap-2">
                <span class="text-xs text-muted">{{ t('settings.output.encoderOptions') }}</span>
                <div class="grid grid-cols-1 sm:grid-cols-2 xl:grid-cols-4 gap-3.5">
                    <template v-for="setting in videoSettings" :key="setting.key">
                        <label v-if="settingVisible(setting)" class="field-label">
                            {{ settingLabel(setting) }}
                            <select
                                v-if="setting.kind === 'select'"
                                class="field-input"
                                :value="output.video_options[setting.key]"
                                @change="setVideoOption(setting.key, ($event.target as HTMLSelectElement).value)"
                            >
                                <option v-for="choice in setting.choices" :key="choice.value" :value="choice.value">
                                    {{ choice.label }}
                                </option>
                            </select>
                            <input
                                v-else
                                type="number"
                                class="field-input font-mono"
                                :min="setting.minimum ?? undefined"
                                :max="setting.maximum ?? undefined"
                                :value="output.video_options[setting.key]"
                                @input="setVideoOption(setting.key, ($event.target as HTMLInputElement).value)"
                            />
                        </label>
                    </template>
                </div>
            </div>
        </SectionCard>

        <SectionCard
            v-if="output.mode === 'hls'"
            :title="t('settings.output.adaptive')"
            :description="t('settings.output.adaptiveHint')"
        >
            <template #aside>
                <input
                    v-model="adaptive"
                    type="checkbox"
                    role="switch"
                    class="toggle toggle-accent"
                    :aria-label="t('settings.output.adaptiveToggle')"
                />
            </template>
            <template v-if="adaptive">
                <div class="relative overflow-x-auto">
                    <table class="w-full min-w-[560px] border-collapse text-[13px]">
                        <thead>
                            <tr class="text-muted text-left text-xs">
                                <th class="py-2 pe-2 font-medium">{{ t('settings.output.variantName') }}</th>
                                <th class="py-2 px-2 font-medium">{{ t('settings.output.resolution') }}</th>
                                <th class="py-2 px-2 font-medium">{{ t('settings.output.video') }}</th>
                                <th class="py-2 px-2 font-medium">{{ t('settings.output.audio') }}</th>
                                <th class="py-2 ps-2 w-10">
                                    <span class="sr-only">{{ t('settings.actions') }}</span>
                                </th>
                            </tr>
                        </thead>
                        <tbody>
                            <tr v-for="(variant, index) in variants" :key="index" class="border-t border-line">
                                <td class="py-1.5 pe-2">
                                    <input
                                        :value="variant.name"
                                        class="field-input min-h-9 font-mono text-[13px]"
                                        :aria-label="t('settings.output.variantName')"
                                        @input="updateVariant(index, 'name', ($event.target as HTMLInputElement).value)"
                                    />
                                </td>
                                <td class="py-1.5 px-2">
                                    <input
                                        :value="variant.size"
                                        class="field-input min-h-9 font-mono text-[13px]"
                                        placeholder="1280x720"
                                        :aria-label="t('settings.output.resolution')"
                                        @input="updateVariant(index, 'size', ($event.target as HTMLInputElement).value)"
                                    />
                                </td>
                                <td class="py-1.5 px-2">
                                    <input
                                        :value="variant.video"
                                        class="field-input min-h-9 font-mono text-[13px]"
                                        placeholder="2500k"
                                        :aria-label="t('settings.output.video')"
                                        @input="
                                            updateVariant(index, 'video', ($event.target as HTMLInputElement).value)
                                        "
                                    />
                                </td>
                                <td class="py-1.5 px-2">
                                    <input
                                        :value="variant.audio"
                                        class="field-input min-h-9 font-mono text-[13px]"
                                        placeholder="128k"
                                        :aria-label="t('settings.output.audio')"
                                        @input="
                                            updateVariant(index, 'audio', ($event.target as HTMLInputElement).value)
                                        "
                                    />
                                </td>
                                <td class="py-1.5 ps-2">
                                    <button
                                        class="btn-icon"
                                        :aria-label="t('settings.output.removeVariant', { name: variant.name })"
                                        @click="removeVariant(index)"
                                    >
                                        <i class="bi bi-x-lg" aria-hidden="true" />
                                    </button>
                                </td>
                            </tr>
                        </tbody>
                    </table>
                </div>
                <p class="m-0 field-hint">{{ t('settings.output.variantHint') }}</p>
                <button class="link-accent self-start min-h-8" @click="addVariant">
                    + {{ t('settings.output.addVariant') }}
                </button>
            </template>
        </SectionCard>

        <SectionCard v-if="output.mode !== 'desktop'" :title="t('settings.output.audioTitle')">
            <div class="grid grid-cols-1 sm:grid-cols-2 xl:grid-cols-3 gap-3.5">
                <label class="field-label">
                    {{ t('settings.output.encoder') }}
                    <select v-model="output.audio_codec" class="field-input" @change="output.audio_options = {}">
                        <option v-for="codec in codecOptions.audio" :key="codec.name" :value="codec.name">
                            {{ codecLabel(codec) }}
                        </option>
                    </select>
                </label>
                <label v-if="audioUsesBitrate" class="field-label">
                    {{ t('settings.output.bitrate') }}
                    <UnitInput v-model.number="output.audio_bitrate" unit="kb/s" type="number" min="16" />
                </label>
            </div>
        </SectionCard>

        <SectionCard v-if="output.mode !== 'desktop'">
            <button
                class="flex items-center gap-2 bg-transparent border-0 p-0 min-h-8 cursor-pointer font-semibold text-left"
                :aria-expanded="showAdvanced"
                @click="showAdvanced = !showAdvanced"
            >
                <i
                    class="bi text-xs"
                    :class="showAdvanced ? 'bi-chevron-down' : 'bi-chevron-right'"
                    aria-hidden="true"
                />
                {{ t('settings.output.advanced') }}
            </button>
            <div v-if="showAdvanced" class="grid grid-cols-1 lg:grid-cols-2 gap-5">
                <KeyValueEditor
                    v-model="output.audio_options"
                    :label="t('config.audioEncoderOptions')"
                    :hint="t('config.audioEncoderOptionsHelp')"
                />
                <KeyValueEditor
                    v-model="output.muxer_options"
                    :label="t('config.muxerOptions')"
                    :hint="t('config.muxerOptionsHelp')"
                />
                <KeyValueEditor
                    v-model="output.metadata_options"
                    :label="t('config.outputMetadata')"
                    :hint="t('config.outputMetadataHelp')"
                />
                <KeyValueEditor
                    v-if="output.mode === 'stream'"
                    v-model="output.protocol_options"
                    :label="t('config.protocolOptions')"
                    :hint="t('config.protocolOptionsHelp')"
                />
            </div>
        </SectionCard>

        <SaveBar
            :changes="changes"
            :also-saves="alsoSaves"
            :saving="settings.saving"
            :save-label="saveLabel"
            @save="save"
            @discard="discard"
        />
    </template>
</template>
