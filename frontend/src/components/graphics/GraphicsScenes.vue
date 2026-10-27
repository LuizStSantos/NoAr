<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'

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
const LOGO_ACTIONS = ['keep', 'hide', 'show', 'replace'] as const
const LSHAPE_ACTIONS = ['keep', 'on', 'off'] as const
const PLACES = [
    'top-left',
    'top',
    'top-right',
    'left',
    'center',
    'right',
    'bottom-left',
    'bottom',
    'bottom-right',
] as const
const CORNERS = ['top-left', 'top-right', 'bottom-left', 'bottom-right'] as const
const SIZES = [30, 35, 40, 45, 50, 55, 60, 65, 70, 75, 80, 85, 90, 95]

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

/** A select's value, where an empty string means "leave it". */
function optional(event: Event): string | null {
    return (event.target as HTMLSelectElement).value || null
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

            <div class="grid grid-cols-[repeat(auto-fit,minmax(min(100%,280px),1fr))] gap-4">
                <fieldset class="flex flex-col gap-3 border border-line rounded-field p-3 m-0">
                    <legend class="px-1 text-[13px] font-semibold">{{ t('settings.logo.remote.logo') }}</legend>
                    <label class="field-label">
                        {{ t('graphics.scenes.logoAction') }}
                        <select v-model="scene.logo" class="field-input">
                            <option v-for="action in LOGO_ACTIONS" :key="action" :value="action">
                                {{ t(`graphics.scenes.logoActions.${action}`) }}
                            </option>
                        </select>
                    </label>
                    <div v-if="scene.logo === 'replace'" class="field-label">
                        {{ t('graphics.scenes.logoImage') }}
                        <div class="flex gap-2">
                            <input
                                v-model="scene.logo_image"
                                class="field-input font-mono text-[13px]"
                                :aria-label="t('graphics.scenes.logoImage')"
                            />
                            <button class="btn-quiet flex-none" @click="picking = { scene, field: 'logo_image' }">
                                {{ t('graphics.scenes.choose') }}
                            </button>
                        </div>
                    </div>
                    <label class="field-label">
                        {{ t('graphics.scenes.logoPosition') }}
                        <select
                            class="field-input"
                            :value="scene.logo_position ?? ''"
                            @change="scene.logo_position = optional($event)"
                        >
                            <option value="">{{ t('graphics.scenes.leave') }}</option>
                            <option value="default">{{ t('settings.logo.remote.moveDefault') }}</option>
                            <option v-for="place in PLACES" :key="place" :value="place">
                                {{ t(`graphics.places.${place}`) }}
                            </option>
                            <option
                                v-if="scene.logo_position && !['default', ...PLACES].includes(scene.logo_position)"
                                :value="scene.logo_position"
                            >
                                {{ scene.logo_position }}
                            </option>
                        </select>
                    </label>
                </fieldset>

                <fieldset class="flex flex-col gap-3 border border-line rounded-field p-3 m-0">
                    <legend class="px-1 text-[13px] font-semibold">{{ t('settings.logo.remote.lshape') }}</legend>
                    <label class="field-label">
                        {{ t('graphics.scenes.lshapeAction') }}
                        <select v-model="scene.lshape" class="field-input">
                            <option v-for="action in LSHAPE_ACTIONS" :key="action" :value="action">
                                {{ t(`graphics.scenes.lshapeActions.${action}`) }}
                            </option>
                        </select>
                    </label>
                    <div class="field-label">
                        {{ t('graphics.scenes.lshapeGraphic') }}
                        <div class="flex gap-2">
                            <input
                                v-model="scene.lshape_graphic"
                                class="field-input font-mono text-[13px]"
                                :placeholder="t('graphics.scenes.graphicKeep')"
                                :aria-label="t('graphics.scenes.lshapeGraphic')"
                            />
                            <button class="btn-quiet flex-none" @click="picking = { scene, field: 'lshape_graphic' }">
                                {{ t('graphics.scenes.choose') }}
                            </button>
                        </div>
                    </div>
                    <div class="grid grid-cols-2 gap-3">
                        <label class="field-label">
                            {{ t('graphics.scenes.corner') }}
                            <select
                                class="field-input"
                                :value="scene.lshape_corner ?? ''"
                                @change="scene.lshape_corner = optional($event)"
                            >
                                <option value="">{{ t('graphics.scenes.leave') }}</option>
                                <option value="default">{{ t('graphics.scenes.cornerDefault') }}</option>
                                <option v-for="corner in CORNERS" :key="corner" :value="corner">
                                    {{ t(`lshape.corners.${corner}`) }}
                                </option>
                            </select>
                        </label>
                        <label class="field-label">
                            {{ t('graphics.scenes.size') }}
                            <select
                                class="field-input"
                                :value="scene.lshape_scale ?? ''"
                                @change="
                                    scene.lshape_scale = optional($event) === null ? null : Number(optional($event))
                                "
                            >
                                <option value="">{{ t('graphics.scenes.leave') }}</option>
                                <option v-for="size in SIZES" :key="size" :value="size">{{ size }}%</option>
                            </select>
                        </label>
                    </div>
                </fieldset>
            </div>
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
