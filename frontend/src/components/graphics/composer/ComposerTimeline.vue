<script setup lang="ts">
import { useI18n } from 'vue-i18n'

import { useGraphicsDisplayNames } from '@/composables/graphicsDisplayNames'

import type { TemplateLayer } from '@/composables/graphicsTemplates'

/** Render authored timing; playback and document changes belong to the editor. */
defineProps<{
    layers: TemplateLayer[]
    duration: number
    end: number
    time?: number
}>()
const selectedId = defineModel<string>('selectedId', { required: true })
const emit = defineEmits<{ seek: [seconds: number]; stop: [] }>()
const { t } = useI18n()
const { layerName } = useGraphicsDisplayNames()
</script>
<template>
    <section class="card-box p-3 flex flex-col gap-2" :aria-label="t('graphics.composer.timeline')">
        <h2 class="text-sm font-semibold m-0">{{ t('graphics.composer.timeline') }}</h2>
        <label class="field-label"
            >{{ t('graphics.composer.playhead')
            }}<input
                type="range"
                min="0"
                :max="end"
                step="0.01"
                :value="time ?? 0"
                @input="emit('seek', Number(($event.target as HTMLInputElement).value))"
        /></label>
        <div
            v-for="layer in layers"
            :key="layer.id"
            class="flex gap-2 items-center text-xs"
            :class="{ 'opacity-40': !layer.visible }"
        >
            <button
                class="w-24 truncate text-left cursor-pointer"
                :aria-pressed="selectedId === layer.id"
                @click="selectedId = layer.id"
            >
                {{ layerName(layer) }}
            </button>
            <div
                class="relative flex-1 h-6 bg-base-300 rounded overflow-hidden"
                :aria-label="`${layerName(layer)}: ${layer.delay_ms} ms + ${layer.animation_ms} ms`"
            >
                <div
                    class="absolute inset-y-1 bg-accent/25"
                    :style="{
                        left: (layer.delay_ms / 1000 / end) * 100 + '%',
                        width: Math.max(0, (((duration || end) - layer.delay_ms / 1000) / end) * 100) + '%',
                    }"
                />
                <div
                    class="absolute inset-y-1 bg-accent rounded"
                    :style="{
                        left: (layer.delay_ms / 1000 / end) * 100 + '%',
                        width: (layer.animation_in === 'none' ? 0 : (layer.animation_ms / 1000 / end) * 100) + '%',
                    }"
                />
                <div
                    v-if="duration && layer.animation_out !== 'none'"
                    class="absolute inset-y-1 bg-indigo-400 rounded"
                    :style="{
                        left: (duration / end) * 100 + '%',
                        width: (layer.animation_ms / 1000 / end) * 100 + '%',
                    }"
                />
                <div
                    v-if="time !== undefined"
                    class="absolute inset-y-0 border-l border-white"
                    :style="{ left: (time / end) * 100 + '%' }"
                />
            </div>
        </div>
        <p class="m-0 text-xs text-muted">
            {{ t('graphics.composer.timelineHint') }} {{ t('graphics.composer.previewHint') }}
        </p>
        <button v-if="time !== undefined" class="btn-quiet" @click="emit('stop')">
            {{ t('graphics.composer.backToEdit') }}
        </button>
    </section>
</template>
