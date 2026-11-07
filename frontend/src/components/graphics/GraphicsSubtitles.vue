<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'

import SaveBar from '@/components/settings/SaveBar.vue'
import SectionCard from '@/components/settings/SectionCard.vue'
import ToggleRow from '@/components/settings/ToggleRow.vue'
import { usePlayoutPage } from '@/composables/settingsPage'

// VTT fields retain their existing settings save contract; channel artwork is
// authored exclusively in Composer, not in this metadata page.
const { t } = useI18n()
const { ready, changes, alsoSaves, saveLabel, save, discard, settings, configStore } = usePlayoutPage('logo')
const processing = computed(() => configStore.playout.processing)
</script>
<template>
    <template v-if="ready">
        <div>
            <h1 class="m-0 text-xl font-semibold">{{ t('graphics.sections.subtitles') }}</h1>
            <p class="mt-1 mb-0 text-muted">{{ t('graphics.intro.subtitles') }}</p>
        </div>
        <SectionCard :title="t('settings.logo.subtitles')" :description="t('settings.logo.subtitlesHint')">
            <template #aside>
                <input
                    v-model="processing.vtt_enable"
                    type="checkbox"
                    role="switch"
                    class="toggle toggle-accent"
                    :aria-label="t('settings.logo.useSubtitles')"
                />
            </template>
            <div class="grid grid-cols-1 md:grid-cols-3 gap-3.5" :class="processing.vtt_enable ? '' : 'opacity-60'">
                <label class="field-label">
                    {{ t('settings.logo.trackName') }}
                    <input v-model.trim="processing.vtt_name" class="field-input" />
                </label>
                <label class="field-label">
                    {{ t('settings.logo.language') }}
                    <input v-model.trim="processing.vtt_language" class="field-input font-mono" placeholder="pt-BR" />
                </label>
                <label class="field-label">
                    {{ t('settings.logo.emptyFile') }}
                    <input
                        v-model.trim="processing.vtt_dummy"
                        class="field-input font-mono text-[13px]"
                        spellcheck="false"
                    />
                    <span class="field-hint">{{ t('settings.logo.emptyFileHint') }}</span>
                </label>
            </div>
            <ToggleRow v-model="processing.vtt_default" :label="t('settings.logo.subtitleDefault')" />
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
