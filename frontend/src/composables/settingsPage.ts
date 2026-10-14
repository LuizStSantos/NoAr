import { computed } from 'vue'
import { useI18n } from 'vue-i18n'

import { useConfig } from '@/stores/config'
import { useIndex } from '@/stores/index'
import { usePlaylist } from '@/stores/playlist'
import { SETTINGS_SECTIONS, useSettings, type SettingsSectionName } from '@/stores/settings'

/** Save and discard for a settings page that edits the channel configuration. */
export function usePlayoutPage(name: SettingsSectionName) {
    const { t } = useI18n()
    const configStore = useConfig()
    const indexStore = useIndex()
    const playlistStore = usePlaylist()
    const settings = useSettings()

    const section = SETTINGS_SECTIONS.find((item) => item.name === name)
    const ready = computed(() => Boolean(configStore.playout?.output))
    const changes = computed(() => settings.sectionChanges(name))
    const alsoSaves = computed(() =>
        settings.changedSections
            .filter((other) => other !== name && other !== 'general')
            .map((other) => t(`settings.sections.${other}`)),
    )
    /** Pages whose changes need a restart restart the playout right away. */
    const restarts = computed(() => Boolean(section?.restarts) && playlistStore.playoutIsRunning)
    const saveLabel = computed(() => (restarts.value ? t('settings.saveAndRestart') : t('settings.save')))

    async function save() {
        try {
            await settings.savePlayout(restarts.value)
            indexStore.msgAlert('success', t('settings.saved'), 2)
        } catch (error) {
            indexStore.msgAlert('error', error instanceof Error ? error.message : String(error), 4)
        }
    }

    function discard() {
        settings.discardSection(name)

        for (const other of settings.changedSections) {
            if (other !== 'general') {
                settings.discardSection(other)
            }
        }
    }

    return { ready, changes, alsoSaves, restarts, saveLabel, save, discard, settings, configStore }
}
