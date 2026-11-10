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
    <section
        class="card-box px-3 py-2 flex flex-col gap-1.5 shrink-0 max-h-[40%] overflow-y-auto"
        :aria-label="t('graphics.composer.timeline')"
    >
        <div class="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs">
            <h2 class="text-xs font-semibold m-0">{{ t('graphics.composer.timeline') }}</h2>
            <label class="flex items-center gap-2 flex-auto min-w-40 text-muted"
                >{{ t('graphics.composer.playhead')
                }}<input
                    type="range"
                    class="flex-auto"
                    min="0"
                    :max="end"
                    step="0.01"
                    :value="time ?? 0"
                    @input="emit('seek', Number(($event.target as HTMLInputElement).value))"
            /></label>
            <span class="font-mono text-muted">{{ (time ?? 0).toFixed(2) }} / {{ end.toFixed(2) }} s</span>
            <span class="inline-flex items-center gap-1 text-muted"
                ><span class="w-2.5 h-1.5 rounded-sm bg-accent" aria-hidden="true" />{{
                    t('graphics.composer.legendIn')
                }}</span
            >
            <span class="inline-flex items-center gap-1 text-muted"
                ><span class="w-2.5 h-1.5 rounded-sm bg-indigo-400" aria-hidden="true" />{{
                    t('graphics.composer.legendOut')
                }}</span
            >
            <button v-if="time !== undefined" class="btn-quiet min-h-8 py-0" @click="emit('stop')">
                {{ t('graphics.composer.backToEdit') }}
            </button>
        </div>
        <div
            v-for="layer in layers"
            :key="layer.id"
            class="flex gap-2 items-center text-xs"
            :class="{ 'opacity-40': !layer.visible }"
        >
            <button
                class="w-20 truncate text-left cursor-pointer text-muted"
                :class="{ 'text-base-content font-semibold': selectedId === layer.id }"
                :aria-pressed="selectedId === layer.id"
                @click="selectedId = layer.id"
            >
                {{ layerName(layer) }}
            </button>
            <div
                class="relative flex-1 h-3 bg-base-300 rounded overflow-hidden"
                :aria-label="`${layerName(layer)}: ${layer.delay_ms} ms + ${layer.animation_ms} ms`"
            >
                <div
                    class="absolute inset-y-0 bg-accent/25"
                    :style="{
                        left: (layer.delay_ms / 1000 / end) * 100 + '%',
                        width: Math.max(0, (((duration || end) - layer.delay_ms / 1000) / end) * 100) + '%',
                    }"
                />
                <div
                    class="absolute inset-y-0 bg-accent rounded-sm"
                    :style="{
                        left: (layer.delay_ms / 1000 / end) * 100 + '%',
                        width: (layer.animation_in === 'none' ? 0 : (layer.animation_ms / 1000 / end) * 100) + '%',
                    }"
                />
                <div
                    v-if="duration && layer.animation_out !== 'none'"
                    class="absolute inset-y-0 bg-indigo-400 rounded-sm"
                    :style="{
                        left: (duration / end) * 100 + '%',
                        width: (layer.animation_ms / 1000 / end) * 100 + '%',
                    }"
                />
                <div
                    v-if="time !== undefined"
                    class="absolute inset-y-0 border-l-2 border-white"
                    :style="{ left: (time / end) * 100 + '%' }"
                />
            </div>
        </div>
        <p class="m-0 text-[11px] text-muted">{{ t('graphics.composer.previewHint') }}</p>
    </section>
</template>
