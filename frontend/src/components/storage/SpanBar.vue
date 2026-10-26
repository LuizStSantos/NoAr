<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import type { LogoSpan } from '@/composables/storage'

/**
 * Timeline of a video with stretches on it (where the logo is off, or the
 * ad breaks), each between two handles like the trim: drag a handle, or
 * focus it and use the arrows (Shift for a second at a time). A stretch
 * cannot run into the next one. A click elsewhere moves the play position.
 */
const props = defineProps<{
    /** Length of the video, in seconds. */
    duration: number
    spans: LogoSpan[]
    /** `logo` for stretches without logo, `break` for ad breaks. */
    tone: 'logo' | 'break'
    /** Play position, when there is a video to play. */
    position?: number
    /** Where a stretch being marked started, while it is open. */
    pending?: number | null
    /** The part of the video that airs; the rest is shown cut off. */
    trim?: LogoSpan | null
    /** The stretch picked in the list. */
    selected?: number | null
}>()

const emit = defineEmits<{
    change: [index: number, span: LogoSpan]
    seek: [seconds: number]
    select: [index: number]
}>()

const { t } = useI18n()

const MIN_LENGTH = 0.1

type Edge = 'from' | 'to'

const track = ref<HTMLElement>()
const dragging = ref<{ index: number; edge: Edge } | null>(null)

const fill = computed(() => (props.tone === 'logo' ? 'bg-after-clip/35 border-after-clip' : 'bg-live/35 border-live'))
const handle = computed(() => (props.tone === 'logo' ? 'bg-after-clip' : 'bg-live'))

function percent(seconds: number): string {
    return `${props.duration > 0 ? Math.min(100, Math.max(0, (seconds / props.duration) * 100)) : 0}%`
}

function round(seconds: number): number {
    return Math.round(seconds * 10) / 10
}

/** The ends of the stretches before and after `index`, by time. */
function neighbours(index: number): { before: number; after: number } {
    const span = props.spans[index]!
    let before = 0
    let after = props.duration > 0 ? props.duration : Number.POSITIVE_INFINITY

    props.spans.forEach((other, position) => {
        if (position === index) {
            return
        }

        if (other.to <= span.from + 1e-6) {
            before = Math.max(before, other.to)
        } else if (other.from >= span.to - 1e-6) {
            after = Math.min(after, other.from)
        }
    })

    return { before, after }
}

function set(index: number, edge: Edge, seconds: number) {
    const span = props.spans[index]

    if (!span) {
        return
    }

    const { before, after } = neighbours(index)
    const value =
        edge === 'from'
            ? round(Math.min(Math.max(before, seconds), span.to - MIN_LENGTH))
            : round(Math.max(Math.min(after, seconds), span.from + MIN_LENGTH))

    emit('change', index, edge === 'from' ? { from: value, to: span.to } : { from: span.from, to: value })
}

function secondsAt(clientX: number): number {
    const box = track.value?.getBoundingClientRect()

    return box?.width ? ((clientX - box.left) / box.width) * props.duration : 0
}

function grab(index: number, edge: Edge, event: PointerEvent) {
    dragging.value = { index, edge }
    emit('select', index)
    ;(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId)
    event.preventDefault()
}

function drag(event: PointerEvent) {
    const current = dragging.value

    if (current) {
        set(current.index, current.edge, secondsAt(event.clientX))
        emit('seek', props.spans[current.index]![current.edge])
    }
}

function release(event: PointerEvent) {
    drag(event)
    dragging.value = null
}

function key(index: number, edge: Edge, event: KeyboardEvent) {
    const span = props.spans[index]!
    const step = event.shiftKey ? 1 : 0.1
    const current = span[edge]
    const moves: Record<string, number> = {
        ArrowLeft: current - step,
        ArrowDown: current - step,
        ArrowRight: current + step,
        ArrowUp: current + step,
    }

    if (event.key in moves) {
        event.preventDefault()
        set(index, edge, moves[event.key]!)
        emit('select', index)
        emit('seek', props.spans[index]![edge])
    }
}

function click(event: MouseEvent) {
    if (props.duration > 0) {
        emit('seek', round(Math.min(props.duration, Math.max(0, secondsAt(event.clientX)))))
    }
}

function pick(index: number) {
    emit('select', index)
    emit('seek', props.spans[index]!.from)
}

/** `1:02:03.4` or `02:03.4`. */
function label(seconds: number): string {
    const total = Math.max(0, Math.round(seconds * 10)) / 10
    const h = Math.floor(total / 3600)
    const m = Math.floor((total % 3600) / 60)
    const s = (total % 60).toFixed(1).padStart(4, '0')

    return h ? `${h}:${String(m).padStart(2, '0')}:${s}` : `${String(m).padStart(2, '0')}:${s}`
}

function handleLabel(index: number, edge: Edge): string {
    const key = props.tone === 'logo' ? 'storage.logoMarks' : 'storage.breakMarks'

    return `${t(`${key}.${edge}`)} · ${index + 1}`
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
        <!-- The parts the trim cuts off. -->
        <template v-if="trim">
            <span
                class="absolute inset-y-0 left-0 rounded-l-md bg-base-content/10 bg-[repeating-linear-gradient(135deg,transparent_0_6px,rgb(0_0_0/0.1)_6px_12px)] pointer-events-none"
                :style="{ width: percent(trim.from) }"
            />
            <span
                class="absolute inset-y-0 right-0 rounded-r-md bg-base-content/10 bg-[repeating-linear-gradient(135deg,transparent_0_6px,rgb(0_0_0/0.1)_6px_12px)] pointer-events-none"
                :style="{ left: percent(trim.to) }"
            />
        </template>
        <span
            v-for="(span, index) in spans"
            :key="`span-${index}`"
            class="absolute inset-y-1 border-y-2 rounded-sm"
            :class="[fill, selected === index ? 'ring-2 ring-accent ring-offset-1 ring-offset-field' : '']"
            :style="{ left: percent(span.from), width: `calc(${percent(span.to)} - ${percent(span.from)})` }"
            @click.stop="pick(index)"
        />
        <span
            v-if="pending !== null && pending !== undefined && position !== undefined && position > pending"
            class="absolute inset-y-1 border-y-2 border-dashed rounded-sm pointer-events-none"
            :class="fill"
            :style="{ left: percent(pending), width: `calc(${percent(position)} - ${percent(pending)})` }"
        />
        <span
            v-if="position !== undefined"
            class="absolute inset-y-0 w-0.5 bg-base-content pointer-events-none"
            :style="{ left: percent(position) }"
        />
        <template v-for="(span, index) in spans" :key="`handles-${index}`">
            <button
                v-for="edge in ['from', 'to'] as const"
                :key="edge"
                type="button"
                role="slider"
                :aria-label="handleLabel(index, edge)"
                :aria-valuemin="0"
                :aria-valuemax="Math.round(duration * 10) / 10"
                :aria-valuenow="span[edge]"
                :aria-valuetext="label(span[edge])"
                :disabled="duration <= 0"
                class="absolute -top-1 -bottom-1 w-3.5 -translate-x-1/2 rounded border-2 border-base-100 shadow cursor-ew-resize flex items-center justify-center p-0 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent"
                :class="handle"
                :style="{ left: percent(span[edge]) }"
                @pointerdown="grab(index, edge, $event)"
                @click.stop
                @keydown="key(index, edge, $event)"
            >
                <span class="w-0.5 h-4 bg-base-100/80 rounded" aria-hidden="true" />
            </button>
        </template>
    </div>
</template>
