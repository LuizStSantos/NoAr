<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'

import KeyValueEditor from '@/components/settings/KeyValueEditor.vue'
import RadioCards from '@/components/settings/RadioCards.vue'
import SaveBar from '@/components/settings/SaveBar.vue'
import SectionCard from '@/components/settings/SectionCard.vue'
import UnitInput from '@/components/settings/UnitInput.vue'
import { useCodecLabels } from '@/composables/codecs'
import { usePlayoutPage } from '@/composables/settingsPage'
import { usePlaylist } from '@/stores/playlist'

const { t } = useI18n()
const playlistStore = usePlaylist()
const { codecLabel, settingLabel } = useCodecLabels()
const { ready, changes, alsoSaves, saveLabel, save, discard, settings, configStore } = usePlayoutPage('recording')

const SEGMENTS = [60, 300, 600, 900, 1800, 3600]

const recording = computed(() => configStore.playout.recording)
const output = computed(() => configStore.playout.output)
const activeOutput = computed(() => configStore.outputs.find((item) => item.id === output.value.id))

/** The base HLS rendition plus the adaptive ones of the active output. */
const hlsVariants = computed(() => {
    if (output.value.mode !== 'hls') {
        return []
    }

    return [
        {
            value: output.value.hls_playlist_name || 'stream',
            label: `${output.value.hls_playlist_name || 'stream'} · ${output.value.width}×${output.value.height}`,
        },
        ...output.value.hls_variants.map((spec) => {
            const [name = '', size = '', video = ''] = spec.split(':')

            return { value: name, label: `${name} · ${size.replace('x', '×')} · ${video}` }
        }),
    ].filter((variant) => variant.value)
})

const sources = computed(() => [
    {
        value: 'stream',
        label: t('settings.recording.sourceStream'),
        description: t('settings.recording.sourceStreamHint'),
        disabled: output.value.mode !== 'stream',
    },
    {
        value: 'hls_variant',
        label: t('settings.recording.sourceHls'),
        description: t('settings.recording.sourceHlsHint'),
        disabled: output.value.mode !== 'hls',
    },
    {
        value: 'encode',
        label: t('settings.recording.sourceEncode'),
        description: t('settings.recording.sourceEncodeHint'),
    },
])

const source = computed({
    get: () => recording.value.source,
    set: (value: string) => {
        recording.value.source = value as typeof recording.value.source

        if (value !== 'encode') {
            recording.value.source_output_id = activeOutput.value?.id ?? null
        }

        if (
            value === 'hls_variant' &&
            !hlsVariants.value.some((variant) => variant.value === recording.value.variant)
        ) {
            recording.value.variant = hlsVariants.value[0]?.value ?? ''
        }
    },
})

const videoCodecs = computed(() => configStore.outputCodecs.recording.video)
const audioCodecs = computed(() => configStore.outputCodecs.recording.audio)
const videoSettings = computed(
    () => videoCodecs.value.find((codec) => codec.name === recording.value.video_codec)?.settings ?? [],
)
const audioUsesBitrate = computed(
    () => audioCodecs.value.find((codec) => codec.name === recording.value.audio_codec)?.uses_bitrate ?? true,
)

function setVideoCodec() {
    const codec = videoCodecs.value.find((item) => item.name === recording.value.video_codec)

    if (codec) {
        recording.value.video_options = Object.fromEntries(
            codec.settings.map((setting) => [setting.key, setting.default]),
        )
    }
}

function settingVisible(setting: EncoderSetting) {
    const condition = setting.visible_when

    return !condition || recording.value.video_options[condition.key] === condition.value
}

function setVideoOption(key: string, value: string) {
    recording.value.video_options = { ...recording.value.video_options, [key]: value }
}

function segmentLabel(seconds: number) {
    return seconds >= 3600
        ? t('settings.recording.hours', seconds / 3600)
        : t('settings.recording.minutes', seconds / 60)
}

const recordingNow = computed(() => recording.value?.enable && playlistStore.playoutIsRunning)
</script>
<template>
    <template v-if="ready">
        <div class="flex flex-wrap items-end justify-between gap-3">
            <div>
                <h1 class="m-0 text-xl font-semibold">{{ t('settings.sections.recording') }}</h1>
                <p class="mt-1 mb-0 text-muted max-w-2xl">{{ t('settings.recording.intro') }}</p>
            </div>
            <label
                class="flex items-center gap-3 min-h-11 bg-base-100 border border-line rounded-field px-3.5 cursor-pointer"
            >
                {{ t('settings.recording.enable') }}
                <input v-model="recording.enable" type="checkbox" role="switch" class="toggle toggle-accent" />
            </label>
        </div>

        <section
            :aria-label="t('settings.recording.state')"
            class="card-box px-4.5 py-3.5 flex flex-wrap items-center gap-x-5 gap-y-2 text-[13px]"
        >
            <span v-if="recordingNow" class="inline-flex items-center gap-2 font-semibold text-on-air-text">
                <span class="dot bg-[#ff5a5f]" aria-hidden="true" />{{ t('settings.recording.recording') }}
            </span>
            <span v-else class="inline-flex items-center gap-2 text-muted">
                <span class="dot bg-subtle" aria-hidden="true" />{{
                    recording.enable ? t('settings.recording.waiting') : t('settings.recording.off')
                }}
            </span>
            <span class="font-mono text-muted break-all">{{ recording.path }}</span>
        </section>

        <SectionCard :title="t('settings.recording.what')">
            <RadioCards v-model="source" :options="sources" :label="t('settings.recording.what')" />
            <p v-if="sources.find((item) => item.value === source)?.disabled" class="m-0 text-[13px] text-accent-text">
                <i class="bi bi-exclamation-triangle me-1" aria-hidden="true" />{{ t('settings.recording.sourceMismatch') }}
            </p>
            <div v-if="source === 'hls_variant'" class="grid grid-cols-1 sm:grid-cols-2 gap-3.5">
                <label class="field-label">
                    {{ t('settings.recording.variant') }}
                    <select v-model="recording.variant" class="field-input">
                        <option v-for="variant in hlsVariants" :key="variant.value" :value="variant.value">
                            {{ variant.label }}
                        </option>
                    </select>
                </label>
            </div>
            <template v-if="source === 'encode'">
                <div class="grid grid-cols-1 sm:grid-cols-2 xl:grid-cols-4 gap-3.5">
                    <label class="field-label">
                        {{ t('settings.output.width') }}
                        <UnitInput v-model.number="recording.width" unit="px" type="number" min="0" step="2" />
                    </label>
                    <label class="field-label">
                        {{ t('settings.output.height') }}
                        <UnitInput v-model.number="recording.height" unit="px" type="number" min="0" step="2" />
                    </label>
                    <label class="field-label">
                        {{ t('settings.recording.videoCodec') }}
                        <select v-model="recording.video_codec" class="field-input" @change="setVideoCodec">
                            <option v-for="codec in videoCodecs" :key="codec.name" :value="codec.name">
                                {{ codecLabel(codec) }}
                            </option>
                        </select>
                    </label>
                    <label class="field-label">
                        {{ t('settings.recording.audioCodec') }}
                        <select
                            v-model="recording.audio_codec"
                            class="field-input"
                            @change="recording.audio_options = {}"
                        >
                            <option v-for="codec in audioCodecs" :key="codec.name" :value="codec.name">
                                {{ codecLabel(codec) }}
                            </option>
                        </select>
                    </label>
                    <template v-for="setting in videoSettings" :key="setting.key">
                        <label v-if="settingVisible(setting)" class="field-label">
                            {{ settingLabel(setting) }}
                            <select
                                v-if="setting.kind === 'select'"
                                class="field-input"
                                :value="recording.video_options[setting.key]"
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
                                :value="recording.video_options[setting.key]"
                                @input="setVideoOption(setting.key, ($event.target as HTMLInputElement).value)"
                            />
                        </label>
                    </template>
                    <label v-if="audioUsesBitrate" class="field-label">
                        {{ t('settings.output.bitrate') }}
                        <UnitInput v-model.number="recording.audio_bitrate" unit="kb/s" type="number" min="16" />
                    </label>
                </div>
                <KeyValueEditor
                    v-model="recording.audio_options"
                    :label="t('config.audioEncoderOptions')"
                    :hint="t('config.audioEncoderOptionsHelp')"
                />
            </template>
        </SectionCard>

        <SectionCard :title="t('settings.recording.files')">
            <label class="field-label max-w-2xl">
                {{ t('settings.recording.folder') }}
                <input v-model.trim="recording.path" class="field-input font-mono text-[13px]" spellcheck="false" />
                <span class="field-hint">{{ t('settings.recording.folderHint') }}</span>
            </label>
            <div class="grid grid-cols-1 sm:grid-cols-3 gap-3.5">
                <label class="field-label">
                    {{ t('settings.recording.segment') }}
                    <select v-model.number="recording.segment_duration" class="field-input">
                        <option v-for="segment in SEGMENTS" :key="segment" :value="segment">
                            {{ segmentLabel(segment) }}
                        </option>
                        <option
                            v-if="!SEGMENTS.includes(recording.segment_duration)"
                            :value="recording.segment_duration"
                        >
                            {{ recording.segment_duration }} s
                        </option>
                    </select>
                </label>
                <label class="field-label">
                    {{ t('settings.recording.retention') }}
                    <UnitInput
                        v-model.number="recording.retention_days"
                        :unit="t('settings.recording.days')"
                        type="number"
                        min="0"
                    />
                    <span class="field-hint">{{ t('settings.recording.retentionHint') }}</span>
                </label>
                <label class="field-label">
                    {{ t('settings.recording.freeSpace') }}
                    <UnitInput v-model.number="recording.minimum_free_space_gb" unit="GB" type="number" min="0" />
                    <span class="field-hint">{{ t('settings.recording.freeSpaceHint') }}</span>
                </label>
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
