<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'

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
const canvas = ref<HTMLElement>()
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
    <div
        ref="canvas"
        class="relative touch-none select-none"
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
                <div v-if="guideV" class="absolute left-1/2 inset-y-0 border-l border-cyan-300 pointer-events-none" />
                <div v-if="guideH" class="absolute top-1/2 inset-x-0 border-t border-cyan-300 pointer-events-none" />
                <template v-if="time === undefined">
                    <div
                        v-for="layer in template.layers.filter((layer) => layer.visible)"
                        :key="layer.id"
                        role="button"
                        tabindex="0"
                        :aria-label="t('graphics.composer.selectNamed', { name: layer.name })"
                        class="absolute cursor-move outline-offset-1 focus-visible:outline-2 focus-visible:outline-accent"
                        :class="selectedId === layer.id ? 'outline-2 outline-cyan-300 z-10' : ''"
                        :style="{
                            left: layer.x + '%',
                            top: layer.y + '%',
                            width: layer.width + '%',
                            height: layer.height + '%',
                        }"
                        @keydown.enter="selectedId = layer.id"
                        @pointerdown.stop="startInteraction($event, layer, 'move')"
                    >
                        <template v-if="selectedId === layer.id">
                            <button
                                v-for="handle in ['width', 'height', 'both'] as const"
                                :key="handle"
                                class="absolute w-4 h-4 border-2 border-cyan-300 bg-base-100 rounded-xs z-10"
                                :class="
                                    handle === 'width'
                                        ? '-right-2 top-1/2 -translate-y-1/2 cursor-ew-resize'
                                        : handle === 'height'
                                          ? 'left-1/2 -bottom-2 -translate-x-1/2 cursor-ns-resize'
                                          : '-right-2 -bottom-2 cursor-nwse-resize'
                                "
                                :aria-label="t('graphics.composer.handles.' + handle)"
                                @pointerdown.stop="startInteraction($event, layer, handle)"
                            />
                        </template>
                    </div>
                </template>
            </template>
        </TemplatePreview>
    </div>
</template>

<style scoped>
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
