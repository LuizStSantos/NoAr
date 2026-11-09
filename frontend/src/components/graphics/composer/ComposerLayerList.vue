<script setup lang="ts">
import { useI18n } from 'vue-i18n'

import { useGraphicsDisplayNames } from '@/composables/graphicsDisplayNames'

import { type TemplateLayer, templateFields } from '@/composables/graphicsTemplates'

/** The document owner performs all add/remove/visibility changes. */
defineProps<{ layers: TemplateLayer[]; alwaysOn?: boolean | null }>()
const selectedId = defineModel<string>('selectedId', { required: true })
const adding = defineModel<boolean>('adding', { required: true })
const emit = defineEmits<{
    add: [kind: TemplateLayer['kind']]
    remove: [layerId: string]
    toggle: [layer: TemplateLayer]
}>()
const { t } = useI18n()
const { layerName } = useGraphicsDisplayNames()
</script>
<template>
    <aside class="card-box p-3 flex flex-col gap-3 min-w-0" :aria-label="t('graphics.composer.layers')">
        <div class="flex items-center justify-between">
            <h2 class="m-0 text-base font-semibold">{{ t('graphics.composer.layers') }}</h2>
            <button
                class="btn-icon min-h-11 min-w-11"
                :aria-label="t('graphics.composer.add')"
                :aria-expanded="adding"
                :disabled="layers.length >= 64"
                @click="adding = !adding"
            >
                <i class="bi bi-plus-lg" />
            </button>
        </div>
        <div
            v-if="adding"
            class="flex flex-col gap-1 border border-line-strong rounded-field p-1"
            role="group"
            :aria-label="t('graphics.composer.layerType')"
        >
            <button
                v-for="kind in ['image', 'text', 'box', 'video', 'clock'] as const"
                :key="kind"
                class="btn-quiet text-left"
                :disabled="kind === 'video' && (alwaysOn === true || layers.some((layer) => layer.kind === 'video'))"
                :title="kind === 'video' && alwaysOn === true ? t('graphics.composer.videoModelOnly') : undefined"
                @click="emit('add', kind)"
            >
                {{ t('graphics.composer.kinds.' + kind) }}
            </button>
        </div>
        <p v-if="!layers.length" class="m-0 text-sm text-muted">{{ t('graphics.composer.empty') }}</p>
        <div
            v-for="layer in layers"
            :key="layer.id"
            class="flex items-center border rounded-field min-w-0"
            :class="[
                selectedId === layer.id ? 'border-accent bg-base-300' : 'border-line-strong',
                { 'opacity-50': !layer.visible },
            ]"
        >
            <button
                class="flex-auto min-w-0 min-h-12 text-left px-2 cursor-pointer"
                :aria-pressed="selectedId === layer.id"
                @click="selectedId = layer.id"
            >
                <span class="text-[10px] text-muted block">{{ t('graphics.composer.kinds.' + layer.kind) }}</span
                ><span class="block text-sm truncate">{{ layerName(layer) }}</span
                ><span class="block font-mono text-xs text-accent truncate">{{
                    templateFields([layer])
                        .map((field) => '{' + field + '}')
                        .join(' ')
                }}</span>
            </button>
            <button
                class="btn-icon min-h-9 min-w-9"
                :aria-label="
                    t(layer.visible ? 'graphics.composer.hide' : 'graphics.composer.show', { name: layerName(layer) })
                "
                :aria-pressed="!layer.visible"
                @click="emit('toggle', layer)"
            >
                <i :class="layer.visible ? 'bi bi-eye' : 'bi bi-eye-slash'" />
            </button>
            <button
                class="btn-icon min-h-9 min-w-9"
                :aria-label="t('graphics.composer.deleteNamed', { name: layerName(layer) })"
                @click="emit('remove', layer.id)"
            >
                <i class="bi bi-x-lg" />
            </button>
        </div>
    </aside>
</template>
