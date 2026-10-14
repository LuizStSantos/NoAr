<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'

import SaveBar from '@/components/settings/SaveBar.vue'
import SectionCard from '@/components/settings/SectionCard.vue'
import ToggleRow from '@/components/settings/ToggleRow.vue'
import UnitInput from '@/components/settings/UnitInput.vue'
import { usePlayoutPage } from '@/composables/settingsPage'

const { t } = useI18n()
const { ready, changes, alsoSaves, saveLabel, save, discard, settings, configStore } = usePlayoutPage('advanced')

const LOG_LEVELS = ['INFO', 'WARNING', 'ERROR']

const logging = computed(() => configStore.playout.logging)
const ignoreLines = computed(() => logging.value.ignore_lines)

function updateLine(index: number, value: string) {
    logging.value.ignore_lines = ignoreLines.value.map((line, position) => (position === index ? value : line))
}

function addLine() {
    logging.value.ignore_lines = [...ignoreLines.value, '']
}

function removeLine(index: number) {
    logging.value.ignore_lines = ignoreLines.value.filter((_, position) => position !== index)
}
</script>
<template>
    <template v-if="ready">
        <div>
            <h1 class="m-0 text-xl font-semibold">{{ t('settings.sections.advanced') }}</h1>
            <p class="mt-1 mb-0 text-muted">{{ t('settings.advanced.intro') }}</p>
        </div>

        <SectionCard :title="t('settings.advanced.sync')" :description="t('settings.advanced.syncHint')">
            <label class="field-label max-w-[200px]">
                {{ t('settings.advanced.limit') }}
                <UnitInput v-model.number="configStore.playout.general.stop_threshold" unit="s" type="number" min="3" />
            </label>
        </SectionCard>

        <SectionCard :title="t('settings.advanced.log')" :description="t('settings.advanced.logHint')">
            <div class="grid grid-cols-1 sm:grid-cols-2 gap-3.5 max-w-2xl">
                <label class="field-label">
                    {{ t('settings.advanced.ffmpegLevel') }}
                    <select v-model="logging.ffmpeg_level" class="field-input">
                        <option v-for="level in LOG_LEVELS" :key="level" :value="level">
                            {{ t(`settings.logLevels.${level}`) }}
                        </option>
                    </select>
                </label>
                <label class="field-label">
                    {{ t('settings.advanced.ingestLevel') }}
                    <select v-model="logging.ingest_level" class="field-input">
                        <option v-for="level in LOG_LEVELS" :key="level" :value="level">
                            {{ t(`settings.logLevels.${level}`) }}
                        </option>
                    </select>
                </label>
            </div>
            <ToggleRow
                v-model="logging.detect_silence"
                :label="t('settings.advanced.silence')"
                :hint="t('settings.advanced.silenceHint')"
            />
            <div class="flex flex-col gap-2 max-w-2xl">
                <span class="text-xs text-muted">{{ t('settings.advanced.ignore') }}</span>
                <div v-for="(line, index) in ignoreLines" :key="index" class="flex gap-2">
                    <input
                        :value="line"
                        class="field-input font-mono text-[13px] min-h-9.5"
                        :aria-label="t('settings.advanced.ignoredText')"
                        spellcheck="false"
                        @input="updateLine(index, ($event.target as HTMLInputElement).value)"
                    />
                    <button class="btn-icon" :aria-label="t('settings.advanced.removeText')" @click="removeLine(index)">
                        <i class="bi bi-x-lg" aria-hidden="true" />
                    </button>
                </div>
                <button class="link-accent self-start min-h-8" @click="addLine">
                    + {{ t('settings.advanced.addText') }}
                </button>
            </div>
        </SectionCard>

        <SectionCard :title="t('settings.advanced.task')" :description="t('settings.advanced.taskHint')">
            <template #aside>
                <input
                    v-model="configStore.playout.task.enable"
                    type="checkbox"
                    role="switch"
                    class="toggle toggle-accent"
                    :aria-label="t('settings.advanced.runTask')"
                />
            </template>
            <label class="field-label max-w-2xl" :class="configStore.playout.task.enable ? '' : 'opacity-60'">
                {{ t('settings.advanced.program') }}
                <input
                    v-model.trim="configStore.playout.task.path"
                    class="field-input font-mono text-[13px]"
                    placeholder="/data/scripts/on-clip-change.sh"
                    spellcheck="false"
                />
            </label>
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
