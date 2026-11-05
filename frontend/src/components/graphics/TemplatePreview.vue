<script setup lang="ts">
import { computed, ref, watch } from 'vue'

import { storageImageUrl } from '@/composables/logo'
import { type GraphicsTemplate, type TemplateLayer, fillFields } from '@/composables/graphicsTemplates'
import { useConfig } from '@/stores/config'

/**
 * A template drawn in the browser, at 16:9, the way the engine places it:
 * boxes, texts and images in percent of the frame, sizes in percent of its
 * height. An approximation for choosing and checking, not the air.
 */
const props = defineProps<{
    template: GraphicsTemplate | null | undefined
    values?: Record<string, string>
    time?: number
}>()

const configStore = useConfig()
const images = ref<Record<string, string>>({})
let imageVersion = 0

const layers = computed(() => (props.template?.layers ?? []).filter((layer) => layer.visible))

function imagePath(layer: TemplateLayer): string {
    const image = layer.image?.trim() ?? ''
    const field = /^\{([\p{L}\p{N}_-]+)\}$/u.exec(image)

    return field ? (props.values?.[field[1]!]?.trim() ?? '') : image
}

watch(
    () => layers.value.filter((layer) => layer.kind === 'image').map(imagePath),
    async (paths) => {
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
        background: layer.kind === 'image' ? undefined : (layer.fill ?? undefined),
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
</script>
<template>
    <div class="relative w-full aspect-video overflow-hidden [container-type:size]">
        <slot />
        <div
            v-for="layer in layers"
            :key="layer.id"
            class="absolute overflow-hidden flex items-center"
            :style="boxStyle(layer)"
        >
            <span
                v-if="layer.kind === 'video'"
                class="w-full h-full flex items-center justify-center bg-slate-700 text-white border border-white/40 text-[3cqh]"
                >VIDEO</span
            >
            <span
                v-if="layer.kind === 'text'"
                class="w-full flex leading-[1.2] whitespace-pre-wrap break-words"
                :style="textStyle(layer)"
            >
                {{ fillFields(layer.text, props.values ?? {}) }}
            </span>
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
