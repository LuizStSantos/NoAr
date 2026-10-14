<script setup lang="ts">
import dayjs from 'dayjs'
import timezone from 'dayjs/plugin/timezone.js'
import utc from 'dayjs/plugin/utc.js'
import 'dayjs/locale/de'
import 'dayjs/locale/pt-br'
import 'dayjs/locale/ru'

import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useElementSize } from '@vueuse/core'
import { storeToRefs } from 'pinia'
import { useHead } from '@unhead/vue'

import GenericModal from '@/components/utils/GenericModal.vue'
import LiveDialog from '@/components/schedule/LiveDialog.vue'
import ScheduleDialog from '@/components/schedule/ScheduleDialog.vue'
import StoragePicker from '@/components/schedule/StoragePicker.vue'
import {
    SCHEDULE_COLORS,
    addDays,
    eventColor,
    fileName,
    fileStem,
    formatLength,
    parentFolder,
    secondsFromDay,
    secondsToTime,
    splitDateTime,
    useScheduleText,
} from '@/composables/schedule'
import { useConfig } from '@/stores/config'
import { useIndex } from '@/stores/index'
import { useSchedule } from '@/stores/schedule'

dayjs.extend(utc)
dayjs.extend(timezone)

const { t, locale } = useI18n()
const { repeatLabel, entryLabel, afterLabel } = useScheduleText()
const configStore = useConfig()
const indexStore = useIndex()
const scheduleStore = useSchedule()
const { i } = storeToRefs(configStore)

useHead({ title: computed(() => t('button.schedule')) })

const DAY = 86400
const LABEL_WIDTH = 112
const DRAG_TYPE = 'application/x-noar-schedule'

const zoomOptions = [
    { minutes: 60, label: '1 h', tick: 600 },
    { minutes: 360, label: '6 h', tick: 3600 },
    { minutes: 1440, label: '24 h', tick: 7200 },
]

const tab = ref<'storage' | 'lives'>('storage')
const zoom = ref(360)
const date = ref(channelNow().format('YYYY-MM-DD'))
const nowSeconds = ref(secondsOfDay())
const scroller = ref<HTMLElement>()
const { width: scrollerWidth } = useElementSize(scroller)
const ghost = ref<{ lane: 'grade' | 'events'; seconds: number; label: string } | null>(null)
const mediaDraft = ref<MediaDraft | null>(null)
const liveDraft = ref<LiveDraft | null>(null)
const deleting = ref<ScheduleEvent | null>(null)

let refreshTimer: ReturnType<typeof setInterval> | undefined
let clockTimer: ReturnType<typeof setInterval> | undefined

function channelNow() {
    return dayjs().tz(configStore.timezone || 'UTC')
}

function secondsOfDay(): number {
    const now = channelNow()

    return now.hour() * 3600 + now.minute() * 60 + now.second()
}

const today = computed(() => channelNow().format('YYYY-MM-DD'))
const isToday = computed(() => date.value === today.value)
const dateLabel = computed(() =>
    dayjs(`${date.value}T12:00:00`).locale(locale.value.toLowerCase()).format('ddd, D MMM YYYY'),
)
const zoomTick = computed(() => zoomOptions.find((option) => option.minutes === zoom.value)?.tick ?? 3600)
const pxPerSecond = computed(() => Math.max(scrollerWidth.value - LABEL_WIDTH - 8, 300) / (zoom.value * 60))
const laneWidth = computed(() => DAY * pxPerSecond.value)

const ticks = computed(() => {
    const list = []

    for (let second = 0; second <= DAY; second += zoomTick.value) {
        list.push({ second, label: second === DAY ? '24:00' : secondsToTime(second).slice(0, 5) })
    }

    return list
})

interface GradeBlock {
    start: number
    end: number
    label: string
    detail: string | null
    scheduled: boolean
}

/** The day's programme from the guide: program names and times moved by events. */
const gradeBlocks = computed<GradeBlock[]>(() =>
    scheduleStore.grade.map((block) => ({
        start: secondsFromDay(block.start, date.value),
        end: secondsFromDay(block.stop, date.value),
        label: block.title,
        detail: block.sub_title,
        scheduled: block.scheduled,
    })),
)

/** When an event that waits for the clip on air will really start. */
function expectedStart(occurrence: ScheduleOccurrence): number {
    const start = secondsFromDay(occurrence.start, date.value)

    if (occurrence.event.entry_mode !== 'after_clip') {
        return start
    }

    for (const item of scheduleStore.program) {
        const itemStart = secondsFromDay(item.start, date.value)
        const itemEnd = itemStart + (item.out - item.in)

        if (itemStart < start && start < itemEnd) {
            return itemEnd
        }
    }

    return start
}

const eventBlocks = computed(() =>
    scheduleStore.occurrences.map((occurrence) => {
        const nominal = secondsFromDay(occurrence.start, date.value)
        const start = expectedStart(occurrence)
        const end = Math.max(start + 1, secondsFromDay(occurrence.end, date.value) + (start - nominal))

        return { occurrence, nominal, start, end, color: eventColor(occurrence.event) }
    }),
)

function left(seconds: number): string {
    return `${Math.max(0, seconds) * pxPerSecond.value}px`
}

function width(start: number, end: number, minimum = 4): string {
    return `${Math.max(minimum, (Math.min(end, DAY) - Math.max(start, 0)) * pxPerSecond.value)}px`
}

function textColor(color: string): string {
    return color === SCHEDULE_COLORS.after_clip ? '#ffffff' : '#1c1c1c'
}

function occurrenceState(occurrence: ScheduleOccurrence): { label: string; tone: string } {
    const active = scheduleStore.status.active
    const start = secondsFromDay(occurrence.start, date.value)

    if (active && active.event_id === occurrence.event.id && occurrence.status === 'running') {
        return { label: t('schedule.onAir'), tone: 'accent' }
    }

    if (occurrence.status === 'done') {
        return { label: t('schedule.done'), tone: 'muted' }
    }

    if (occurrence.status === 'failed' || occurrence.status === 'skipped') {
        return { label: t('schedule.failed'), tone: 'error' }
    }

    if (occurrence.status === 'running') {
        return { label: t('schedule.waitingClip'), tone: 'accent' }
    }

    if (date.value < today.value || (isToday.value && start < nowSeconds.value - 60)) {
        return { label: t('schedule.notAired'), tone: 'muted' }
    }

    if (isToday.value && start - nowSeconds.value < 3600) {
        return {
            label: t('schedule.waitingIn', { minutes: Math.max(0, Math.ceil((start - nowSeconds.value) / 60)) }),
            tone: 'waiting',
        }
    }

    return { label: t('schedule.scheduled'), tone: 'neutral' }
}

function stateClass(tone: string): string {
    return (
        {
            accent: 'bg-accent text-accent-content font-semibold',
            waiting: 'bg-accent/15 text-accent',
            error: 'bg-error/15 text-error',
            muted: 'bg-base-200 text-base-content/60',
            neutral: 'bg-base-200 text-base-content/80',
        }[tone] ?? ''
    )
}

function sourcesLabel(event: ScheduleEvent): string {
    if (event.kind === 'live') {
        const live = scheduleStore.lives.find((item) => item.path === event.sources[0])

        return live ? `${live.protocol} · ${live.url}` : (event.sources[0] ?? '')
    }

    return [
        t('schedule.files', { count: event.sources.length }, event.sources.length),
        formatLength(event.duration),
        repeatLabel(event.repeat_days),
    ].join(' · ')
}

async function refresh() {
    try {
        await Promise.all([scheduleStore.loadDay(date.value), scheduleStore.loadStatus()])
    } catch (err) {
        indexStore.msgAlert('error', err instanceof Error ? err.message : String(err), 4)
    }
}

function scrollToFocus() {
    nextTick(() => {
        if (!scroller.value) {
            return
        }

        const focus = isToday.value ? nowSeconds.value : (gradeBlocks.value[0]?.start ?? 6 * 3600)
        scroller.value.scrollLeft = Math.max(0, (focus - zoom.value * 60 * 0.25) * pxPerSecond.value)
    })
}

function changeDay(days: number) {
    date.value = days === 0 ? today.value : addDays(date.value, days)
}

function laneSeconds(event: DragEvent): number {
    const lane = event.currentTarget as HTMLElement
    const offset = event.clientX - lane.getBoundingClientRect().left
    const step = zoom.value <= 60 ? 10 : 60

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
        label: t('schedule.dropAt', { time: secondsToTime(seconds).slice(0, zoom.value <= 60 ? 8 : 5) }),
    }
}

function dragLeave(event: DragEvent) {
    const lane = event.currentTarget as HTMLElement

    if (!lane.contains(event.relatedTarget as Node)) {
        ghost.value = null
    }
}

async function drop(event: DragEvent) {
    const payload = readPayload(event)
    const seconds = laneSeconds(event)
    ghost.value = null

    if (!payload) {
        return
    }

    event.preventDefault()

    if (payload.type === 'media') {
        mediaDraft.value = {
            sources: [{ path: payload.path, name: payload.name, duration: payload.duration }],
            date: date.value,
            time: secondsToTime(seconds),
        }
    } else if (payload.type === 'live') {
        liveDraft.value = {
            live: scheduleStore.lives.find((live) => live.path === payload.path),
            date: date.value,
            start: secondsToTime(seconds),
            end: secondsToTime(seconds + 3600),
        }
    } else {
        await moveEvent(payload.id, seconds)
    }
}

function dragLive(event: DragEvent, live: LiveSource) {
    const payload: ScheduleDragItem = { type: 'live', path: live.path, name: live.name, protocol: live.protocol }

    event.dataTransfer?.setData(DRAG_TYPE, JSON.stringify(payload))
    event.dataTransfer?.setData('text/plain', live.path)

    if (event.dataTransfer) {
        event.dataTransfer.effectAllowed = 'copy'
    }
}

function dragEvent(event: DragEvent, occurrence: ScheduleOccurrence) {
    const payload: ScheduleDragItem = { type: 'event', id: occurrence.event.id }

    event.dataTransfer?.setData(DRAG_TYPE, JSON.stringify(payload))

    if (event.dataTransfer) {
        event.dataTransfer.effectAllowed = 'move'
    }
}

function inputOf(event: ScheduleEvent): ScheduleEventInput {
    return {
        kind: event.kind,
        title: event.title,
        sources: [...event.sources],
        start_date: event.start_date,
        start_time: event.start_time,
        end_time: event.end_time,
        repeat_days: event.repeat_days,
        entry_mode: event.entry_mode,
        after_mode: event.after_mode,
        show_in_guide: event.show_in_guide,
        retry_seconds: event.retry_seconds,
        enabled: event.enabled,
    }
}

/** Move an event to `seconds` on the shown day, keeping a live's length. */
async function moveEvent(eventId: number, seconds: number) {
    const event = scheduleStore.occurrences.find((occurrence) => occurrence.event.id === eventId)?.event

    if (!event) {
        return
    }

    const input = inputOf(event)
    const oldStart = secondsFromDay(`${event.start_date}T${event.start_time}`, event.start_date)

    input.start_time = secondsToTime(seconds)

    if (!event.repeat_days) {
        input.start_date = date.value
    }

    if (event.kind === 'live' && event.end_time) {
        let length = secondsFromDay(`${event.start_date}T${event.end_time}`, event.start_date) - oldStart

        if (length <= 0) {
            length += DAY
        }

        input.end_time = secondsToTime(seconds + length)
    }

    try {
        await scheduleStore.saveEvent(input, event.id)
        indexStore.msgAlert('success', t('schedule.moved', { time: input.start_time }), 2)
        await refresh()
    } catch (err) {
        indexStore.msgAlert('error', err instanceof Error ? err.message : String(err), 4)
    }
}

function edit(occurrence: ScheduleOccurrence) {
    const event = occurrence.event

    if (event.kind === 'live') {
        const path = event.sources[0] ?? ''

        liveDraft.value = {
            event,
            live: scheduleStore.lives.find((live) => live.path === path) ?? {
                path,
                name: parentFolder(path) || fileStem(path),
                url: '',
                protocol: '',
                title: null,
            },
            date: event.start_date,
            start: event.start_time,
            end: event.end_time ?? event.start_time,
        }
    } else {
        mediaDraft.value = {
            event,
            sources: event.sources.map((path) => ({ path, name: fileName(path), duration: 0 })),
            date: event.start_date,
            time: event.start_time,
        }
    }
}

async function trigger(occurrence: ScheduleOccurrence) {
    try {
        await scheduleStore.trigger(occurrence.event.id)
        indexStore.msgAlert('success', t('schedule.triggered', { title: occurrence.event.title }), 3)
        await refresh()
    } catch (err) {
        indexStore.msgAlert('error', err instanceof Error ? err.message : String(err), 4)
    }
}

async function confirmDelete(confirmed: boolean) {
    const event = deleting.value
    deleting.value = null

    if (!confirmed || !event) {
        return
    }

    try {
        await scheduleStore.deleteEvent(event.id)
        await refresh()
    } catch (err) {
        indexStore.msgAlert('error', err instanceof Error ? err.message : String(err), 4)
    }
}

function newMaterial() {
    mediaDraft.value = {
        sources: [],
        date: date.value,
        time: secondsToTime(isToday.value ? nowSeconds.value + 300 : 12 * 3600).slice(0, 5) + ':00',
    }
}

function newLive() {
    const start = isToday.value ? Math.ceil((nowSeconds.value + 600) / 900) * 900 : 12 * 3600

    liveDraft.value = { date: date.value, start: secondsToTime(start), end: secondsToTime(start + 3600) }
}

async function saved() {
    mediaDraft.value = null
    liveDraft.value = null
    await refresh()
}

const nextUp = computed(() =>
    scheduleStore.occurrences.filter(
        (occurrence) =>
            !occurrence.status && isToday.value && secondsFromDay(occurrence.start, date.value) >= nowSeconds.value,
    ),
)

onMounted(async () => {
    await Promise.all([refresh(), scheduleStore.loadLives().catch(() => undefined)])
    scrollToFocus()
    refreshTimer = setInterval(refresh, 5000)
    clockTimer = setInterval(() => (nowSeconds.value = secondsOfDay()), 1000)
})

onBeforeUnmount(() => {
    clearInterval(refreshTimer)
    clearInterval(clockTimer)
})

watch([date, i], async () => {
    await refresh()
    scrollToFocus()
})

watch(i, () => scheduleStore.loadLives().catch(() => undefined))
watch(zoom, scrollToFocus)
</script>

<template>
    <div class="min-h-[calc(100vh-56px)] flex flex-col">
        <div class="flex flex-wrap items-center gap-2.5 px-4 py-3 border-b border-base-300">
            <div class="join border border-base-content/30 rounded-md">
                <button
                    class="btn btn-sm btn-ghost join-item"
                    :aria-label="t('schedule.previousDay')"
                    @click="changeDay(-1)"
                >
                    <i class="bi bi-chevron-left" />
                </button>
                <label class="join-item flex items-center gap-2 px-2 text-sm cursor-pointer">
                    <i class="bi bi-calendar3" />
                    <span class="capitalize">{{ dateLabel }}</span>
                    <input v-model="date" type="date" class="sr-only" :aria-label="t('schedule.day')" />
                </label>
                <button
                    class="btn btn-sm btn-ghost join-item"
                    :aria-label="t('schedule.nextDay')"
                    @click="changeDay(1)"
                >
                    <i class="bi bi-chevron-right" />
                </button>
            </div>
            <button class="btn btn-sm btn-outline border-base-content/30" :disabled="isToday" @click="changeDay(0)">
                {{ t('schedule.today') }}
            </button>
            <div class="join" role="group" :aria-label="t('schedule.zoom')">
                <button
                    v-for="option in zoomOptions"
                    :key="option.minutes"
                    class="btn btn-sm join-item"
                    :class="
                        zoom === option.minutes
                            ? 'bg-base-content/25 border-base-content/30'
                            : 'btn-outline border-base-content/30'
                    "
                    :aria-pressed="zoom === option.minutes"
                    @click="zoom = option.minutes"
                >
                    {{ option.label }}
                </button>
            </div>
            <div class="flex-1" />
            <button class="btn btn-sm btn-outline border-base-content/30" @click="newLive">
                <i class="bi bi-broadcast" /> {{ t('schedule.newLive') }}
            </button>
            <button class="btn btn-sm btn-accent font-bold" @click="newMaterial">
                <i class="bi bi-plus-lg" /> {{ t('schedule.scheduleMaterial') }}
            </button>
        </div>

        <div class="flex flex-wrap gap-4 p-4 items-start flex-1">
            <aside
                class="flex-[1_1_260px] max-w-80 bg-base-100 border border-base-300 rounded-lg flex flex-col max-h-[calc(100vh-150px)]"
            >
                <div role="tablist" class="flex border-b border-base-300">
                    <button
                        role="tab"
                        :aria-selected="tab === 'storage'"
                        class="flex-1 min-h-11 border-b-2 cursor-pointer"
                        :class="tab === 'storage' ? 'border-accent' : 'border-transparent text-base-content/60'"
                        @click="tab = 'storage'"
                    >
                        {{ t('button.media') }}
                    </button>
                    <button
                        role="tab"
                        :aria-selected="tab === 'lives'"
                        class="flex-1 min-h-11 border-b-2 cursor-pointer"
                        :class="tab === 'lives' ? 'border-accent' : 'border-transparent text-base-content/60'"
                        @click="tab = 'lives'"
                    >
                        {{ t('schedule.lives') }}
                    </button>
                </div>
                <div class="p-3 flex flex-col gap-2.5 min-h-0 overflow-auto">
                    <StoragePicker v-if="tab === 'storage'" draggable />
                    <template v-else>
                        <ul class="flex flex-col gap-0.5">
                            <li
                                v-for="live in scheduleStore.lives"
                                :key="live.path"
                                class="flex items-center gap-2 px-2 py-2 rounded-md hover:bg-base-200 cursor-grab"
                                draggable="true"
                                @dragstart="dragLive($event, live)"
                            >
                                <i
                                    class="bi bi-broadcast"
                                    :style="{ color: SCHEDULE_COLORS.live }"
                                    aria-hidden="true"
                                />
                                <span class="flex-1 min-w-0 truncate" :title="live.url">{{
                                    live.title || live.name
                                }}</span>
                                <span
                                    class="badge badge-sm badge-outline"
                                    :style="{ borderColor: SCHEDULE_COLORS.live }"
                                >
                                    {{ live.protocol }}
                                </span>
                            </li>
                            <li v-if="!scheduleStore.lives.length" class="text-sm text-base-content/60 px-2 py-3">
                                {{ t('schedule.noLives') }}
                            </li>
                        </ul>
                        <button class="btn btn-sm btn-outline border-base-content/30" @click="newLive">
                            <i class="bi bi-plus-lg" /> {{ t('schedule.newLive') }}
                        </button>
                    </template>
                    <p class="text-xs text-base-content/60 leading-relaxed">{{ t('schedule.dragHint') }}</p>
                </div>
            </aside>

            <main class="flex-[999_1_560px] min-w-0 flex flex-col gap-4">
                <section :aria-label="t('schedule.timeline')" class="bg-base-100 border border-base-300 rounded-lg p-4">
                    <div class="flex flex-wrap gap-4 text-xs text-base-content/80 mb-3">
                        <span class="flex items-center gap-1.5"
                            ><span class="w-3 h-3 rounded-sm bg-[#4a4a4a]" />{{ t('schedule.legendGrade') }}</span
                        >
                        <span class="flex items-center gap-1.5"
                            ><span class="w-3 h-3 rounded-sm" :style="{ background: SCHEDULE_COLORS.after_clip }" />{{
                                t('schedule.afterClip')
                            }}</span
                        >
                        <span class="flex items-center gap-1.5"
                            ><span class="w-3 h-3 rounded-sm" :style="{ background: SCHEDULE_COLORS.interrupt }" />{{
                                t('schedule.interrupt')
                            }}</span
                        >
                        <span class="flex items-center gap-1.5"
                            ><span class="w-3 h-3 rounded-sm" :style="{ background: SCHEDULE_COLORS.live }" />{{
                                t('schedule.live')
                            }}</span
                        >
                    </div>

                    <div ref="scroller" class="overflow-x-auto overflow-y-hidden pb-2">
                        <div class="grid gap-y-2.5" :style="{ gridTemplateColumns: `${LABEL_WIDTH}px ${laneWidth}px` }">
                            <div class="sticky left-0 z-20 bg-base-100" />
                            <div class="relative h-5 text-xs text-base-content/60 tabular-nums">
                                <span
                                    v-for="tick in ticks"
                                    :key="tick.second"
                                    class="absolute -translate-x-1/2 first:translate-x-0"
                                    :style="{ left: left(tick.second) }"
                                >
                                    {{ tick.label }}
                                </span>
                            </div>

                            <div class="sticky left-0 z-20 bg-base-100 text-xs text-base-content/80 flex items-center">
                                {{ t('schedule.grade') }}
                            </div>
                            <div
                                class="relative h-16 bg-base-200 rounded-md"
                                @dragover="dragOver($event, 'grade')"
                                @dragleave="dragLeave"
                                @drop="drop"
                            >
                                <div
                                    v-for="(block, index) in gradeBlocks"
                                    :key="`g${index}`"
                                    class="absolute top-1.5 bottom-1.5 rounded px-2.5 py-1.5 overflow-hidden border-r-2 border-base-200"
                                    :class="block.scheduled ? 'text-[#1c1c1c]' : 'bg-[#4a4a4a] text-white'"
                                    :style="{
                                        left: left(block.start),
                                        width: width(block.start, block.end, 2),
                                        background: block.scheduled ? SCHEDULE_COLORS.live : undefined,
                                    }"
                                    :title="`${block.label}${block.detail ? ` — ${block.detail}` : ''} · ${secondsToTime(block.start)} – ${secondsToTime(block.end)}`"
                                >
                                    <div class="text-sm font-semibold truncate">{{ block.label }}</div>
                                    <div class="text-xs opacity-75 truncate tabular-nums">
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
                                        class="absolute -top-8 -translate-x-1/2 bg-accent text-accent-content text-[11px] font-bold px-1.5 rounded z-10 whitespace-nowrap pointer-events-none"
                                        :style="{ left: left(nowSeconds) }"
                                    >
                                        {{ t('schedule.now') }} {{ secondsToTime(nowSeconds).slice(0, 5) }}
                                    </div>
                                </template>
                            </div>

                            <div class="sticky left-0 z-20 bg-base-100 text-xs text-base-content/80 flex items-center">
                                {{ t('schedule.events') }}
                            </div>
                            <div
                                class="relative h-16 bg-base-200 rounded-md"
                                @dragover="dragOver($event, 'events')"
                                @dragleave="dragLeave"
                                @drop="drop"
                            >
                                <button
                                    v-for="block in eventBlocks"
                                    :key="`${block.occurrence.event.id}-${block.occurrence.start}`"
                                    class="absolute top-1.5 bottom-1.5 rounded px-2.5 py-1 overflow-hidden text-left cursor-grab min-w-30"
                                    :class="{
                                        'opacity-50': block.occurrence.status === 'done',
                                        'ring-2 ring-error': block.occurrence.status === 'failed',
                                        'ring-2 ring-white animate-pulse':
                                            scheduleStore.status.active?.event_id === block.occurrence.event.id,
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
                                    @click="edit(block.occurrence)"
                                >
                                    <div class="text-[13px] font-bold truncate flex items-center gap-1.5">
                                        <span
                                            v-if="block.occurrence.event.kind === 'live'"
                                            class="bg-[#1c1c1c] text-[#ffb1a3] text-[10px] px-1 rounded"
                                        >
                                            {{ t('schedule.liveBadge') }}
                                        </span>
                                        {{ block.occurrence.event.title }}
                                    </div>
                                    <div class="text-xs opacity-80 tabular-nums truncate">
                                        {{ secondsToTime(block.start).slice(0, 5) }}
                                        <template v-if="block.occurrence.event.kind === 'live'">
                                            – {{ secondsToTime(block.end).slice(0, 5) }}</template
                                        >
                                        <template v-else>
                                            · {{ formatLength(block.occurrence.event.duration) }}</template
                                        >
                                    </div>
                                </button>
                                <div
                                    v-if="ghost"
                                    class="absolute top-1.5 bottom-1.5 w-32 rounded border-2 border-dashed border-accent bg-base-300 text-accent px-2 py-1 text-[13px] font-semibold whitespace-nowrap pointer-events-none z-20"
                                    :style="{ left: left(ghost.seconds) }"
                                >
                                    {{ ghost.label }}
                                </div>
                            </div>
                        </div>
                    </div>
                </section>

                <section :aria-label="t('schedule.todayEvents')" class="bg-base-100 border border-base-300 rounded-lg">
                    <div class="flex items-center justify-between px-4 py-3 border-b border-base-300">
                        <h2 class="text-[15px] font-semibold">
                            {{ isToday ? t('schedule.todayEvents') : t('schedule.dayEvents') }}
                        </h2>
                        <span class="text-xs text-base-content/60">
                            {{
                                t(
                                    'schedule.countScheduled',
                                    { count: scheduleStore.occurrences.length },
                                    scheduleStore.occurrences.length,
                                )
                            }}
                            <template v-if="nextUp[0]">
                                ·
                                {{
                                    t('schedule.nextIn', {
                                        minutes: Math.max(
                                            0,
                                            Math.ceil((secondsFromDay(nextUp[0].start, date) - nowSeconds) / 60),
                                        ),
                                    })
                                }}
                            </template>
                        </span>
                    </div>
                    <div class="overflow-x-auto">
                        <table class="w-full min-w-215 text-[13px]">
                            <thead>
                                <tr class="text-left text-base-content/60">
                                    <th class="px-4 py-2.5 font-medium">{{ t('schedule.time') }}</th>
                                    <th class="px-2 py-2.5 font-medium">{{ t('schedule.material') }}</th>
                                    <th class="px-2 py-2.5 font-medium">{{ t('schedule.howEnters') }}</th>
                                    <th class="px-2 py-2.5 font-medium">{{ t('schedule.afterwards') }}</th>
                                    <th class="px-2 py-2.5 font-medium">{{ t('schedule.inGuide') }}</th>
                                    <th class="px-2 py-2.5 font-medium">{{ t('schedule.state') }}</th>
                                    <th class="px-4 py-2.5 font-medium text-right">{{ t('schedule.actions') }}</th>
                                </tr>
                            </thead>
                            <tbody>
                                <tr
                                    v-for="block in eventBlocks"
                                    :key="`row-${block.occurrence.event.id}-${block.occurrence.start}`"
                                    class="border-t border-base-200"
                                >
                                    <td class="px-4 py-3 tabular-nums align-top">
                                        <template v-if="block.occurrence.event.kind === 'live'">
                                            {{ splitDateTime(block.occurrence.start).time.slice(0, 5) }} –
                                            {{ splitDateTime(block.occurrence.end).time.slice(0, 5) }}
                                        </template>
                                        <template v-else>{{ splitDateTime(block.occurrence.start).time }}</template>
                                        <div
                                            v-if="block.start !== block.nominal"
                                            class="text-xs"
                                            :style="{ color: '#9cc0e6' }"
                                        >
                                            {{ t('schedule.expected', { time: secondsToTime(block.start) }) }}
                                        </div>
                                    </td>
                                    <td class="px-2 py-3 align-top">
                                        {{ block.occurrence.event.kind === 'live' ? `${t('schedule.live')} · ` : ''
                                        }}{{ block.occurrence.event.title }}
                                        <div class="text-xs text-base-content/60 truncate max-w-72">
                                            {{ sourcesLabel(block.occurrence.event) }}
                                        </div>
                                    </td>
                                    <td class="px-2 py-3 align-top">
                                        <span class="inline-flex items-center gap-1.5">
                                            <span
                                                class="w-2 h-2 rounded-sm"
                                                :style="{ background: block.color }"
                                                aria-hidden="true"
                                            />
                                            {{ entryLabel(block.occurrence.event) }}
                                        </span>
                                    </td>
                                    <td class="px-2 py-3 align-top">{{ afterLabel(block.occurrence.event) }}</td>
                                    <td
                                        class="px-2 py-3 align-top"
                                        :class="{ 'text-base-content/60': !block.occurrence.event.show_in_guide }"
                                    >
                                        {{
                                            block.occurrence.event.show_in_guide ? t('schedule.yes') : t('schedule.no')
                                        }}
                                    </td>
                                    <td class="px-2 py-3 align-top">
                                        <span
                                            class="rounded px-2 py-0.5 text-xs whitespace-nowrap"
                                            :class="stateClass(occurrenceState(block.occurrence).tone)"
                                        >
                                            {{ occurrenceState(block.occurrence).label }}
                                        </span>
                                    </td>
                                    <td class="px-4 py-2 text-right whitespace-nowrap align-top">
                                        <button
                                            class="btn btn-sm btn-square btn-outline border-base-content/30"
                                            :aria-label="t('schedule.playNow')"
                                            :title="t('schedule.playNow')"
                                            @click="trigger(block.occurrence)"
                                        >
                                            <i class="bi bi-lightning-charge" />
                                        </button>
                                        <button
                                            class="btn btn-sm btn-square btn-outline border-base-content/30 ms-1"
                                            :aria-label="t('schedule.edit')"
                                            :title="t('schedule.edit')"
                                            @click="edit(block.occurrence)"
                                        >
                                            <i class="bi bi-pencil" />
                                        </button>
                                        <button
                                            class="btn btn-sm btn-square btn-outline border-base-content/30 ms-1"
                                            :aria-label="t('schedule.remove')"
                                            :title="t('schedule.remove')"
                                            @click="deleting = block.occurrence.event"
                                        >
                                            <i class="bi bi-trash" />
                                        </button>
                                    </td>
                                </tr>
                                <tr v-if="!eventBlocks.length">
                                    <td colspan="7" class="px-4 py-6 text-center text-base-content/60">
                                        {{ t('schedule.empty') }}
                                    </td>
                                </tr>
                            </tbody>
                        </table>
                    </div>
                </section>
            </main>
        </div>

        <ScheduleDialog
            :draft="mediaDraft"
            :program="scheduleStore.program"
            :grade="scheduleStore.grade"
            @close="mediaDraft = null"
            @saved="saved"
        />
        <LiveDialog :draft="liveDraft" @close="liveDraft = null" @saved="saved" />
        <GenericModal
            :show="Boolean(deleting)"
            :title="t('schedule.removeTitle')"
            :text="t('schedule.removeText', { title: deleting?.title ?? '' })"
            :modal-action="confirmDelete"
        />
    </div>
</template>
