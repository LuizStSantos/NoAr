<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useElementSize } from '@vueuse/core'

import { SCHEDULE_COLORS, secondsToTime, formatLength } from '@/composables/schedule'
import {
    SCHEDULE_DAY_SECONDS as DAY,
    SCHEDULE_DRAG_TYPE as DRAG_TYPE,
    SCHEDULE_GRAPHICS_COLOR as GRAPHICS_COLOR,
    SCHEDULE_ZOOM_OPTIONS,
    type ScheduleGradeBlock,
    type ScheduleEventBlock,
    type ScheduleGraphicsBlock,
    useScheduledGraphicsText,
} from '@/composables/schedulePresentation'

/** Own only timeline interactions; loading, event mutations and drafts stay with the page. */
const props = defineProps<{
    gradeBlocks: ScheduleGradeBlock[]
    eventBlocks: ScheduleEventBlock[]
    graphicsBlocks: ScheduleGraphicsBlock[]
    zoom: number
    isToday: boolean
    nowSeconds: number
    activeEventId?: number
}>()
const emit = defineEmits<{
    drop: [payload: ScheduleDragItem, seconds: number]
    edit: [occurrence: ScheduleOccurrence]
    editGraphics: [event: GraphicsEvent]
}>()
const { t } = useI18n()
const { graphicsSummary } = useScheduledGraphicsText()
const LABEL_WIDTH = 112
const scroller = ref<HTMLElement>()
const { width: scrollerWidth } = useElementSize(scroller)
const ghost = ref<{ lane: 'grade' | 'events'; seconds: number; label: string } | null>(null)
const zoomTick = computed(() => SCHEDULE_ZOOM_OPTIONS.find((option) => option.minutes === props.zoom)?.tick ?? 3600)
const pxPerSecond = computed(() => Math.max(scrollerWidth.value - LABEL_WIDTH - 8, 300) / (props.zoom * 60))
const laneWidth = computed(() => DAY * pxPerSecond.value)
const ticks = computed(() => {
    const list = []

    for (let second = 0; second <= DAY; second += zoomTick.value) {
        list.push({ second, label: second === DAY ? '24:00' : secondsToTime(second).slice(0, 5) })
    }

    return list
})

function scrollToFocus(focus: number) {
    if (scroller.value) {
        scroller.value.scrollLeft = Math.max(0, (focus - props.zoom * 60 * 0.25) * pxPerSecond.value)
    }
}

function left(seconds: number): string {
    return `${Math.max(0, seconds) * pxPerSecond.value}px`
}

function width(start: number, end: number, minimum = 4): string {
    return `${Math.max(minimum, pixels(start, end))}px`
}

function pixels(start: number, end: number): number {
    return (Math.min(end, DAY) - Math.max(start, 0)) * pxPerSecond.value
}

/** Narrower than its text: the block keeps its true length and the label goes beside it. */
const LABEL_INSIDE_PX = 120

function narrow(start: number, end: number): boolean {
    return pixels(start, end) < LABEL_INSIDE_PX
}

function textColor(color: string): string {
    return color === SCHEDULE_COLORS.after_clip ? '#ffffff' : '#1c1c1c'
}

function laneSeconds(event: DragEvent): number {
    const lane = event.currentTarget as HTMLElement
    const offset = event.clientX - lane.getBoundingClientRect().left
    const step = props.zoom <= 60 ? 10 : 60

    return Math.min(DAY - step, Math.max(0, Math.round(offset / pxPerSecond.value / step) * step))
}

function readPayload(event: DragEvent): ScheduleDragItem | null {
    const raw = event.dataTransfer?.getData(DRAG_TYPE)

    try {
        return raw ? (JSON.parse(raw) as ScheduleDragItem) : null
    } catch {
        return null
    }
}

function dragOver(event: DragEvent, lane: 'grade' | 'events') {
    if (!event.dataTransfer?.types.includes(DRAG_TYPE)) {
        return
    }

    event.preventDefault()
    event.dataTransfer.dropEffect = event.dataTransfer.effectAllowed === 'move' ? 'move' : 'copy'
    const seconds = laneSeconds(event)
    ghost.value = {
        lane,
        seconds,
        label: t('schedule.dropAt', { time: secondsToTime(seconds).slice(0, props.zoom <= 60 ? 8 : 5) }),
    }
}

function dragLeave(event: DragEvent) {
    const lane = event.currentTarget as HTMLElement

    if (!lane.contains(event.relatedTarget as Node)) {
        ghost.value = null
    }
}

function drop(event: DragEvent) {
    const payload = readPayload(event)
    const seconds = laneSeconds(event)
    ghost.value = null

    if (!payload) {
        return
    }

    event.preventDefault()
    emit('drop', payload, seconds)
}

function dragEvent(event: DragEvent, occurrence: ScheduleOccurrence) {
    const payload: ScheduleDragItem = { type: 'event', id: occurrence.event.id }

    event.dataTransfer?.setData(DRAG_TYPE, JSON.stringify(payload))

    if (event.dataTransfer) {
        event.dataTransfer.effectAllowed = 'move'
    }
}

defineExpose({ scrollToFocus })
</script>
<template>
    <section :aria-label="t('schedule.timeline')" class="card-box px-4.5 py-4">
        <div class="flex flex-wrap gap-x-4.5 gap-y-2 text-[13px] text-base-content/85 mb-3.5">
            <span class="flex items-center gap-2"
                ><span class="w-3 h-3 rounded-[3px] bg-[#4a4a4a]" />{{ t('schedule.legendGrade') }}</span
            >
            <span class="flex items-center gap-2"
                ><span class="w-3 h-3 rounded-[3px]" :style="{ background: SCHEDULE_COLORS.after_clip }" />{{
                    t('schedule.afterClip')
                }}</span
            >
            <span class="flex items-center gap-2"
                ><span class="w-3 h-3 rounded-[3px]" :style="{ background: SCHEDULE_COLORS.interrupt }" />{{
                    t('schedule.interrupt')
                }}</span
            >
            <span class="flex items-center gap-2"
                ><span class="w-3 h-3 rounded-[3px]" :style="{ background: SCHEDULE_COLORS.live }" />{{
                    t('schedule.live')
                }}</span
            >
            <span class="flex items-center gap-2"
                ><span class="w-3 h-3 rounded-[3px]" :style="{ background: GRAPHICS_COLOR }" />{{
                    t('schedule.graphics.lane')
                }}</span
            >
        </div>

        <div ref="scroller" class="overflow-x-auto overflow-y-hidden pb-2">
            <div class="grid gap-y-2.5 w-max" :style="{ gridTemplateColumns: `${LABEL_WIDTH}px ${laneWidth}px` }">
                <div class="sticky left-0 z-20 bg-base-100" />
                <div class="relative h-5 font-mono text-[13px] text-muted">
                    <span
                        v-for="tick in ticks"
                        :key="tick.second"
                        class="absolute -translate-x-1/2 first:translate-x-0 whitespace-nowrap"
                        :style="{ left: left(tick.second) }"
                    >
                        {{ tick.label }}
                    </span>
                </div>

                <div class="sticky left-0 z-20 bg-base-100 text-[13px] text-base-content/85 flex items-center">
                    {{ t('schedule.grade') }}
                </div>
                <div
                    class="relative h-16 bg-base-200 rounded-lg"
                    @dragover="dragOver($event, 'grade')"
                    @dragleave="dragLeave"
                    @drop="drop"
                >
                    <div
                        v-for="(block, index) in gradeBlocks"
                        :key="`g${index}`"
                        class="absolute top-1.5 bottom-1.5 rounded-md px-2.5 py-1.5 overflow-hidden [overflow-wrap:normal] border-r-2 border-base-200"
                        :class="block.scheduled ? 'text-[#1c1c1c]' : 'bg-[#44464c] text-white'"
                        :style="{
                            left: left(block.start),
                            width: width(block.start, block.end, 2),
                            background: block.scheduled ? SCHEDULE_COLORS.live : undefined,
                        }"
                        :title="`${block.label}${block.detail ? ` — ${block.detail}` : ''} · ${secondsToTime(block.start)} – ${secondsToTime(block.end)}`"
                    >
                        <div class="text-sm font-semibold truncate">{{ block.label }}</div>
                        <div class="font-mono text-xs opacity-80 truncate">
                            {{ secondsToTime(block.start).slice(0, 5) }} –
                            {{ secondsToTime(block.end).slice(0, 5) }}
                        </div>
                    </div>
                    <p
                        v-if="!gradeBlocks.length"
                        class="absolute inset-0 flex items-center px-4 text-sm text-base-content/50"
                    >
                        {{ t('schedule.noPlaylist') }}
                    </p>
                    <template v-if="isToday">
                        <div
                            class="absolute -top-8 -bottom-21 w-0.5 bg-accent z-10 pointer-events-none"
                            :style="{ left: left(nowSeconds) }"
                        />
                        <div
                            class="absolute -top-8 -translate-x-1/2 bg-accent text-accent-content text-xs font-semibold px-1.5 rounded z-10 whitespace-nowrap pointer-events-none"
                            :style="{ left: left(nowSeconds) }"
                        >
                            {{ t('schedule.now') }} {{ secondsToTime(nowSeconds).slice(0, 5) }}
                        </div>
                    </template>
                </div>

                <div class="sticky left-0 z-20 bg-base-100 text-[13px] text-base-content/85 flex items-center">
                    {{ t('schedule.events') }}
                </div>
                <div
                    class="relative h-16 bg-base-200 rounded-lg"
                    @dragover="dragOver($event, 'events')"
                    @dragleave="dragLeave"
                    @drop="drop"
                >
                    <button
                        v-for="block in eventBlocks"
                        :key="`${block.occurrence.event.id}-${block.occurrence.start}`"
                        class="absolute top-1.5 bottom-1.5 rounded-md py-1 [overflow-wrap:normal] text-left cursor-grab border-0"
                        :class="{
                            'px-2.5 overflow-hidden': !narrow(block.start, block.end),
                            'px-0 overflow-visible hover:z-10': narrow(block.start, block.end),
                            'opacity-50': block.occurrence.status === 'done',
                            'ring-2 ring-error': block.occurrence.status === 'failed',
                            'ring-2 ring-white animate-pulse': activeEventId === block.occurrence.event.id,
                        }"
                        :style="{
                            left: left(block.start),
                            width: width(block.start, block.end),
                            background: block.color,
                            color: textColor(block.color),
                        }"
                        :title="`${block.occurrence.event.title} · ${secondsToTime(block.start)}`"
                        draggable="true"
                        @dragstart="dragEvent($event, block.occurrence)"
                        @click="emit('edit', block.occurrence)"
                    >
                        <div
                            :class="
                                narrow(block.start, block.end)
                                    ? 'absolute left-[calc(100%+4px)] top-0 bottom-0 flex flex-col justify-center px-2 rounded-md bg-base-300 text-base-content whitespace-nowrap border-l-3'
                                    : 'contents'
                            "
                            :style="narrow(block.start, block.end) ? { borderColor: block.color } : undefined"
                        >
                            <div class="text-sm font-semibold truncate flex items-center gap-1.5">
                                <span
                                    v-if="block.occurrence.event.kind === 'live'"
                                    class="bg-[#1c1c1c] text-[#ffb1a3] text-[11px] px-1 rounded"
                                >
                                    {{ t('schedule.liveBadge') }}
                                </span>
                                {{ block.occurrence.event.title }}
                            </div>
                            <div class="font-mono text-xs opacity-85 truncate">
                                {{ secondsToTime(block.start).slice(0, 5) }}
                                <template v-if="block.occurrence.event.kind === 'live'">
                                    – {{ secondsToTime(block.end).slice(0, 5) }}</template
                                >
                                <template v-else> · {{ formatLength(block.occurrence.event.duration) }}</template>
                            </div>
                        </div>
                    </button>
                    <div
                        v-if="ghost"
                        class="absolute top-1.5 bottom-1.5 w-34 rounded-md border-2 border-dashed border-accent bg-base-300 text-accent-text px-2 py-1 text-sm font-semibold whitespace-nowrap pointer-events-none z-20"
                        :style="{ left: left(ghost.seconds) }"
                    >
                        {{ ghost.label }}
                    </div>
                </div>

                <div class="sticky left-0 z-20 bg-base-100 text-[13px] text-base-content/85 flex items-center">
                    {{ t('schedule.graphics.lane') }}
                </div>
                <div class="relative h-12 bg-base-200 rounded-lg">
                    <button
                        v-for="block in graphicsBlocks"
                        :key="`gr-${block.event.id}`"
                        type="button"
                        class="absolute top-1.5 bottom-1.5 rounded-md [overflow-wrap:normal] text-left cursor-pointer border-0 text-[#16121f]"
                        :class="[
                            block.open ? 'rounded-e-none' : '',
                            !block.open && narrow(block.start, block.end)
                                ? 'px-0 overflow-visible hover:z-10'
                                : 'px-2.5 overflow-hidden',
                        ]"
                        :style="{
                            left: left(block.start),
                            width: width(block.start, block.end),
                            background: block.open
                                ? `linear-gradient(90deg, ${GRAPHICS_COLOR} 70%, transparent)`
                                : GRAPHICS_COLOR,
                        }"
                        :title="`${block.event.title} · ${graphicsSummary(block.event)}`"
                        @click="emit('editGraphics', block.event)"
                    >
                        <div
                            :class="
                                !block.open && narrow(block.start, block.end)
                                    ? 'absolute left-[calc(100%+4px)] top-0 bottom-0 flex flex-col justify-center px-2 rounded-md bg-base-300 text-base-content whitespace-nowrap border-l-3'
                                    : 'contents'
                            "
                            :style="
                                !block.open && narrow(block.start, block.end)
                                    ? { borderColor: GRAPHICS_COLOR }
                                    : undefined
                            "
                        >
                            <div class="text-[13px] font-semibold truncate leading-tight">
                                {{ block.event.title }}
                            </div>
                            <div class="font-mono text-[11px] opacity-85 truncate">
                                {{ block.event.start_time.slice(0, 5) }}
                                <template v-if="block.event.end_time">
                                    – {{ block.event.end_time.slice(0, 5) }}</template
                                >
                            </div>
                        </div>
                    </button>
                </div>
            </div>
        </div>
    </section>
</template>
