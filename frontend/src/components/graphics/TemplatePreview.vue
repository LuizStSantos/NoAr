<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useGraphicsFonts } from '@/composables/graphicsFonts'
import ScrollingText from '@/components/graphics/ScrollingText.vue'

import { sourceBindings, sourceValues, useGraphicsSources } from '@/composables/graphicsSources'
import { useGraphicsClock } from '@/composables/graphicsClock'
import { storageImageUrl } from '@/composables/graphicsImages'
import { type GraphicsTemplate, type TemplateLayer, fillFields } from '@/composables/graphicsTemplates'
import { useConfig } from '@/stores/config'

/**
 * A template drawn in the browser, at 16:9, the way the engine places it:
 * boxes, texts and images in percent of the frame, sizes in percent of its
 * height. An approximation for choosing and checking, not the air.
 */
const props = withDefaults(
    defineProps<{
        template: GraphicsTemplate | null | undefined
        values?: Record<string, string>
        records?: Record<string, number>
        time?: number
        animateText?: boolean
    }>(),
    { animateText: true },
)

const { t } = useI18n()
const { fontError } = useGraphicsFonts(() => props.template?.layers ?? [])
const { sources } = useGraphicsSources(() => Object.keys(sourceBindings(props.template)).length > 0)
const effectiveValues = computed(() => ({
    ...sourceValues(props.template, sources.value, props.records),
    ...Object.fromEntries(Object.entries(props.values ?? {}).filter(([, value]) => value !== '')),
}))
const dataMissing = computed(() =>
    Object.keys(sourceBindings(props.template)).some((field) => effectiveValues.value[field] === undefined),
)
const configStore = useConfig()
const { clockValue } = useGraphicsClock()
const images = ref<Record<string, string>>({})
let imageVersion = 0

const layers = computed(() => (props.template?.layers ?? []).filter((layer) => layer.visible))

function imagePath(layer: TemplateLayer): string {
    const image = layer.image?.trim() ?? ''
    const field = /^\{([\p{L}\p{N}_-]+)\}$/u.exec(image)

    return field ? (effectiveValues.value[field[1]!]?.trim() ?? '') : image
}

watch(
    () =>
        JSON.stringify([
            configStore.channels[configStore.i]?.id,
            ...layers.value.filter((layer) => layer.kind === 'image').map(imagePath),
        ]),
    async (serialized) => {
        const [, ...paths] = JSON.parse(serialized) as [number, ...string[]]
        const version = ++imageVersion
        const urls: Record<string, string> = {}

        for (const path of paths) {
            if (path && !(path in urls)) {
                urls[path] = await storageImageUrl(configStore.channels[configStore.i], path).catch(() => '')
            }
        }

        if (version === imageVersion) images.value = urls
    },
    { immediate: true },
)

function boxStyle(layer: TemplateLayer) {
    const elapsed = props.time === undefined ? undefined : props.time * 1000 - layer.delay_ms
    const duration = Math.max(1, layer.animation_ms)
    const progress = elapsed === undefined ? 1 : Math.max(0, Math.min(1, elapsed / duration))
    const leaving = props.time !== undefined && !!props.template?.duration && props.time >= props.template.duration
    const amount = leaving ? Math.max(0, 1 - ((props.time! - props.template!.duration) * 1000) / duration) : progress
    const animation = leaving ? layer.animation_out : layer.animation_in
    const offset = (1 - amount) * 100
    const transform =
        animation === 'scale'
            ? `scale(${amount})`
            : {
                  slide_left: `translateX(-${offset}%)`,
                  slide_right: `translateX(${offset}%)`,
                  slide_up: `translateY(${offset}%)`,
                  slide_down: `translateY(-${offset}%)`,
              }[animation as 'slide_left']
    return {
        left: `${layer.x}%`,
        top: `${layer.y}%`,
        width: `${layer.width}%`,
        height: `${layer.height}%`,
        opacity: String(
            layer.opacity *
                (elapsed !== undefined && elapsed < 0
                    ? 0
                    : animation === 'none'
                      ? leaving
                          ? 0
                          : 1
                      : animation === 'wipe'
                        ? 1
                        : amount),
        ),
        transform: elapsed === undefined ? undefined : transform,
        clipPath: elapsed !== undefined && animation === 'wipe' ? `inset(0 ${offset}% 0 0)` : undefined,
        borderRadius: `${layer.radius}cqh`,
        background: layer.kind === 'image' || layer.kind === 'box' ? undefined : (layer.fill ?? undefined),
    }
}

function textStyle(layer: TemplateLayer) {
    return {
        color: layer.color,
        fontSize: `${layer.font_size}cqh`,
        fontWeight: layer.bold ? '700' : '400',
        fontFamily: layer.font_family || undefined,
        textAlign: layer.align,
        justifyContent: { left: 'flex-start', center: 'center', right: 'flex-end' }[layer.align],
        padding: layer.fill ? `0 ${layer.font_size * 0.4}cqh` : undefined,
    }
}
function shapeSize(layer: TemplateLayer) {
    const output = configStore.playout.output
    return {
        width: ((output?.width || 1920) * layer.width) / 100,
        height: ((output?.height || 1080) * layer.height) / 100,
        stroke: ((output?.height || 1080) * (layer.stroke_width ?? 0.25)) / 100,
        radius: ((output?.height || 1080) * layer.radius) / 100,
    }
}
</script>
<template>
    <div class="graphics-preview relative w-full aspect-video overflow-hidden [container-type:size]">
        <slot />
        <p
            v-if="dataMissing"
            role="status"
            class="absolute z-20 bottom-1 left-1 right-1 m-0 p-1 bg-base-100/90 text-error text-xs"
        >
            {{ t('graphics.sources.noData') }}
        </p>
        <p
            v-if="fontError"
            role="status"
            class="absolute z-20 top-1 left-1 right-1 m-0 bg-base-100/90 p-1 text-xs text-error"
        >
            {{ t('graphics.composer.fontUnavailable') }}
        </p>
        <div
            v-for="layer in layers"
            :key="layer.id"
            class="absolute overflow-hidden flex items-center"
            :style="boxStyle(layer)"
        >
            <ScrollingText
                v-if="layer.kind === 'text' && layer.scroll"
                :layer="layer"
                :text="fillFields(layer.text, effectiveValues)"
                :output-width="configStore.playout.output?.width || 1920"
                :time="time"
                :animate="animateText !== false"
                :style="{
                    color: layer.color,
                    fontSize: `${layer.font_size}cqh`,
                    fontWeight: layer.bold ? '700' : '400',
                    fontFamily: layer.font_family || undefined,
                }"
            />
            <span
                v-if="layer.kind === 'video'"
                class="w-full h-full flex items-center justify-center bg-slate-700 text-white border border-white/40 text-[3cqh]"
                >VIDEO</span
            >
            <span
                v-if="(layer.kind === 'text' && !layer.scroll) || layer.kind === 'clock'"
                class="w-full flex leading-[1.2] whitespace-pre-wrap break-words"
                :style="textStyle(layer)"
            >
                {{ layer.kind === 'clock' ? clockValue(layer) : fillFields(layer.text, effectiveValues) }}
            </span>
            <svg
                v-if="layer.kind === 'box'"
                class="w-full h-full"
                :viewBox="`0 0 ${shapeSize(layer).width} ${shapeSize(layer).height}`"
                preserveAspectRatio="none"
                aria-hidden="true"
            >
                <line
                    v-if="layer.shape === 'line'"
                    x1="0"
                    :y1="shapeSize(layer).height / 2"
                    :x2="shapeSize(layer).width"
                    :y2="shapeSize(layer).height / 2"
                    :stroke="layer.stroke || layer.fill || 'none'"
                    :stroke-width="shapeSize(layer).stroke"
                />
                <ellipse
                    v-else-if="layer.shape === 'ellipse'"
                    :cx="shapeSize(layer).width / 2"
                    :cy="shapeSize(layer).height / 2"
                    :rx="Math.max(0, (shapeSize(layer).width - (layer.stroke ? shapeSize(layer).stroke : 0)) / 2)"
                    :ry="Math.max(0, (shapeSize(layer).height - (layer.stroke ? shapeSize(layer).stroke : 0)) / 2)"
                    :fill="layer.fill || 'none'"
                    :stroke="layer.stroke || 'none'"
                    :stroke-width="shapeSize(layer).stroke"
                />
                <rect
                    v-else
                    :x="layer.stroke ? shapeSize(layer).stroke / 2 : 0"
                    :y="layer.stroke ? shapeSize(layer).stroke / 2 : 0"
                    :width="Math.max(0, shapeSize(layer).width - (layer.stroke ? shapeSize(layer).stroke : 0))"
                    :height="Math.max(0, shapeSize(layer).height - (layer.stroke ? shapeSize(layer).stroke : 0))"
                    :rx="Math.max(0, shapeSize(layer).radius - (layer.stroke ? shapeSize(layer).stroke / 2 : 0))"
                    :fill="layer.fill || 'none'"
                    :stroke="layer.stroke || 'none'"
                    :stroke-width="shapeSize(layer).stroke"
                />
            </svg>
            <img
                v-else-if="layer.kind === 'image' && images[imagePath(layer)]"
                :src="images[imagePath(layer)]"
                alt=""
                class="w-full h-full object-fill"
            />
        </div>
        <slot name="overlay" />
    </div>
</template>

<style scoped>
.graphics-preview {
    --graphics-preview-checker: conic-gradient(#30343a 25%, transparent 0 50%, #30343a 0 75%, transparent 0);
    background-color: #24272c;
    background-image: var(--graphics-preview-checker);
    background-size: 20px 20px;
}
</style>
