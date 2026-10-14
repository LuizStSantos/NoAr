<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import SaveBar from '@/components/settings/SaveBar.vue'
import SectionCard from '@/components/settings/SectionCard.vue'
import UnitInput from '@/components/settings/UnitInput.vue'
import { usePlayoutPage } from '@/composables/settingsPage'
import { useIndex } from '@/stores/index'
import { usePlaylist } from '@/stores/playlist'
import type { Audio } from '@/types/playout_config'

const { t, locale } = useI18n()
const indexStore = useIndex()
const playlistStore = usePlaylist()
const { ready, changes, alsoSaves, saveLabel, save, discard, settings, configStore } = usePlayoutPage('audio')

type AudioKey = keyof Audio

interface Field {
    key: AudioKey
    unit: string
    min?: number
    max?: number
    step: number
}

/** NoAr's defaults, used by "Back to defaults". */
const DEFAULTS: Partial<Record<AudioKey, number>> = {
    volume: 1,
    loudness_target_lufs: -23,
    loudness_true_peak_ceiling_dbtp: -1,
    loudness_dead_band_lu: 1,
    loudness_max_gain_db: 8,
    loudness_max_attenuation_db: -12,
    loudness_gain_up_db_per_second: 0.5,
    loudness_gain_down_db_per_second: 2,
    loudness_silence_gate_lufs: -60,
    loudness_output_max_correction_db: 3,
    loudness_output_gain_up_db_per_second: 0.1,
    loudness_output_gain_down_db_per_second: 0.25,
    compressor_threshold_dbfs: -26,
    compressor_ratio: 3,
    compressor_knee_db: 6,
    compressor_attack_ms: 5,
    compressor_hold_ms: 100,
    compressor_release_ms: 1200,
    compressor_strong_release_ms: 500,
    pause_threshold_dbfs: -55,
    pause_hold_ms: 300,
    pause_return_delay_ms: 2000,
}

const PRESETS = [
    { target: -23, ceiling: -1, key: 'ebu' },
    { target: -24, ceiling: -2, key: 'atsc' },
    { target: -16, ceiling: -1, key: 'web' },
]

const FINE: Field[] = [
    { key: 'loudness_dead_band_lu', unit: 'LU', min: 0, step: 0.1 },
    { key: 'loudness_max_gain_db', unit: 'dB', min: 0, step: 0.1 },
    { key: 'loudness_max_attenuation_db', unit: 'dB', max: 0, step: 0.1 },
    { key: 'loudness_gain_up_db_per_second', unit: 'dB/s', min: 0, step: 0.1 },
    { key: 'loudness_gain_down_db_per_second', unit: 'dB/s', min: 0, step: 0.1 },
    { key: 'loudness_silence_gate_lufs', unit: 'LUFS', step: 0.1 },
    { key: 'loudness_output_max_correction_db', unit: 'dB', min: 0, max: 12, step: 0.1 },
    { key: 'loudness_output_gain_up_db_per_second', unit: 'dB/s', min: 0, max: 5, step: 0.1 },
    { key: 'loudness_output_gain_down_db_per_second', unit: 'dB/s', min: 0, max: 5, step: 0.05 },
]

const COMPRESSOR: Field[] = [
    { key: 'compressor_threshold_dbfs', unit: 'dBFS', min: -60, max: 0, step: 1 },
    { key: 'compressor_ratio', unit: ': 1', min: 1, max: 10, step: 0.5 },
    { key: 'compressor_knee_db', unit: 'dB', min: 0, max: 24, step: 0.1 },
    { key: 'compressor_attack_ms', unit: 'ms', min: 0.1, max: 50, step: 0.1 },
    { key: 'compressor_hold_ms', unit: 'ms', min: 0, max: 2000, step: 1 },
    { key: 'compressor_release_ms', unit: 'ms', min: 10, max: 10000, step: 10 },
    { key: 'compressor_strong_release_ms', unit: 'ms', min: 10, max: 10000, step: 10 },
]

const PAUSES: Field[] = [
    { key: 'pause_threshold_dbfs', unit: 'dBFS', min: -90, max: -20, step: 1 },
    { key: 'pause_hold_ms', unit: 'ms', min: 0, max: 5000, step: 10 },
    { key: 'pause_return_delay_ms', unit: 'ms', min: 0, max: 30000, step: 100 },
]

const showFine = ref(false)
const applying = ref(false)

const audio = computed(() => configStore.playout.audio as unknown as Record<AudioKey, number>)

const volume = computed({
    get: () => Math.round(Number(configStore.playout.audio.volume) * 100),
    set: (value: number) => (configStore.playout.audio.volume = Math.min(1.5, Math.max(0, value / 100))),
})

const normalize = computed({
    get: () => configStore.playout.audio.loudness_scope !== 'off',
    set: (on: boolean) => (configStore.playout.audio.loudness_scope = on ? 'all' : 'off'),
})

const isDefault = computed(() =>
    Object.entries(DEFAULTS).every(([key, value]) => Number(audio.value[key as AudioKey]) === value),
)

function number(value: number | null | undefined, digits = 1): string {
    if (value === null || value === undefined) {
        return '—'
    }

    return `${value < 0 ? '−' : ''}${Math.abs(value).toLocaleString(locale.value, {
        minimumFractionDigits: digits,
        maximumFractionDigits: digits,
    })}`
}

function applyPreset(preset: { target: number; ceiling: number }) {
    configStore.playout.audio.loudness_target_lufs = preset.target
    configStore.playout.audio.loudness_true_peak_ceiling_dbtp = preset.ceiling
}

function resetDefaults() {
    for (const [key, value] of Object.entries(DEFAULTS)) {
        audio.value[key as AudioKey] = value
    }
}

async function applyVolume() {
    applying.value = true

    try {
        await configStore.applyAudioEffects(configStore.playout.audio.volume)
        indexStore.msgAlert('success', t('settings.audio.volumeApplied', { n: volume.value }), 2)
    } catch (error) {
        indexStore.msgAlert('error', error instanceof Error ? error.message : String(error), 3)
    } finally {
        applying.value = false
    }
}
</script>
<template>
    <template v-if="ready">
        <div class="flex flex-wrap items-end justify-between gap-3">
            <div>
                <h1 class="m-0 text-xl font-semibold">{{ t('settings.sections.audio') }}</h1>
                <p class="mt-1 mb-0 text-muted">{{ t('settings.audio.intro') }}</p>
            </div>
            <div
                v-if="playlistStore.playoutIsRunning"
                class="text-[13px] text-muted bg-base-100 border border-line rounded-field px-3 py-2"
            >
                {{ t('settings.audio.now') }}:
                <span class="font-mono text-base-content"
                    >{{ number(playlistStore.loudness?.short_term_lufs) }} LUFS</span
                >
                · {{ t('onair.peak') }}
                <span class="font-mono"
                    >{{
                        number(playlistStore.loudness?.true_peak_dbtp ?? playlistStore.audioLevel?.peak_db)
                    }}
                    dBTP</span
                >
            </div>
        </div>

        <SectionCard :title="t('settings.audio.volume')" :description="t('settings.audio.volumeHint')">
            <div class="flex flex-wrap items-center gap-4">
                <input
                    v-model.number="volume"
                    type="range"
                    min="0"
                    max="150"
                    step="1"
                    class="range range-xs range-accent flex-[1_1_240px] max-w-[420px]"
                    :aria-label="t('settings.audio.volume')"
                />
                <span class="font-mono w-12 tabular">{{ volume }}%</span>
                <button class="btn-quiet" :disabled="applying || !playlistStore.playoutIsRunning" @click="applyVolume">
                    {{ t('settings.audio.applyNow') }}
                </button>
            </div>
        </SectionCard>

        <SectionCard :title="t('settings.audio.loudness')" :description="t('settings.audio.loudnessHint')">
            <template #aside>
                <input
                    v-model="normalize"
                    type="checkbox"
                    role="switch"
                    class="toggle toggle-accent"
                    :aria-label="t('settings.audio.normalize')"
                />
            </template>
            <template v-if="normalize">
                <div class="grid grid-cols-1 sm:grid-cols-3 gap-3.5">
                    <label class="field-label">
                        {{ t('settings.audio.scope') }}
                        <select v-model="configStore.playout.audio.loudness_scope" class="field-input">
                            <option value="all">{{ t('settings.audio.scopeAll') }}</option>
                            <option value="live">{{ t('settings.audio.scopeLive') }}</option>
                        </select>
                    </label>
                    <label class="field-label">
                        {{ t('settings.audio.target') }}
                        <UnitInput
                            v-model.number="configStore.playout.audio.loudness_target_lufs"
                            unit="LUFS"
                            type="number"
                            step="0.1"
                        />
                    </label>
                    <label class="field-label">
                        {{ t('settings.audio.ceiling') }}
                        <UnitInput
                            v-model.number="configStore.playout.audio.loudness_true_peak_ceiling_dbtp"
                            unit="dBTP"
                            type="number"
                            max="0"
                            step="0.1"
                        />
                    </label>
                </div>
                <div class="flex flex-wrap items-center gap-2 text-[13px] text-muted">
                    {{ t('settings.audio.presets') }}:
                    <button
                        v-for="preset in PRESETS"
                        :key="preset.key"
                        class="btn-quiet min-h-8 text-[13px]"
                        :class="
                            configStore.playout.audio.loudness_target_lufs === preset.target &&
                            configStore.playout.audio.loudness_true_peak_ceiling_dbtp === preset.ceiling
                                ? 'border-accent! text-accent-text!'
                                : ''
                        "
                        :aria-pressed="configStore.playout.audio.loudness_target_lufs === preset.target"
                        @click="applyPreset(preset)"
                    >
                        {{ t(`settings.audio.preset.${preset.key}`) }}
                    </button>
                </div>
                <div class="flex flex-col gap-3">
                    <button
                        class="flex items-center gap-2 bg-transparent border-0 p-0 min-h-8 cursor-pointer text-muted text-left"
                        :aria-expanded="showFine"
                        @click="showFine = !showFine"
                    >
                        <i
                            class="bi text-xs"
                            :class="showFine ? 'bi-chevron-down' : 'bi-chevron-right'"
                            aria-hidden="true"
                        />
                        {{ t('settings.audio.fine') }}
                    </button>
                    <div v-if="showFine" class="grid grid-cols-1 sm:grid-cols-3 gap-3.5">
                        <label v-for="field in FINE" :key="field.key" class="field-label">
                            {{ t(`settings.audio.field.${field.key}`) }}
                            <UnitInput
                                v-model.number="audio[field.key]"
                                :unit="field.unit"
                                type="number"
                                :min="field.min"
                                :max="field.max"
                                :step="field.step"
                            />
                        </label>
                    </div>
                </div>
            </template>
        </SectionCard>

        <SectionCard
            v-if="normalize"
            :title="t('settings.audio.compressor')"
            :description="t('settings.audio.compressorHint')"
        >
            <div class="grid grid-cols-1 sm:grid-cols-3 xl:grid-cols-4 gap-3.5">
                <label v-for="field in COMPRESSOR" :key="field.key" class="field-label">
                    {{ t(`settings.audio.field.${field.key}`) }}
                    <UnitInput
                        v-model.number="audio[field.key]"
                        :unit="field.unit"
                        type="number"
                        :min="field.min"
                        :max="field.max"
                        :step="field.step"
                    />
                </label>
            </div>
        </SectionCard>

        <SectionCard v-if="normalize" :title="t('settings.audio.pauses')" :description="t('settings.audio.pausesHint')">
            <div class="grid grid-cols-1 sm:grid-cols-3 gap-3.5">
                <label v-for="field in PAUSES" :key="field.key" class="field-label">
                    {{ t(`settings.audio.field.${field.key}`) }}
                    <UnitInput
                        v-model.number="audio[field.key]"
                        :unit="field.unit"
                        type="number"
                        :min="field.min"
                        :max="field.max"
                        :step="field.step"
                    />
                </label>
            </div>
        </SectionCard>

        <SaveBar
            :changes="changes"
            :also-saves="alsoSaves"
            :saving="settings.saving"
            :save-label="saveLabel"
            :note="isDefault ? t('settings.audio.defaultsNote') : ''"
            @save="save"
            @discard="discard"
        >
            <template #extra>
                <button class="btn-quiet" :disabled="isDefault" @click="resetDefaults">
                    {{ t('settings.audio.resetDefaults') }}
                </button>
            </template>
        </SaveBar>
    </template>
</template>
