<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'

import PositionPad from '@/components/settings/PositionPad.vue'
import { LOGO_BOX, sharesOf } from '@/composables/graphics'
import type { GraphicsScene } from '@/types/playout_config'

/**
 * What a set of graphics changes does to the logo and the L-shape: a quick
 * scene, or graphics scheduled at a time. "Leave as it is" changes nothing.
 */
const scene = defineModel<GraphicsScene>({ required: true })
const emit = defineEmits<{ pick: [field: 'logo_image' | 'lshape_graphic'] }>()

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

/** The position select: a place name, the channel's, nothing, or free (`X%:Y%`). */
const positionChoice = computed(() => {
    const position = scene.value.logo_position

    if (!position) {
        return ''
    }

    return position === 'default' || (PLACES as readonly string[]).includes(position) ? position : 'free'
})

function choosePosition(value: string) {
    scene.value.logo_position = value === 'free' ? '50%:50%' : value || null
}

/** A select's value, where an empty string means "leave it". */
function optional(event: Event): string | null {
    return (event.target as HTMLSelectElement).value || null
}
</script>
<template>
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
                    <button class="btn-quiet flex-none" @click="emit('pick', 'logo_image')">
                        {{ t('graphics.scenes.choose') }}
                    </button>
                </div>
            </div>
            <label class="field-label">
                {{ t('graphics.scenes.logoPosition') }}
                <select
                    class="field-input"
                    :value="positionChoice"
                    @change="choosePosition(($event.target as HTMLSelectElement).value)"
                >
                    <option value="">{{ t('graphics.scenes.leave') }}</option>
                    <option value="default">{{ t('settings.logo.remote.moveDefault') }}</option>
                    <option v-for="place in PLACES" :key="place" :value="place">
                        {{ t(`graphics.places.${place}`) }}
                    </option>
                    <option value="free">{{ t('graphics.scenes.freePosition') }}</option>
                </select>
            </label>
            <PositionPad
                v-if="positionChoice === 'free'"
                :shares="sharesOf(scene.logo_position)"
                :width="LOGO_BOX.width"
                :height="LOGO_BOX.height"
                :label="t('settings.logo.remote.logoPad')"
                @place="scene.logo_position = `${$event.x}%:${$event.y}%`"
            />
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
                    <button class="btn-quiet flex-none" @click="emit('pick', 'lshape_graphic')">
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
                        @change="scene.lshape_scale = optional($event) === null ? null : Number(optional($event))"
                    >
                        <option value="">{{ t('graphics.scenes.leave') }}</option>
                        <option v-for="size in SIZES" :key="size" :value="size">{{ size }}%</option>
                    </select>
                </label>
            </div>
        </fieldset>
    </div>
</template>
