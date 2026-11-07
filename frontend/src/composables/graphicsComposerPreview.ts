import { computed, onBeforeUnmount, ref } from 'vue'

import type { TemplateInput } from '@/composables/graphicsTemplates'

/** Own the preview clock and cancel its animation frame when the editor is disposed. */
export function useComposerPreview(getTemplate: () => Pick<TemplateInput, 'duration' | 'layers'>) {
    const time = ref<number>()
    const playing = ref(false)
    const timelineEnd = computed(
        () =>
            Math.max(
                getTemplate().duration || 5,
                ...getTemplate().layers.map((layer) => (layer.delay_ms + layer.animation_ms) / 1000),
            ) + Math.max(0.4, ...getTemplate().layers.map((layer) => layer.animation_ms / 1000)),
    )
    let animationFrame = 0

    function seekPreview(seconds: number) {
        stopPreview()
        time.value = seconds
    }

    function stopPreview() {
        cancelAnimationFrame(animationFrame)
        playing.value = false
        time.value = undefined
    }

    function playPreview() {
        if (playing.value) {
            stopPreview()
            return
        }

        const start = performance.now()
        playing.value = true
        const tick = (now: number) => {
            time.value = (now - start) / 1000

            if (time.value < timelineEnd.value) {
                animationFrame = requestAnimationFrame(tick)
            } else {
                playing.value = false
            }
        }

        animationFrame = requestAnimationFrame(tick)
    }

    onBeforeUnmount(stopPreview)

    return { time, playing, timelineEnd, seekPreview, stopPreview, playPreview }
}
