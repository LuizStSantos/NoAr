<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import LShapeCornerPad from '@/components/graphics/LShapeCornerPad.vue'
import SectionCard from '@/components/settings/SectionCard.vue'
import StoragePicker from '@/components/schedule/StoragePicker.vue'
import GenericModal from '@/components/utils/GenericModal.vue'
import { storageImageUrl } from '@/composables/logo'
import { lshapeTemplate, lshapeTemplateName } from '@/composables/lshapeTemplate'
import { useConfig } from '@/stores/config'
import { useIndex } from '@/stores/index'

/**
 * The L-shape: a full-screen graphic the picture shrinks into, its size and
 * corner. It goes on air from the live graphics card below, from encoders
 * with the graphics key, and for stretches marked in a video.
 */
type Corner = 'top-left' | 'top-right' | 'bottom-left' | 'bottom-right'

const { t } = useI18n()
const configStore = useConfig()
const indexStore = useIndex()

const picking = ref(false)
const imageUrl = ref('')
const imageFailed = ref(false)

const processing = computed(() => configStore.playout.processing)
const frame = computed(() => ({
    width: Math.max(1, configStore.playout.output?.width || 1920),
    height: Math.max(1, configStore.playout.output?.height || 1080),
}))

const scale = computed({
    get: () => processing.value.lshape_scale ?? 75,
    set: (value: number) => (processing.value.lshape_scale = Math.min(95, Math.max(30, Math.round(value)))),
})

/** Where the picture goes, in percent of the frame, as the engine places it. */
const windowStyle = computed(() => {
    const share = scale.value
    const corner = (processing.value.lshape_corner || 'top-right') as Corner
    const style: Record<string, string> = { width: `${share}%`, height: `${share}%` }

    style[corner.startsWith('top') ? 'top' : 'bottom'] = '0'
    style[corner.endsWith('left') ? 'left' : 'right'] = '0'

    return style
})

async function loadImage() {
    imageFailed.value = false
    imageUrl.value = ''

    try {
        imageUrl.value = await storageImageUrl(configStore.channels[configStore.i], processing.value?.lshape)
    } catch {
        imageFailed.value = true
    }
}

/** The base for the image editor, for the size and corner set here. */
async function downloadTemplate() {
    const layout = {
        width: frame.value.width,
        height: frame.value.height,
        scale: scale.value,
        corner: processing.value.lshape_corner || 'top-right',
    }

    try {
        const blob = await lshapeTemplate(layout, {
            band: t('lshape.template.band'),
            video: t('lshape.template.video'),
            safe: t('lshape.template.safe'),
        })
        const link = document.createElement('a')
        link.href = URL.createObjectURL(blob)
        link.download = lshapeTemplateName(layout)
        link.click()
        setTimeout(() => URL.revokeObjectURL(link.href), 1000)
    } catch {
        indexStore.msgAlert('error', t('lshape.template.failed'), 3)
    }
}

function pick(file: { path: string }) {
    processing.value.lshape = file.path
    picking.value = false
}

watch(() => processing.value?.lshape, loadImage, { immediate: true })
// A save loads the graphic on air.
</script>
<template>
    <SectionCard :title="t('lshape.title')" :description="t('lshape.intro')">
        <div class="flex flex-wrap gap-6">
            <div class="w-full max-w-[480px] flex-none self-start flex flex-col gap-2">
                <div
                    class="relative w-full rounded-xl overflow-hidden bg-[#0b0c0d] border border-line"
                    :style="{ aspectRatio: `${frame.width} / ${frame.height}` }"
                >
                    <img
                        v-if="imageUrl && !imageFailed"
                        :src="imageUrl"
                        alt=""
                        class="absolute inset-0 w-full h-full object-fill"
                        @error="imageFailed = true"
                    />
                    <span
                        v-else
                        class="absolute inset-0 flex items-center justify-center text-[#6e6e72] text-xs text-center px-4"
                        >{{ t('lshape.noGraphic') }}</span
                    >
                    <span
                        class="absolute bg-[#2a2d33]/90 border-2 border-accent flex items-center justify-center text-[11px] text-white"
                        :style="windowStyle"
                        >{{ t('lshape.picture') }}</span
                    >
                </div>
                <span class="field-hint">{{ t('lshape.previewHint') }}</span>
            </div>
            <div class="flex-[1_1_300px] flex flex-col gap-4 min-w-0">
                <label class="field-label">
                    {{ t('lshape.graphic') }}
                    <span class="flex gap-2">
                        <input
                            v-model.trim="processing.lshape"
                            class="field-input font-mono text-[13px]"
                            :placeholder="t('lshape.graphicPlaceholder')"
                            spellcheck="false"
                        />
                        <button class="btn-quiet flex-none" @click="picking = true">
                            {{ t('settings.choose') }}
                        </button>
                    </span>
                    <span class="field-hint">{{ t('lshape.graphicHint', frame) }}</span>
                </label>
                <div class="flex flex-wrap items-center gap-x-3 gap-y-1">
                    <button type="button" class="btn-quiet" @click="downloadTemplate">
                        <i class="bi bi-download" aria-hidden="true" />{{ t('lshape.template.download') }}
                    </button>
                    <span class="field-hint">{{ t('lshape.template.hint') }}</span>
                </div>
                <div class="flex flex-wrap gap-5">
                    <LShapeCornerPad
                        :corner="processing.lshape_corner || 'top-right'"
                        :label="t('lshape.corner')"
                        @choose="processing.lshape_corner = $event"
                    />
                    <label class="field-label flex-[1_1_200px]">
                        {{ t('lshape.size', { n: scale }) }}
                        <input
                            v-model.number="scale"
                            type="range"
                            min="30"
                            max="95"
                            class="range range-xs range-accent max-w-[360px]"
                        />
                    </label>
                </div>

                <p class="m-0 field-hint">
                    <i class="bi bi-arrow-down-short" aria-hidden="true" />{{ t('lshape.liveBelow') }}
                </p>
            </div>
        </div>

        <GenericModal
            :show="picking"
            :title="t('settings.choose')"
            hide-buttons
            :modal-action="() => (picking = false)"
        >
            <div class="w-[min(520px,90vw)] max-h-[60vh] flex flex-col">
                <StoragePicker @pick="pick" />
            </div>
        </GenericModal>
    </SectionCard>
</template>
