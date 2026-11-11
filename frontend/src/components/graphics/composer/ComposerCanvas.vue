<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useElementSize } from '@vueuse/core'

import { useGraphicsDisplayNames } from '@/composables/graphicsDisplayNames'

import TemplatePreview from '@/components/graphics/TemplatePreview.vue'
import { type GraphicsTemplate, type TemplateLayer, fitLayer } from '@/composables/graphicsTemplates'

/** Canvas interactions emit changes; the editor owns the draft and one undo entry per gesture. */
const props = defineProps<{
    template: GraphicsTemplate
    values: Record<string, string>
    time?: number
    proportional: boolean
}>()
const selectedId = defineModel<string>('selectedId', { required: true })
const emit = defineEmits<{
    interactionStart: [layerId: string]
    interactionChange: [layerId: string, patch: Partial<TemplateLayer>]
    interactionEnd: []
}>()
const { t } = useI18n()
const { layerName } = useGraphicsDisplayNames()
const canvas = ref<HTMLElement>()
const canvasSize = useElementSize(canvas)
// Authoring aids are never part of the saved model or the on-air preview.
const showSafeAreas = ref(false)
const guideV = ref(false)
const guideH = ref(false)
type InteractionMode = 'move' | 'width' | 'height' | 'both'
let interaction:
    { id: string; mode: InteractionMode; x: number; y: number; rect: DOMRect; layer: TemplateLayer } | undefined

function startInteraction(event: PointerEvent, layer: TemplateLayer, mode: InteractionMode) {
    if (event.button !== 0 || !canvas.value) {
        return
    }

    event.preventDefault()
    selectedId.value = layer.id
    emit('interactionStart', layer.id)
    ;(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId)
    interaction = {
        id: layer.id,
        mode,
        x: event.clientX,
        y: event.clientY,
        rect: canvas.value.getBoundingClientRect(),
        layer: { ...layer },
    }
}

/** Clamp authoring targets to the canvas without changing the saved artwork. */
function handleStyle(layer: TemplateLayer, handle: Exclude<InteractionMode, 'move'>) {
    if ((layer.width * canvasSize.width.value) / 100 < 80 || (layer.height * canvasSize.height.value) / 100 < 80) {
        const index = ['width', 'height', 'both'].indexOf(handle)

        return {
            left: `calc(clamp(0px, calc(${layer.x + layer.width}% - 80px), calc(100% - 80px)) + ${index * 28}px)`,
            top: `clamp(0px, calc(${layer.y + layer.height}% + 4px), calc(100% - 24px))`,
        }
    }

    const x = handle === 'height' ? layer.x + layer.width / 2 : layer.x + layer.width
    const y = handle === 'width' ? layer.y + layer.height / 2 : layer.y + layer.height

    return {
        left: `clamp(0px, calc(${x}% - ${handle === 'height' ? 12 : 24}px), calc(100% - 24px))`,
        top: `clamp(0px, calc(${y}% - ${handle === 'width' ? 12 : 24}px), calc(100% - 24px))`,
    }
}

function resizeFromKeyboard(event: KeyboardEvent, layer: TemplateLayer, handle: Exclude<InteractionMode, 'move'>) {
    const step = event.shiftKey ? 10 : 1
    const patch: Partial<TemplateLayer> = {}

    if (handle !== 'height' && ['ArrowLeft', 'ArrowRight', 'Enter', ' '].includes(event.key)) {
        patch.width = layer.width + (event.key === 'ArrowLeft' ? -step : step)
    }
    if (handle !== 'width' && ['ArrowUp', 'ArrowDown', 'Enter', ' '].includes(event.key)) {
        patch.height = layer.height + (event.key === 'ArrowUp' ? -step : step)
    }
    if (patch.width === undefined && patch.height === undefined) {
        return
    }

    event.preventDefault()
    event.stopPropagation()
    emit('interactionStart', layer.id)
    emit('interactionChange', layer.id, fitLayer(layer, patch, props.proportional))
    emit('interactionEnd')
}

/** Match the existing centre/safe-edge snap without changing typed geometry. */
function snapPosition(value: number, size: number) {
    const targets = [50 - size / 2, 5, 95 - size]

    return Math.max(0, Math.min(targets.find((target) => Math.abs(value - target) < 1.2) ?? value, 100 - size))
}

function moveInteraction(event: PointerEvent) {
    if (!interaction || !props.template.layers.some((layer) => layer.id === interaction?.id)) {
        return
    }

    const original = interaction.layer
    const dx = ((event.clientX - interaction.x) / interaction.rect.width) * 100
    const dy = ((event.clientY - interaction.y) / interaction.rect.height) * 100

    if (interaction.mode === 'move') {
        const x = snapPosition(original.x + dx, original.width)
        const y = snapPosition(original.y + dy, original.height)
        guideV.value = Math.abs(x + original.width / 2 - 50) < 0.01
        guideH.value = Math.abs(y + original.height / 2 - 50) < 0.01
        emit('interactionChange', interaction.id, { x, y })
    } else {
        const patch =
            interaction.mode === 'height'
                ? { height: original.height + dy }
                : interaction.mode === 'width'
                  ? { width: original.width + dx }
                  : { width: original.width + dx, height: original.height + dy }
        emit('interactionChange', interaction.id, fitLayer(original, patch, props.proportional))
    }
}

function endInteraction() {
    if (!interaction) {
        return
    }

    interaction = undefined
    guideV.value = false
    guideH.value = false
    emit('interactionEnd')
}
</script>
<template>
    <p id="composer-resize-help" class="sr-only">{{ t('graphics.composer.resizeKeys') }}</p>
    <div class="flex flex-wrap gap-3 items-center text-xs text-muted">
        <span>{{ t('graphics.onAir.preview') }}</span>
        <label class="flex items-center gap-2 cursor-pointer min-h-9">
            <input v-model="showSafeAreas" type="checkbox" class="checkbox checkbox-sm" />
            {{ t('graphics.composer.safeArea') }}
        </label>
        <span class="ml-auto font-mono">{{ time?.toFixed(2) ?? '0.00' }} s</span>
    </div>
    <p v-if="showSafeAreas" class="m-0 text-xs text-muted">
        {{ t('graphics.composer.safeAreaHint') }}
    </p>
    <div class="composer-fit">
        <div
            ref="canvas"
            class="composer-fit-inner relative touch-none select-none"
            @pointermove="moveInteraction"
            @pointerup="endInteraction"
            @pointercancel="endInteraction"
            @lostpointercapture="endInteraction"
        >
            <TemplatePreview
                :template="template"
                :values="values"
                :time="time"
                :animate-text="false"
                class="composer-canvas rounded-field border border-line-strong"
            >
                <template #overlay>
                    <template v-if="showSafeAreas">
                        <div
                            class="composer-safe-area composer-action-safe"
                            role="img"
                            :aria-label="t('graphics.composer.actionSafe')"
                        />
                        <div
                            class="composer-safe-area composer-title-safe"
                            role="img"
                            :aria-label="t('graphics.composer.titleSafe')"
                        />
                    </template>
                    <div
                        v-if="guideV"
                        class="absolute left-1/2 inset-y-0 border-l border-cyan-300 pointer-events-none"
                    />
                    <div
                        v-if="guideH"
                        class="absolute top-1/2 inset-x-0 border-t border-cyan-300 pointer-events-none"
                    />
                    <template v-if="time === undefined">
                        <template v-for="layer in template.layers.filter((layer) => layer.visible)" :key="layer.id">
                            <div
                                class="absolute"
                                :class="selectedId === layer.id ? 'outline-2 outline-cyan-300 z-10' : ''"
                                :style="{
                                    left: layer.x + '%',
                                    top: layer.y + '%',
                                    width: layer.width + '%',
                                    height: layer.height + '%',
                                }"
                            >
                                <button
                                    type="button"
                                    :aria-label="t('graphics.composer.selectNamed', { name: layerName(layer) })"
                                    :aria-pressed="selectedId === layer.id"
                                    class="absolute inset-0 w-full h-full cursor-move outline-offset-1 focus-visible:outline-2 focus-visible:outline-accent"
                                    @click="selectedId = layer.id"
                                    @pointerdown.stop="startInteraction($event, layer, 'move')"
                                />
                            </div>
                            <template v-if="selectedId === layer.id">
                                <button
                                    v-for="handle in ['width', 'height', 'both'] as const"
                                    :key="handle"
                                    type="button"
                                    class="absolute w-[24px] h-[24px] border-2 border-cyan-300 bg-base-100 rounded-xs z-20"
                                    :class="
                                        handle === 'width'
                                            ? 'cursor-ew-resize'
                                            : handle === 'height'
                                              ? 'cursor-ns-resize'
                                              : 'cursor-nwse-resize'
                                    "
                                    :style="handleStyle(layer, handle)"
                                    :aria-label="t('graphics.composer.handles.' + handle)"
                                    :title="t('graphics.composer.resizeKeys')"
                                    aria-describedby="composer-resize-help"
                                    @keydown="resizeFromKeyboard($event, layer, handle)"
                                    @pointerdown.stop="startInteraction($event, layer, handle)"
                                >
                                    <i
                                        class="bi text-[10px]"
                                        :class="
                                            handle === 'width'
                                                ? 'bi-arrow-left-right'
                                                : handle === 'height'
                                                  ? 'bi-arrow-down-up'
                                                  : 'bi-arrows-angle-expand'
                                        "
                                        aria-hidden="true"
                                    />
                                </button>
                            </template>
                        </template>
                    </template>
                </template>
            </TemplatePreview>
        </div>
    </div>
</template>

<style scoped>
.composer-fit-inner {
    width: 100%;
}
/* On a wide screen the editor fits the window. The preview is as tall as a
   16:9 picture of the column's width, so the timeline sits right under it;
   in a low window it shrinks to the height left by the timeline, centred.
   (100cqw is the width of the editor's middle column, its query container.) */
@media (min-width: 1100px) {
    .composer-fit {
        container-type: size;
        flex: 0 1 calc(100cqw * 9 / 16);
        min-height: 10rem;
        display: flex;
        justify-content: center;
    }
    .composer-fit-inner {
        width: min(100cqw, calc(100cqh * 16 / 9));
    }
}
.composer-canvas.graphics-preview {
    background-image:
        linear-gradient(to right, #ffffff24 1px, transparent 1px),
        linear-gradient(to bottom, #ffffff24 1px, transparent 1px), var(--graphics-preview-checker);
    background-size:
        10% 10%,
        10% 10%,
        20px 20px;
}
.composer-safe-area {
    position: absolute;
    z-index: 20;
    pointer-events: none;
    border: 1px dashed;
    box-shadow: 0 0 0 1px #0009;
}
.composer-action-safe {
    inset: 5%;
    border-color: #fff;
}
.composer-title-safe {
    inset: 10%;
    border-color: #fbbf24;
}
</style>
