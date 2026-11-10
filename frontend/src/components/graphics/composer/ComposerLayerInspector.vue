<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import ColorField from '@/components/graphics/composer/ColorField.vue'

import { useGraphicsDisplayNames } from '@/composables/graphicsDisplayNames'

import type { GraphicsFont } from '@/composables/graphicsFonts'
import { CLOCK_FORMATS } from '@/composables/graphicsClock'
import { type TemplateLayer, LAYER_ANIMATIONS } from '@/composables/graphicsTemplates'

/** Appearance controls emit patches so the document owner records undo consistently. */
const props = defineProps<{
    selected?: TemplateLayer
    fonts: string[]
    uploadedFonts: GraphicsFont[]
    fontUploading: boolean
}>()
const emit = defineEmits<{
    updateLayer: [patch: Partial<TemplateLayer>]
    remove: [layerId: string]
    uploadFont: [event: Event]
}>()
const { t } = useI18n()
const { layerName } = useGraphicsDisplayNames()

function renameLayer(name: string) {
    if (props.selected && name !== layerName(props.selected)) {
        emit('updateLayer', { name })
    }
}

function setShape(shape: NonNullable<TemplateLayer['shape']>) {
    emit('updateLayer', {
        shape,
        ...(shape === 'rounded' ? { radius: 2 } : {}),
        ...(shape === 'line' ? { stroke: props.selected?.stroke || '#ffffff', fill: null } : {}),
    })
}

function setScroll(direction: string) {
    emit('updateLayer', {
        scroll:
            direction === 'left' || direction === 'right'
                ? { speed: 80, gap: 80, ...props.selected?.scroll, direction }
                : null,
    })
}
</script>
<template>
    <aside class="card-box p-3 flex flex-col gap-3 min-w-0" :aria-label="t('graphics.composer.properties')">
        <template v-if="selected">
            <label class="field-label"
                >{{ t('graphics.composer.layerName')
                }}<input
                    :value="layerName(selected)"
                    class="field-input"
                    maxlength="100"
                    @change="renameLayer(($event.target as HTMLInputElement).value)"
            /></label>
            <button class="btn-quiet text-error" @click="emit('remove', selected.id)">
                {{ t('graphics.composer.delete') }}
            </button>
            <slot name="geometry" />
            <template v-if="selected.kind === 'text' || selected.kind === 'clock'">
                <template v-if="selected.kind === 'clock'">
                    <label class="field-label"
                        >{{ t('graphics.composer.clockFormat') }}
                        <select
                            class="field-input"
                            :aria-label="t('graphics.composer.clockFormat')"
                            :value="selected.clock_format || 'HH:mm:ss'"
                            @change="
                                emit('updateLayer', {
                                    clock_format: ($event.target as HTMLSelectElement).value,
                                })
                            "
                        >
                            <option v-for="format in CLOCK_FORMATS" :key="format" :value="format">
                                {{ format }}
                            </option>
                        </select>
                    </label>
                    <label class="field-label"
                        >{{ t('graphics.composer.clockTimezone')
                        }}<input
                            class="field-input"
                            :value="selected.clock_timezone || ''"
                            :placeholder="t('graphics.composer.channelTimezone')"
                            @change="
                                emit('updateLayer', {
                                    clock_timezone: ($event.target as HTMLInputElement).value.trim() || null,
                                })
                            "
                    /></label>
                    <p class="text-xs text-muted m-0">{{ t('graphics.composer.clockHint') }}</p>
                </template>
                <label v-if="selected.kind === 'text'" class="field-label"
                    >{{ t('graphics.composer.text')
                    }}<textarea
                        class="field-input min-h-20"
                        :value="selected.text"
                        maxlength="1000"
                        @change="emit('updateLayer', { text: ($event.target as HTMLTextAreaElement).value })"
                    />
                </label>
                <p v-if="selected.kind === 'text'" class="m-0 text-xs text-muted">
                    {{ t('graphics.composer.fieldsHint') }}
                </p>
                <fieldset v-if="selected.kind === 'text'" class="flex flex-col gap-2 border-t border-line pt-3">
                    <legend class="text-sm font-semibold">{{ t('graphics.composer.scroll.title') }}</legend>
                    <label class="field-label">
                        {{ t('graphics.composer.scroll.direction') }}
                        <select
                            class="field-input"
                            :value="selected.scroll?.direction || 'none'"
                            @change="setScroll(($event.target as HTMLSelectElement).value)"
                        >
                            <option value="none">{{ t('graphics.composer.scroll.none') }}</option>
                            <option value="left">{{ t('graphics.composer.scroll.left') }}</option>
                            <option value="right">{{ t('graphics.composer.scroll.right') }}</option>
                        </select>
                    </label>
                    <template v-if="selected.scroll">
                        <label class="field-label">
                            {{ t('graphics.composer.scroll.speed') }}
                            <input
                                class="field-input"
                                type="number"
                                min="1"
                                max="2000"
                                step="1"
                                :value="selected.scroll.speed"
                                @change="
                                    emit('updateLayer', {
                                        scroll: {
                                            ...selected.scroll,
                                            speed: Number(($event.target as HTMLInputElement).value),
                                        },
                                    })
                                "
                            />
                        </label>
                        <label class="field-label">
                            {{ t('graphics.composer.scroll.gap') }}
                            <input
                                class="field-input"
                                type="number"
                                min="0"
                                max="4096"
                                step="1"
                                :value="selected.scroll.gap"
                                @change="
                                    emit('updateLayer', {
                                        scroll: {
                                            ...selected.scroll,
                                            gap: Number(($event.target as HTMLInputElement).value),
                                        },
                                    })
                                "
                            />
                        </label>
                        <p class="text-xs text-muted m-0">{{ t('graphics.composer.scroll.hint') }}</p>
                    </template>
                </fieldset>
                <label class="field-label"
                    >{{ t('graphics.composer.font')
                    }}<select
                        class="field-input"
                        :value="selected.font_family ?? ''"
                        @change="
                            emit('updateLayer', {
                                font_family: ($event.target as HTMLSelectElement).value || null,
                            })
                        "
                    >
                        <option value="">{{ t('graphics.composer.defaultFont') }}</option>
                        <option v-for="font in uploadedFonts" :key="font.id" :value="font.family">
                            {{ font.name }} — {{ font.filename }} ({{ font.id.slice(0, 6) }})
                        </option>
                        <option
                            v-if="
                                selected.font_family?.startsWith('NoArFont_') &&
                                !uploadedFonts.some((font) => font.family === selected?.font_family)
                            "
                            :value="selected.font_family"
                        >
                            {{ t('graphics.composer.fontUnavailable') }}
                        </option>
                        <option v-for="font in fonts" :key="font" :value="font">{{ font }}</option>
                    </select></label
                >
                <label
                    class="btn-quiet cursor-pointer inline-flex items-center justify-center focus-within:outline-2 focus-within:outline-offset-2 focus-within:outline-accent"
                >
                    {{ fontUploading ? t('graphics.composer.fontUploading') : t('graphics.composer.uploadFont') }}
                    <input
                        type="file"
                        accept=".ttf,.otf"
                        class="sr-only"
                        :aria-label="t('graphics.composer.uploadFont')"
                        :disabled="fontUploading"
                        @change="emit('uploadFont', $event)"
                    />
                </label>
                <p class="text-xs text-muted m-0">{{ t('graphics.composer.fontHint') }}</p>
                <label class="field-label"
                    >{{ t('graphics.composer.fontSize') }} (%)<input
                        class="field-input"
                        type="number"
                        step="0.1"
                        min="0.1"
                        max="50"
                        :value="selected.font_size"
                        @change="
                            emit('updateLayer', {
                                font_size: Number(($event.target as HTMLInputElement).value),
                            })
                        "
                /></label>
                <label class="flex items-center gap-2 text-sm"
                    ><input
                        type="checkbox"
                        class="checkbox checkbox-sm"
                        :checked="selected.bold"
                        @change="emit('updateLayer', { bold: ($event.target as HTMLInputElement).checked })"
                    />{{ t('graphics.composer.bold') }}</label
                >
                <ColorField
                    :label="t('graphics.composer.color')"
                    :value="selected.color"
                    @change="emit('updateLayer', { color: $event ?? '#ffffff' })"
                />
                <label class="field-label"
                    >{{ t('graphics.composer.align')
                    }}<select
                        class="field-input"
                        :value="selected.align"
                        @change="
                            emit('updateLayer', {
                                align: ($event.target as HTMLSelectElement).value as TemplateLayer['align'],
                            })
                        "
                    >
                        <option v-for="align in ['left', 'center', 'right'] as const" :key="align" :value="align">
                            {{ t('graphics.composer.alignments.' + align) }}
                        </option>
                    </select></label
                >
            </template>
            <slot name="video" />
            <slot name="image" />
            <template v-if="selected.kind === 'box'">
                <label class="field-label"
                    >{{ t('graphics.composer.shape') }}
                    <select
                        class="field-input"
                        :aria-label="t('graphics.composer.shape')"
                        :value="selected.shape || 'rectangle'"
                        @change="
                            setShape(($event.target as HTMLSelectElement).value as NonNullable<TemplateLayer['shape']>)
                        "
                    >
                        <option
                            v-for="shape in ['rectangle', 'rounded', 'ellipse', 'line'] as const"
                            :key="shape"
                            :value="shape"
                        >
                            {{ t(`graphics.composer.shapes.${shape}`) }}
                        </option>
                    </select>
                </label>
                <ColorField
                    :label="t('graphics.composer.stroke')"
                    :value="selected.stroke"
                    clearable
                    @change="emit('updateLayer', { stroke: $event })"
                />
                <label class="field-label"
                    >{{ t('graphics.composer.strokeWidth') }} (%)<input
                        class="field-input"
                        type="number"
                        min="0"
                        max="20"
                        step="0.1"
                        :value="selected.stroke_width ?? 0.25"
                        @change="
                            emit('updateLayer', {
                                stroke_width: Number(($event.target as HTMLInputElement).value),
                            })
                        "
                /></label>
            </template>
            <template v-if="selected.kind !== 'video'">
                <ColorField
                    :label="t('graphics.composer.fill')"
                    :value="selected.fill"
                    clearable
                    @change="emit('updateLayer', { fill: $event })"
                />
                <label class="field-label"
                    >{{ t('graphics.composer.radius') }} (%)<input
                        class="field-input"
                        type="number"
                        min="0"
                        max="50"
                        step="0.1"
                        :value="selected.radius"
                        @change="emit('updateLayer', { radius: Number(($event.target as HTMLInputElement).value) })"
                /></label>
            </template>
            <label class="field-label"
                >{{ t('graphics.composer.opacity')
                }}<input
                    class="field-input"
                    type="number"
                    min="0"
                    max="1"
                    step="0.05"
                    :value="selected.opacity"
                    @change="emit('updateLayer', { opacity: Number(($event.target as HTMLInputElement).value) })"
            /></label>
            <div class="border-t border-line-strong pt-3 flex flex-col gap-3">
                <h3 class="m-0 text-sm font-semibold">{{ t('graphics.composer.animation') }}</h3>
                <label v-for="key in ['animation_in', 'animation_out'] as const" :key="key" class="field-label"
                    >{{ t('graphics.composer.' + key)
                    }}<select
                        class="field-input"
                        :value="selected[key]"
                        @change="emit('updateLayer', { [key]: ($event.target as HTMLSelectElement).value })"
                    >
                        <option v-for="animation in LAYER_ANIMATIONS" :key="animation" :value="animation">
                            {{ t('graphics.composer.animations.' + animation) }}
                        </option>
                    </select></label
                >
                <label v-for="key in ['animation_ms', 'delay_ms'] as const" :key="key" class="field-label"
                    >{{ t('graphics.composer.' + key) }} (ms)<input
                        class="field-input"
                        type="number"
                        min="0"
                        max="10000"
                        :value="selected[key]"
                        @change="emit('updateLayer', { [key]: Number(($event.target as HTMLInputElement).value) })"
                /></label>
            </div>
        </template>
        <slot />
    </aside>
</template>
