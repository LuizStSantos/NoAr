<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'

/**
 * Timeline of a video with the part that airs between two handles: drag a
 * handle, or focus it and use the arrows (Shift for a second at a time).
 * A click elsewhere moves the play position.
 */
const props = defineProps<{
    /** Length of the video, in seconds. */
    duration: number
    from: number
    to: number
    /** Play position, when there is a video to play. */
    position?: number
}>()

const emit = defineEmits<{
    'update:from': [seconds: number]
    'update:to': [seconds: number]
    seek: [seconds: number]
}>()

const { t } = useI18n()

/** Shortest part that can air. */
const MIN_LENGTH = 0.1

const track = ref<HTMLElement>()
const dragging = ref<'from' | 'to' | null>(null)

function percent(seconds: number): string {
    return `${props.duration > 0 ? Math.min(100, Math.max(0, (seconds / props.duration) * 100)) : 0}%`
}

const fromPercent = computed(() => percent(props.from))
const toPercent = computed(() => percent(props.to))

/** Tenths of a second kept. */
function round(seconds: number): number {
    return Math.round(seconds * 10) / 10
}

function clamp(edge: 'from' | 'to', seconds: number): number {
    return edge === 'from'
        ? Math.min(Math.max(0, seconds), props.to - MIN_LENGTH)
        : Math.max(Math.min(props.duration, seconds), props.from + MIN_LENGTH)
}

function set(edge: 'from' | 'to', seconds: number) {
    const value = round(clamp(edge, seconds))

    if (edge === 'from') {
        emit('update:from', value)
    } else {
        emit('update:to', value)
    }
}

function secondsAt(clientX: number): number {
    const box = track.value?.getBoundingClientRect()

    if (!box || !box.width) {
        return 0
    }

    return ((clientX - box.left) / box.width) * props.duration
}

function grab(edge: 'from' | 'to', event: PointerEvent) {
    dragging.value = edge
    ;(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId)
    event.preventDefault()
}

function drag(event: PointerEvent) {
    if (dragging.value) {
        set(dragging.value, secondsAt(event.clientX))
        emit('seek', dragging.value === 'from' ? props.from : props.to)
    }
}

function release(event: PointerEvent) {
    if (dragging.value) {
        set(dragging.value, secondsAt(event.clientX))
        emit('seek', dragging.value === 'from' ? props.from : props.to)
    }

    dragging.value = null
}

function key(edge: 'from' | 'to', event: KeyboardEvent) {
    const step = event.shiftKey ? 1 : 0.1
    const current = edge === 'from' ? props.from : props.to
    const moves: Record<string, number> = {
        ArrowLeft: current - step,
        ArrowDown: current - step,
        ArrowRight: current + step,
        ArrowUp: current + step,
        Home: edge === 'from' ? 0 : props.from + MIN_LENGTH,
        End: edge === 'from' ? props.to - MIN_LENGTH : props.duration,
    }

    if (event.key in moves) {
        event.preventDefault()
        set(edge, moves[event.key]!)
        emit('seek', edge === 'from' ? props.from : props.to)
    }
}

function click(event: MouseEvent) {
    if (props.duration > 0) {
        emit('seek', round(Math.min(props.duration, Math.max(0, secondsAt(event.clientX)))))
    }
}

/** `1:02:03.4` or `02:03.4`. */
function label(seconds: number): string {
    const total = Math.max(0, Math.round(seconds * 10)) / 10
    const h = Math.floor(total / 3600)
    const m = Math.floor((total % 3600) / 60)
    const s = (total % 60).toFixed(1).padStart(4, '0')

    return h ? `${h}:${String(m).padStart(2, '0')}:${s}` : `${String(m).padStart(2, '0')}:${s}`
}
</script>
<template>
    <div
        ref="track"
        class="relative h-12 rounded-md bg-field border border-line select-none touch-none"
        :class="duration > 0 ? 'cursor-pointer' : 'opacity-60'"
        role="presentation"
        @click="click"
        @pointermove="drag"
        @pointerup="release"
        @pointercancel="dragging = null"
    >
        <!-- What is cut off, before and after. -->
        <span
            class="absolute inset-y-0 left-0 rounded-l-md bg-base-content/15 bg-[repeating-linear-gradient(135deg,transparent_0_6px,rgb(0_0_0/0.12)_6px_12px)]"
            :style="{ width: fromPercent }"
        />
        <span
            class="absolute inset-y-0 right-0 rounded-r-md bg-base-content/15 bg-[repeating-linear-gradient(135deg,transparent_0_6px,rgb(0_0_0/0.12)_6px_12px)]"
            :style="{ left: toPercent }"
        />
        <!-- What airs. -->
        <span
            class="absolute inset-y-0 bg-accent-soft border-y-2 border-accent pointer-events-none"
            :style="{ left: fromPercent, width: `calc(${toPercent} - ${fromPercent})` }"
        />
        <span
            v-if="position !== undefined"
            class="absolute inset-y-0 w-0.5 bg-base-content pointer-events-none"
            :style="{ left: percent(position) }"
        />
        <button
            v-for="edge in ['from', 'to'] as const"
            :key="edge"
            type="button"
            role="slider"
            :aria-label="edge === 'from' ? t('storage.trim.in') : t('storage.trim.out')"
            :aria-valuemin="0"
            :aria-valuemax="Math.round(duration * 10) / 10"
            :aria-valuenow="edge === 'from' ? from : to"
            :aria-valuetext="label(edge === 'from' ? from : to)"
            :disabled="duration <= 0"
            class="absolute -top-1 -bottom-1 w-4 -translate-x-1/2 rounded bg-accent border-2 border-base-100 shadow cursor-ew-resize flex items-center justify-center p-0 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent"
            :style="{ left: edge === 'from' ? fromPercent : toPercent }"
            @pointerdown="grab(edge, $event)"
            @click.stop
            @keydown="key(edge, $event)"
        >
            <span class="w-0.5 h-4 bg-base-100/80 rounded" aria-hidden="true" />
        </button>
    </div>
</template>
