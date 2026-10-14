<script setup lang="ts">
import dayjs from 'dayjs'

import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import RadioCards from '@/components/settings/RadioCards.vue'
import SaveBar from '@/components/settings/SaveBar.vue'
import SectionCard from '@/components/settings/SectionCard.vue'
import ToggleRow from '@/components/settings/ToggleRow.vue'
import StoragePicker from '@/components/schedule/StoragePicker.vue'
import GenericModal from '@/components/utils/GenericModal.vue'
import { usePlayoutPage } from '@/composables/settingsPage'

const { t, locale } = useI18n()
const { ready, changes, alsoSaves, restarts, saveLabel, save, discard, settings, configStore } =
    usePlayoutPage('playlist')

const TIME = /^([01]?\d|2[0-4]):[0-5]\d:[0-5]\d$/

const picking = ref(false)
const newExtension = ref('')

const modes = computed(() => [
    {
        value: 'playlist',
        label: t('settings.playlist.modePlaylist'),
        description: t('settings.playlist.modePlaylistHint'),
    },
    { value: 'folder', label: t('settings.playlist.modeFolder'), description: t('settings.playlist.modeFolderHint') },
])

const dayStartValid = computed(() => TIME.test(configStore.playout.playlist.day_start))
const lengthValid = computed(() => TIME.test(configStore.playout.playlist.length))

function seconds(time: string): number {
    const [h = 0, m = 0, s = 0] = time.split(':').map(Number)

    return h * 3600 + m * 60 + s
}

/** "The playlist of 14 Oct runs from 14 Oct 06:00 to 15 Oct 06:00." */
const dayExample = computed(() => {
    if (!dayStartValid.value || !lengthValid.value) {
        return ''
    }

    const start = dayjs()
        .startOf('day')
        .add(seconds(configStore.playout.playlist.day_start), 'second')
        .locale(locale.value.toLowerCase())
    const end = start.add(seconds(configStore.playout.playlist.length), 'second')

    return t('settings.playlist.dayExample', {
        day: start.format('D MMM'),
        start: start.format('D MMM HH:mm'),
        end: end.format('D MMM HH:mm'),
    })
})

function addExtension() {
    const values = newExtension.value
        .split(/[,;\s]+/)
        .map((value) => value.trim().replace(/^\./, '').toLowerCase())
        .filter(Boolean)

    for (const value of values) {
        if (!configStore.playout.storage.extensions.includes(value)) {
            configStore.playout.storage.extensions.push(value)
        }
    }

    newExtension.value = ''
}

function removeExtension(value: string) {
    configStore.playout.storage.extensions = configStore.playout.storage.extensions.filter((item) => item !== value)
}

function pickFiller(file: { path: string }) {
    configStore.playout.storage.filler = file.path
    picking.value = false
}
</script>
<template>
    <template v-if="ready">
        <div class="flex flex-wrap items-end justify-between gap-3">
            <div>
                <h1 class="m-0 text-xl font-semibold">{{ t('settings.sections.playlist') }}</h1>
                <p class="mt-1 mb-0 text-muted">{{ t('settings.playlist.intro') }}</p>
            </div>
            <span v-if="restarts" class="inline-flex items-center gap-2 text-[13px] text-accent-text">
                <i class="bi bi-arrow-repeat" aria-hidden="true" />{{ t('settings.restartsOnSave') }}
            </span>
        </div>

        <SectionCard :title="t('settings.playlist.what')">
            <RadioCards
                v-model="configStore.playout.processing.mode"
                :options="modes"
                :label="t('settings.playlist.what')"
            />
        </SectionCard>

        <SectionCard :title="t('settings.playlist.day')">
            <div class="grid grid-cols-1 sm:grid-cols-2 gap-3.5 max-w-2xl">
                <label class="field-label">
                    {{ t('settings.playlist.dayStart') }}
                    <input
                        v-model.trim="configStore.playout.playlist.day_start"
                        class="field-input font-mono"
                        :class="dayStartValid ? '' : 'border-error!'"
                        placeholder="06:00:00"
                    />
                    <span class="field-hint">{{ dayStartValid ? dayExample : t('settings.playlist.timeFormat') }}</span>
                </label>
                <label class="field-label">
                    {{ t('settings.playlist.length') }}
                    <input
                        v-model.trim="configStore.playout.playlist.length"
                        class="field-input font-mono"
                        :class="lengthValid ? '' : 'border-error!'"
                        placeholder="24:00:00"
                    />
                    <span v-if="!lengthValid" class="field-hint">{{ t('settings.playlist.timeFormat') }}</span>
                </label>
            </div>
            <ToggleRow
                v-model="configStore.playout.playlist.infinit"
                :label="t('settings.playlist.infinite')"
                :hint="t('settings.playlist.infiniteHint')"
            />
        </SectionCard>

        <SectionCard :title="t('settings.playlist.storage')">
            <label class="field-label max-w-2xl">
                {{ t('settings.playlist.filler') }}
                <span class="flex gap-2">
                    <input
                        v-model.trim="configStore.playout.storage.filler"
                        class="field-input font-mono text-[13px]"
                        spellcheck="false"
                    />
                    <button class="btn-quiet flex-none" @click="picking = true">{{ t('settings.choose') }}</button>
                </span>
                <span class="field-hint">{{ t('settings.playlist.fillerHint') }}</span>
            </label>
            <div class="flex flex-col gap-2">
                <span class="text-xs text-muted">{{ t('settings.playlist.extensions') }}</span>
                <div
                    class="flex flex-wrap items-center gap-2 bg-field border border-line-strong rounded-field p-2 min-h-11"
                >
                    <span
                        v-for="extension in configStore.playout.storage.extensions"
                        :key="extension"
                        class="inline-flex items-center gap-1 bg-raised rounded-md ps-2.5 pe-1 py-0.5 font-mono text-[13px]"
                    >
                        {{ extension }}
                        <button
                            class="btn-icon min-w-6 min-h-6 text-xs"
                            :aria-label="t('settings.playlist.removeExtension', { name: extension })"
                            @click="removeExtension(extension)"
                        >
                            <i class="bi bi-x" aria-hidden="true" />
                        </button>
                    </span>
                    <input
                        v-model="newExtension"
                        class="bg-transparent border-0 outline-none flex-1 min-w-24 font-mono text-[13px] px-1"
                        :placeholder="t('settings.playlist.addExtension')"
                        :aria-label="t('settings.playlist.addExtension')"
                        @keydown.enter.prevent="addExtension"
                        @input="newExtension.includes(',') && addExtension()"
                        @blur="addExtension"
                    />
                </div>
            </div>
            <ToggleRow
                v-model="configStore.playout.storage.shuffle"
                :label="t('settings.playlist.shuffle')"
                :hint="t('settings.playlist.shuffleHint')"
            />
        </SectionCard>

        <SaveBar
            :changes="changes"
            :also-saves="alsoSaves"
            :saving="settings.saving"
            :save-label="saveLabel"
            @save="dayStartValid && lengthValid && save()"
            @discard="discard"
        />

        <GenericModal
            :show="picking"
            :title="t('settings.playlist.filler')"
            hide-buttons
            :modal-action="() => (picking = false)"
        >
            <div class="w-[min(520px,90vw)] max-h-[60vh] flex flex-col">
                <StoragePicker @pick="pickFiller" />
            </div>
        </GenericModal>
    </template>
</template>
