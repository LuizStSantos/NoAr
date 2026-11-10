<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'

import TemplatePreview from '@/components/graphics/TemplatePreview.vue'
import { type GraphicsTemplate, useGraphicsApi } from '@/composables/graphicsTemplates'
import type { GraphicsCue } from '@/composables/storage'

/**
 * The graphics a video's cues put on air at the play position, drawn over the
 * video in the browser: an approximation for checking times and placement,
 * not the air. An L-shape's video window stays see-through instead of
 * shrinking the video.
 */
const props = defineProps<{ cues: GraphicsCue[]; position: number }>()

const api = useGraphicsApi()
const templates = ref<GraphicsTemplate[]>([])

onMounted(async () => {
    try {
        templates.value = await api.templates()
    } catch {
        templates.value = []
    }
})

/** Same rules as on air: a take without its own time uses the template's; 0 stays until taken off. */
const active = computed(() => {
    const onAir = new Map<number, { start: number; end: number }>()

    for (const cue of [...props.cues].sort((a, b) => a.at - b.at)) {
        if (cue.at > props.position) {
            break
        }

        if (cue.action === 'out') {
            onAir.delete(cue.template_id)
            continue
        }

        const template = templates.value.find((item) => item.id === cue.template_id)
        const seconds = cue.duration ?? template?.duration ?? 0
        onAir.set(cue.template_id, { start: cue.at, end: seconds > 0 ? cue.at + seconds : Infinity })
    }

    return [...onAir.entries()].flatMap(([id, span]) => {
        const template = templates.value.find((item) => item.id === id)

        if (!template || props.position >= span.end) {
            return []
        }

        const shown = span.end === Infinity ? 0 : span.end - span.start

        return [{ template: { ...template, duration: shown }, time: props.position - span.start }]
    })
})
</script>
<template>
    <div v-if="active.length" class="absolute inset-0 pointer-events-none" data-testid="graphics-cues-overlay">
        <TemplatePreview
            v-for="item in active"
            :key="item.template.id"
            :template="item.template"
            :time="item.time"
            transparent
            class="absolute inset-0"
        />
    </div>
</template>
