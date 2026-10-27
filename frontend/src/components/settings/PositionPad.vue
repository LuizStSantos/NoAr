<script setup lang="ts">
import { computed, ref, watch } from 'vue'

/**
 * A small screen to place the logo or the L-shape picture on: click or drag
 * the box, or move it with the arrow keys. Positions are shares of the room
 * around the box, in percent: 0 is the left (or top) edge, 100 the other.
 */
export interface Shares {
    x: number
    y: number
}

const props = defineProps<{
    /** Where the box is now. */
    shares: Shares
    /** Its size, in percent of the screen's width and height. */
    width: number
    height: number
    label: string
    disabled?: boolean
    /** Where the box is on air while `shares` is still to be confirmed. */
    ghost?: Shares | null
}>()

const emit = defineEmits<{ place: [Shares] }>()

/** Shares snap to an edge or the middle when this close to it. */
const SNAP = 4

const screen = ref<HTMLElement | null>(null)
const dragging = ref<Shares | null>(null)
const shown = computed(() => dragging.value ?? props.shares)

function placeAt(shares: Shares) {
    return {
        left: `${((100 - props.width) * shares.x) / 100}%`,
        top: `${((100 - props.height) * shares.y) / 100}%`,
        width: `${props.width}%`,
        height: `${props.height}%`,
    }
}

const box = computed(() => placeAt(shown.value))

function snap(value: number): number {
    const clamped = Math.min(100, Math.max(0, value))

    for (const mark of [0, 50, 100]) {
        if (Math.abs(clamped - mark) <= SNAP) {
            return mark
        }
    }

    return Math.round(clamped)
}

/** The shares that center the box under the pointer. */
function sharesAt(event: PointerEvent): Shares {
    const rect = screen.value!.getBoundingClientRect()
    const room = (size: number) => Math.max(1, 100 - size)
    const x = ((event.clientX - rect.left) / rect.width) * 100 - props.width / 2
    const y = ((event.clientY - rect.top) / rect.height) * 100 - props.height / 2

    return { x: snap((x / room(props.width)) * 100), y: snap((y / room(props.height)) * 100) }
}

function down(event: PointerEvent) {
    if (props.disabled || !screen.value) {
        return
    }

    screen.value.setPointerCapture(event.pointerId)
    dragging.value = sharesAt(event)
}

function move(event: PointerEvent) {
    if (dragging.value) {
        dragging.value = sharesAt(event)
    }
}

function up() {
    if (dragging.value) {
        emit('place', dragging.value)
        dragging.value = null
    }
}

function key(event: KeyboardEvent) {
    const step = event.shiftKey ? 1 : 5
    const moves: Record<string, [number, number]> = {
        ArrowLeft: [-step, 0],
        ArrowRight: [step, 0],
        ArrowUp: [0, -step],
        ArrowDown: [0, step],
        Home: [-100, 0],
        End: [100, 0],
    }
    const delta = moves[event.key]

    if (!delta || props.disabled) {
        return
    }

    event.preventDefault()
    const clamp = (value: number) => Math.min(100, Math.max(0, Math.round(value)))

    emit('place', { x: clamp(props.shares.x + delta[0]), y: clamp(props.shares.y + delta[1]) })
}

// A new position from outside ends a drag that has not been dropped.
watch(
    () => props.shares,
    () => {
        dragging.value = null
    },
)
</script>

<template>
    <div
        ref="screen"
        class="relative aspect-video w-full max-w-60 rounded-md border border-line-strong bg-field select-none touch-none overflow-hidden"
        :class="disabled ? 'opacity-60 cursor-not-allowed' : 'cursor-crosshair'"
        @pointerdown="down"
        @pointermove="move"
        @pointerup="up"
        @pointercancel="dragging = null"
    >
        <!-- Thirds, to aim by. -->
        <span class="absolute inset-y-0 left-1/3 w-px bg-line" aria-hidden="true" />
        <span class="absolute inset-y-0 left-2/3 w-px bg-line" aria-hidden="true" />
        <span class="absolute inset-x-0 top-1/3 h-px bg-line" aria-hidden="true" />
        <span class="absolute inset-x-0 top-2/3 h-px bg-line" aria-hidden="true" />
        <!-- Where it is on air, while the new place waits for a confirmation. -->
        <span
            v-if="ghost"
            class="absolute rounded-sm border-2 border-dashed border-muted/70"
            :style="placeAt(ghost)"
            aria-hidden="true"
        />
        <button
            type="button"
            role="slider"
            class="absolute rounded-sm border-2 border-accent bg-accent/30 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent"
            :class="dragging ? '' : 'transition-all duration-200'"
            :style="box"
            :disabled="disabled"
            :aria-label="label"
            aria-valuemin="0"
            aria-valuemax="100"
            :aria-valuenow="shares.x"
            :aria-valuetext="`${shares.x}% × ${shares.y}%`"
            @keydown="key"
        />
    </div>
</template>
