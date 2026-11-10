<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useRafFn, useResizeObserver } from '@vueuse/core'

import type { TemplateLayer } from '@/composables/graphicsTemplates'

/** Preview only: output uses a prepared glyph strip and the engine's take clock. */
const props = defineProps<{
    layer: TemplateLayer
    text: string
    outputWidth: number
    time?: number
    animate: boolean
}>()
const viewport = ref<HTMLElement>()
const measure = ref<HTMLElement>()
const viewportPixels = ref(0)
const textPixels = ref(0)
const elapsed = ref(0)
const started = performance.now()
const line = computed(() => props.text.split(/\s+/u).filter(Boolean).join(' '))

useResizeObserver([viewport, measure], () => {
    viewportPixels.value = viewport.value?.getBoundingClientRect().width ?? 0
    textPixels.value = measure.value?.getBoundingClientRect().width ?? 0
})
const { pause, resume } = useRafFn(
    () => {
        elapsed.value = (performance.now() - started) / 1000
    },
    { immediate: false },
)
watch(
    () => props.animate && props.time === undefined,
    (playing) => {
        if (playing) {
            resume()
        } else {
            pause()
        }
    },
    { immediate: true },
)

const offsets = computed(() => {
    if (!props.layer.scroll || !line.value || !viewportPixels.value) {
        return []
    }

    if (!props.animate && props.time === undefined) {
        return [0]
    }

    const width = (props.outputWidth * props.layer.width) / 100
    const scale = viewportPixels.value / width
    const textWidth = Math.max(2, Math.ceil(textPixels.value / scale / 2) * 2)
    const period = Math.max(2, textWidth + props.layer.scroll.gap)
    const time = Math.max(0, (props.time ?? elapsed.value) - props.layer.delay_ms / 1000)
    const distance = time * props.layer.scroll.speed
    const first = Math.max(0, Math.ceil((distance - width - textWidth) / period))
    const last = Math.floor(distance / period)

    return Array.from({ length: Math.max(0, last - first + 1) }, (_, index) => {
        const left = width - distance + (first + index) * period
        const x = props.layer.scroll?.direction === 'right' ? width - textWidth - left : left

        return Math.floor(x / 2) * 2 * scale
    })
})
</script>
<template>
    <span ref="viewport" class="relative w-full h-full block overflow-hidden" data-scroll-viewport>
        <span ref="measure" class="absolute invisible whitespace-pre leading-[1.2] w-max" aria-hidden="true">{{
            line
        }}</span>
        <span
            v-for="(offset, index) in offsets"
            :key="index"
            class="absolute top-1/2 whitespace-pre leading-[1.2] w-max"
            :style="{ transform: `translate(${offset}px, -50%)` }"
            data-scroll-text
            >{{ line }}</span
        >
    </span>
</template>
