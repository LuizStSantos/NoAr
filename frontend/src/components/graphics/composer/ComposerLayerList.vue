<script setup lang="ts">
import { computed, nextTick, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import { useGraphicsDisplayNames } from '@/composables/graphicsDisplayNames'

import { type TemplateLayer, templateFields } from '@/composables/graphicsTemplates'

/** The document owner performs all add/remove/visibility changes. */
const props = defineProps<{ layers: TemplateLayer[]; alwaysOn?: boolean | null }>()
const selectedId = defineModel<string>('selectedId', { required: true })
const adding = defineModel<boolean>('adding', { required: true })
const emit = defineEmits<{
    add: [kind: TemplateLayer['kind']]
    remove: [layerId: string]
    toggle: [layer: TemplateLayer]
    move: [layerId: string, toward: 'front' | 'back']
    /** Put the layer at `index` of the drawing order (0 = back). */
    reorder: [layerId: string, index: number]
    rename: [layerId: string, name: string]
}>()
const { t } = useI18n()
const { layerName } = useGraphicsDisplayNames()

/** The layer drawn last is in front: it is listed first. */
const stacked = computed(() => [...props.layers].reverse())

/** On wide screens the order arrows and delete show on the selected row, on hover and on keyboard focus, leaving room for the name. */
const reorderHidden = 'lg:hidden lg:group-hover:inline-flex lg:group-focus-within:inline-flex'

/** The layer being renamed in the list (double-click or F2 on its name). */
const renaming = ref<string>()
const renameInput = ref<HTMLInputElement[]>()

async function startRename(layer: TemplateLayer) {
    renaming.value = layer.id
    await nextTick()
    renameInput.value?.[0]?.select()
}

function finishRename(layer: TemplateLayer, value: string | null) {
    if (renaming.value !== layer.id) {
        return
    }

    renaming.value = undefined
    const name = value?.trim()

    if (name && name !== layerName(layer)) {
        emit('rename', layer.id, name)
    }
}

/** Dragging a row onto another puts it in that row's place. */
const dragged = ref<string>()
const dropTarget = ref<string>()

function drop(targetId: string) {
    const id = dragged.value
    dragged.value = dropTarget.value = undefined

    if (id && id !== targetId) {
        emit(
            'reorder',
            id,
            props.layers.findIndex((layer) => layer.id === targetId),
        )
    }
}
</script>
<template>
    <aside class="card-box p-2 flex flex-col gap-2 min-w-0 min-h-0" :aria-label="t('graphics.composer.layers')">
        <div class="flex items-center justify-between pl-1">
            <h2 class="m-0 text-sm font-semibold">{{ t('graphics.composer.layers') }}</h2>
            <button
                class="btn-icon min-h-9 min-w-9"
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
                class="btn-quiet text-left min-h-9"
                :disabled="kind === 'video' && (alwaysOn === true || layers.some((layer) => layer.kind === 'video'))"
                :title="kind === 'video' && alwaysOn === true ? t('graphics.composer.videoModelOnly') : undefined"
                @click="emit('add', kind)"
            >
                {{ t('graphics.composer.kinds.' + kind) }}
            </button>
        </div>
        <p v-if="!layers.length" class="m-0 px-1 text-sm text-muted">{{ t('graphics.composer.empty') }}</p>
        <p v-else class="m-0 px-1 text-xs text-muted">{{ t('graphics.composer.orderHint') }}</p>
        <ul class="m-0 p-0 list-none flex flex-col gap-1 overflow-y-auto min-h-0">
            <li
                v-for="(layer, index) in stacked"
                :key="layer.id"
                class="group flex items-center gap-0.5 rounded-field min-w-0 border"
                :class="[
                    selectedId === layer.id ? 'border-accent bg-base-300' : 'border-transparent hover:bg-base-200',
                    { 'opacity-50': !layer.visible, 'outline-2 outline-accent/60': dropTarget === layer.id },
                ]"
                draggable="true"
                @dragstart="dragged = layer.id"
                @dragend="dragged = dropTarget = undefined"
                @dragover.prevent="dropTarget = layer.id"
                @dragleave="dropTarget = dropTarget === layer.id ? undefined : dropTarget"
                @drop.prevent="drop(layer.id)"
            >
                <i class="bi bi-grip-vertical text-muted cursor-grab pl-1" aria-hidden="true" />
                <input
                    v-if="renaming === layer.id"
                    ref="renameInput"
                    class="field-input flex-auto min-w-0 min-h-8 py-0 px-1.5 text-sm"
                    :value="layerName(layer)"
                    maxlength="100"
                    :aria-label="t('graphics.composer.renameNamed', { name: layerName(layer) })"
                    @keydown.enter.prevent="finishRename(layer, ($event.target as HTMLInputElement).value)"
                    @keydown.esc.prevent="finishRename(layer, null)"
                    @blur="finishRename(layer, ($event.target as HTMLInputElement).value)"
                />
                <button
                    v-else
                    class="flex-auto min-w-0 min-h-9 text-left px-1 cursor-pointer flex flex-col justify-center"
                    :aria-pressed="selectedId === layer.id"
                    :title="`${layerName(layer)} · ${t('graphics.composer.kinds.' + layer.kind)} — ${t('graphics.composer.renameHint')}`"
                    @click="selectedId = layer.id"
                    @dblclick="startRename(layer)"
                    @keydown.f2.prevent="startRename(layer)"
                >
                    <span class="sr-only">{{ t('graphics.composer.kinds.' + layer.kind) }}</span>
                    <span class="text-sm leading-tight truncate">{{ layerName(layer) }}</span>
                    <span
                        v-if="templateFields([layer]).length"
                        class="font-mono text-[11px] leading-tight text-accent truncate"
                        >{{
                            templateFields([layer])
                                .map((field) => '{' + field + '}')
                                .join(' ')
                        }}</span
                    >
                </button>
                <button
                    class="btn-icon min-h-8 min-w-7"
                    :class="selectedId === layer.id ? '' : reorderHidden"
                    :aria-label="t('graphics.composer.toFront', { name: layerName(layer) })"
                    :title="t('graphics.composer.toFront', { name: layerName(layer) })"
                    :disabled="index === 0"
                    @click="emit('move', layer.id, 'front')"
                >
                    <i class="bi bi-chevron-up text-xs" />
                </button>
                <button
                    class="btn-icon min-h-8 min-w-7"
                    :class="selectedId === layer.id ? '' : reorderHidden"
                    :aria-label="t('graphics.composer.toBack', { name: layerName(layer) })"
                    :title="t('graphics.composer.toBack', { name: layerName(layer) })"
                    :disabled="index === stacked.length - 1"
                    @click="emit('move', layer.id, 'back')"
                >
                    <i class="bi bi-chevron-down text-xs" />
                </button>
                <button
                    class="btn-icon min-h-8 min-w-8"
                    :aria-label="
                        t(layer.visible ? 'graphics.composer.hide' : 'graphics.composer.show', {
                            name: layerName(layer),
                        })
                    "
                    :aria-pressed="!layer.visible"
                    @click="emit('toggle', layer)"
                >
                    <i :class="layer.visible ? 'bi bi-eye' : 'bi bi-eye-slash'" />
                </button>
                <button
                    class="btn-icon min-h-8 min-w-8"
                    :class="selectedId === layer.id ? '' : reorderHidden"
                    :aria-label="t('graphics.composer.deleteNamed', { name: layerName(layer) })"
                    @click="emit('remove', layer.id)"
                >
                    <i class="bi bi-x-lg text-xs" />
                </button>
            </li>
        </ul>
    </aside>
</template>
