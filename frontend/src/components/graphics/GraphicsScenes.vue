<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import SceneActionsFields from '@/components/graphics/SceneActionsFields.vue'
import SaveBar from '@/components/settings/SaveBar.vue'
import SectionCard from '@/components/settings/SectionCard.vue'
import StoragePicker from '@/components/schedule/StoragePicker.vue'
import GenericModal from '@/components/utils/GenericModal.vue'
import { usePlayoutPage } from '@/composables/settingsPage'
import type { GraphicsScene } from '@/types/playout_config'

/**
 * Grafismo › Quick scenes: named sets of logo and L-shape changes, applied
 * in one go from Live now, the Player or an encoder.
 */
const MAX_SCENES = 24

const { t } = useI18n()
const { ready, changes, alsoSaves, saveLabel, save, discard, settings, configStore } = usePlayoutPage('logo')

/** The scene and the image the storage picker is choosing for it. */
const picking = ref<{ scene: GraphicsScene; field: 'logo_image' | 'lshape_graphic' } | null>(null)

const scenes = computed<GraphicsScene[]>(() => configStore.playout.processing.graphics_scenes)

function addScene() {
    scenes.value.push({
        name: '',
        logo: 'keep',
        logo_image: null,
        logo_position: null,
        lshape: 'keep',
        lshape_graphic: null,
        lshape_corner: null,
        lshape_scale: null,
    })
}

function moveScene(index: number, by: number) {
    const [scene] = scenes.value.splice(index, 1)

    if (scene) {
        scenes.value.splice(index + by, 0, scene)
    }
}

function pick(file: { path: string }) {
    if (picking.value) {
        picking.value.scene[picking.value.field] = file.path
    }

    picking.value = null
}
</script>
<template>
    <template v-if="ready">
        <div>
            <h1 class="m-0 text-xl font-semibold">{{ t('graphics.sections.scenes') }}</h1>
            <p class="mt-1 mb-0 text-muted">{{ t('graphics.intro.scenes') }}</p>
        </div>

        <SectionCard
            v-for="(scene, index) in scenes"
            :key="index"
            :title="scene.name.trim() || t('graphics.scenes.untitled')"
        >
            <template #aside>
                <div class="flex gap-1">
                    <button
                        class="btn-icon min-w-9 min-h-9"
                        :disabled="index === 0"
                        :aria-label="t('graphics.scenes.moveUp')"
                        @click="moveScene(index, -1)"
                    >
                        <i class="bi bi-arrow-up" aria-hidden="true" />
                    </button>
                    <button
                        class="btn-icon min-w-9 min-h-9"
                        :disabled="index === scenes.length - 1"
                        :aria-label="t('graphics.scenes.moveDown')"
                        @click="moveScene(index, 1)"
                    >
                        <i class="bi bi-arrow-down" aria-hidden="true" />
                    </button>
                    <button
                        class="btn-icon min-w-9 min-h-9"
                        :aria-label="t('graphics.scenes.remove')"
                        @click="scenes.splice(index, 1)"
                    >
                        <i class="bi bi-trash" aria-hidden="true" />
                    </button>
                </div>
            </template>

            <label class="field-label max-w-md">
                {{ t('graphics.scenes.name') }}
                <input
                    v-model="scene.name"
                    class="field-input"
                    maxlength="60"
                    :placeholder="t('graphics.scenes.namePlaceholder')"
                />
            </label>

            <SceneActionsFields
                v-model="scenes[index]!"
                @pick="(field: 'logo_image' | 'lshape_graphic') => (picking = { scene, field })"
            />
        </SectionCard>

        <p v-if="!scenes.length" class="m-0 text-muted">{{ t('graphics.scenes.empty') }}</p>

        <button class="btn-quiet self-start" :disabled="scenes.length >= MAX_SCENES" @click="addScene">
            <i class="bi bi-plus-lg" aria-hidden="true" />{{ t('graphics.scenes.add') }}
        </button>
        <span v-if="scenes.length >= MAX_SCENES" class="field-hint">{{
            t('graphics.scenes.limit', { n: MAX_SCENES })
        }}</span>

        <SaveBar
            :changes="changes"
            :also-saves="alsoSaves"
            :saving="settings.saving"
            :save-label="saveLabel"
            @save="save"
            @discard="discard"
        />

        <GenericModal
            :show="picking !== null"
            :title="t('settings.choose')"
            hide-buttons
            :modal-action="() => (picking = null)"
        >
            <div class="w-[min(520px,90vw)] max-h-[60vh] flex flex-col">
                <StoragePicker @pick="pick" />
            </div>
        </GenericModal>
    </template>
</template>
