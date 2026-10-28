<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'

import LShapeCornerPad from '@/components/graphics/LShapeCornerPad.vue'
import PositionPad from '@/components/settings/PositionPad.vue'
import { LOGO_BOX, sharesOf } from '@/composables/graphics'
import { useConfig } from '@/stores/config'
import type { GraphicsScene } from '@/types/playout_config'

/**
 * What a set of graphics changes does to the logo and the L-shape: a quick
 * scene, or graphics scheduled at a time. "Leave as it is" changes nothing.
 * The logo and the L-shape are placed on the same small screens as in a
 * video's marks.
 */
const scene = defineModel<GraphicsScene>({ required: true })
const emit = defineEmits<{ pick: [field: 'logo_image' | 'lshape_graphic'] }>()

const LOGO_ACTIONS = ['keep', 'show', 'hide', 'replace'] as const
const LSHAPE_ACTIONS = ['keep', 'on', 'off'] as const
const POSITION_MODES = ['keep', 'default', 'own'] as const
const SIZES = [30, 35, 40, 45, 50, 55, 60, 65, 70, 75, 80, 85, 90, 95]

const { t } = useI18n()
const configStore = useConfig()

const channelPosition = computed(() => configStore.playout?.processing?.logo_position || 'W-w-12:12')
const channelCorner = computed(() => configStore.playout?.processing?.lshape_corner || 'top-right')

/** Leave the logo where it is, put it back where the channel has it, or a place of its own. */
const positionMode = computed(() => {
    const position = scene.value.logo_position

    return !position ? 'keep' : position === 'default' ? 'default' : 'own'
})

/** The pad shows the scene's own place, else the channel's. */
const padShares = computed(() =>
    sharesOf(positionMode.value === 'own' ? scene.value.logo_position : channelPosition.value),
)

function choosePositionMode(mode: (typeof POSITION_MODES)[number]) {
    if (mode === 'keep') {
        scene.value.logo_position = null
    } else if (mode === 'default') {
        scene.value.logo_position = 'default'
    } else {
        const shares = padShares.value

        scene.value.logo_position = `${shares.x}%:${shares.y}%`
    }
}

/** The corner the scene moves the L-shape picture to; `default` is the channel's. */
const corner = computed(() =>
    scene.value.lshape_corner === 'default' ? channelCorner.value : scene.value.lshape_corner,
)

/** A second click on the corner chosen leaves the corner as it is again. */
function chooseCorner(option: string) {
    scene.value.lshape_corner = corner.value === option ? null : option
}

function chooseScale(event: Event) {
    const value = (event.target as HTMLSelectElement).value

    scene.value.lshape_scale = value ? Number(value) : null
}
</script>
<template>
    <div class="grid grid-cols-[repeat(auto-fit,minmax(min(100%,300px),1fr))] gap-4">
        <fieldset class="flex flex-col gap-3 border border-line rounded-field p-3 m-0 min-w-0">
            <legend class="px-1 text-[13px] font-semibold">{{ t('settings.logo.remote.logo') }}</legend>

            <div class="flex flex-col gap-1.5">
                <span class="text-xs text-muted">{{ t('graphics.scenes.logoAction') }}</span>
                <div
                    role="radiogroup"
                    :aria-label="t('graphics.scenes.logoAction')"
                    class="grid grid-cols-2 gap-0.5 bg-base-200 rounded-md p-0.5 text-xs"
                >
                    <button
                        v-for="action in LOGO_ACTIONS"
                        :key="action"
                        type="button"
                        role="radio"
                        :aria-checked="scene.logo === action"
                        class="px-2.5 min-h-8 rounded cursor-pointer"
                        :class="scene.logo === action ? 'bg-base-100 font-semibold shadow-sm' : 'text-muted'"
                        @click="scene.logo = action"
                    >
                        {{ t(`graphics.scenes.logoActions.${action}`) }}
                    </button>
                </div>
            </div>

            <div v-if="scene.logo === 'replace'" class="field-label">
                {{ t('graphics.scenes.logoImage') }}
                <div class="flex flex-wrap gap-2">
                    <input
                        v-model="scene.logo_image"
                        class="field-input font-mono text-[13px] flex-[1_1_180px] min-w-0"
                        :placeholder="t('graphics.scenes.logoImagePlaceholder')"
                        :aria-label="t('graphics.scenes.logoImage')"
                        spellcheck="false"
                    />
                    <button type="button" class="btn-quiet flex-none" @click="emit('pick', 'logo_image')">
                        {{ t('graphics.scenes.choose') }}
                    </button>
                </div>
            </div>

            <div class="flex flex-col gap-2">
                <div class="flex flex-wrap items-center justify-between gap-2">
                    <span class="text-[13px] font-semibold">{{ t('graphics.scenes.logoPosition') }}</span>
                    <div
                        role="radiogroup"
                        :aria-label="t('graphics.scenes.logoPosition')"
                        class="inline-flex bg-base-200 rounded-md p-0.5 text-xs"
                    >
                        <button
                            v-for="mode in POSITION_MODES"
                            :key="mode"
                            type="button"
                            role="radio"
                            :aria-checked="positionMode === mode"
                            class="px-2.5 min-h-8 rounded cursor-pointer"
                            :class="positionMode === mode ? 'bg-base-100 font-semibold shadow-sm' : 'text-muted'"
                            @click="choosePositionMode(mode)"
                        >
                            {{ t(`graphics.scenes.positionModes.${mode}`) }}
                        </button>
                    </div>
                </div>
                <PositionPad
                    class="max-w-72"
                    :class="positionMode === 'own' ? '' : 'opacity-70'"
                    :shares="padShares"
                    :width="LOGO_BOX.width"
                    :height="LOGO_BOX.height"
                    :label="t('settings.logo.remote.logoPad')"
                    @place="scene.logo_position = `${$event.x}%:${$event.y}%`"
                />
                <p class="m-0 field-hint">{{ t(`graphics.scenes.positionHints.${positionMode}`) }}</p>
            </div>
        </fieldset>

        <fieldset class="flex flex-col gap-3 border border-line rounded-field p-3 m-0 min-w-0">
            <legend class="px-1 text-[13px] font-semibold">{{ t('settings.logo.remote.lshape') }}</legend>

            <div class="flex flex-col gap-1.5">
                <span class="text-xs text-muted">{{ t('graphics.scenes.lshapeAction') }}</span>
                <div
                    role="radiogroup"
                    :aria-label="t('graphics.scenes.lshapeAction')"
                    class="inline-flex flex-wrap self-start bg-base-200 rounded-md p-0.5 text-xs"
                >
                    <button
                        v-for="action in LSHAPE_ACTIONS"
                        :key="action"
                        type="button"
                        role="radio"
                        :aria-checked="scene.lshape === action"
                        class="px-2.5 min-h-8 rounded cursor-pointer"
                        :class="scene.lshape === action ? 'bg-base-100 font-semibold shadow-sm' : 'text-muted'"
                        @click="scene.lshape = action"
                    >
                        {{ t(`graphics.scenes.lshapeActions.${action}`) }}
                    </button>
                </div>
            </div>

            <div class="field-label">
                {{ t('graphics.scenes.lshapeGraphic') }}
                <div class="flex flex-wrap gap-2">
                    <input
                        v-model="scene.lshape_graphic"
                        class="field-input font-mono text-[13px] flex-[1_1_180px] min-w-0"
                        :placeholder="t('graphics.scenes.graphicKeep')"
                        :aria-label="t('graphics.scenes.lshapeGraphic')"
                        spellcheck="false"
                    />
                    <button type="button" class="btn-quiet flex-none" @click="emit('pick', 'lshape_graphic')">
                        {{ t('graphics.scenes.choose') }}
                    </button>
                </div>
            </div>

            <div class="flex flex-wrap items-start gap-4">
                <LShapeCornerPad
                    :corner="corner"
                    :fallback-label="t('graphics.scenes.leave')"
                    :label="t('graphics.scenes.corner')"
                    @choose="chooseCorner"
                />
                <label class="field-label flex-[1_1_150px]">
                    {{ t('graphics.scenes.size') }}
                    <select class="field-input" :value="scene.lshape_scale ?? ''" @change="chooseScale">
                        <option value="">{{ t('graphics.scenes.leave') }}</option>
                        <option v-for="size in SIZES" :key="size" :value="size">
                            {{ t('storage.marks.lshapeLook.sizeValue', { n: size }) }}
                        </option>
                    </select>
                </label>
            </div>
            <p class="m-0 field-hint">{{ t('graphics.scenes.lshapeHint') }}</p>
        </fieldset>
    </div>
</template>
