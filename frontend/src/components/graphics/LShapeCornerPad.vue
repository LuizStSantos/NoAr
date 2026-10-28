<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'

import { LSHAPE_CORNERS } from '@/composables/graphics'

/**
 * The corner the L-shape picture goes to, on a small screen: one corner per
 * button. `corner` is the one chosen; without one, `fallback` (the
 * channel's, or the one on air) is shown lighter and `fallbackLabel` names it.
 */
const props = defineProps<{
    corner: string | null | undefined
    fallback?: string
    fallbackLabel?: string
    label: string
}>()
const emit = defineEmits<{ choose: [corner: string] }>()

const { t } = useI18n()

const shown = computed(() => props.corner || props.fallback || '')
const caption = computed(() => {
    if (props.corner) {
        return t(`lshape.corners.${props.corner}`)
    }

    const fallback = props.fallback ? t(`lshape.corners.${props.fallback}`) : ''

    return [props.fallbackLabel, fallback].filter(Boolean).join(' · ')
})
</script>
<template>
    <div class="flex flex-col gap-1.5">
        <span class="text-xs text-muted">{{ label }}</span>
        <div
            role="radiogroup"
            :aria-label="label"
            class="grid grid-cols-2 gap-1.5 aspect-video w-36 rounded-md border border-line-strong bg-field p-1.5"
        >
            <button
                v-for="option in LSHAPE_CORNERS"
                :key="option"
                type="button"
                role="radio"
                class="rounded-sm border cursor-pointer"
                :class="
                    shown === option
                        ? corner
                            ? 'bg-accent border-accent'
                            : 'bg-accent/40 border-accent'
                        : 'bg-base-100 border-line-strong hover:border-accent'
                "
                :aria-checked="shown === option"
                :aria-label="t(`lshape.corners.${option}`)"
                :title="t(`lshape.corners.${option}`)"
                @click="emit('choose', option)"
            />
        </div>
        <span class="text-xs text-muted">{{ caption }}</span>
    </div>
</template>
