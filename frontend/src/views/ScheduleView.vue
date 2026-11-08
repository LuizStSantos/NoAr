<script setup lang="ts">
import dayjs from 'dayjs'
import timezone from 'dayjs/plugin/timezone.js'
import utc from 'dayjs/plugin/utc.js'
import 'dayjs/locale/de'
import 'dayjs/locale/pt-br'
import 'dayjs/locale/ru'

import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { storeToRefs } from 'pinia'
import { useHead } from '@unhead/vue'
import { useRoute, useRouter } from 'vue-router'

import GenericModal from '@/components/utils/GenericModal.vue'
import LiveRemote from '@/components/settings/LiveRemote.vue'
import GraphicsEventDialog from '@/components/schedule/GraphicsEventDialog.vue'
import LiveDialog from '@/components/schedule/LiveDialog.vue'
import ScheduleDialog from '@/components/schedule/ScheduleDialog.vue'
import StoragePicker from '@/components/schedule/StoragePicker.vue'
import ScheduleTimeline from '@/components/schedule/ScheduleTimeline.vue'
import ScheduleEventList from '@/components/schedule/ScheduleEventList.vue'
import ScheduleGraphicsList from '@/components/schedule/ScheduleGraphicsList.vue'
import {
    SCHEDULE_DAY_SECONDS as DAY,
    SCHEDULE_DRAG_TYPE as DRAG_TYPE,
    SCHEDULE_ZOOM_OPTIONS as zoomOptions,
    type ScheduleGradeBlock,
} from '@/composables/schedulePresentation'
import {
    SCHEDULE_COLORS,
    addDays,
    eventColor,
    fileName,
    fileStem,
    parentFolder,
    secondsFromDay,
    secondsToTime,
    timeToSeconds,
} from '@/composables/schedule'
import { useConfig } from '@/stores/config'
import { useIndex } from '@/stores/index'
import { useSchedule } from '@/stores/schedule'

dayjs.extend(utc)
dayjs.extend(timezone)

const { t, locale } = useI18n()
const configStore = useConfig()
const indexStore = useIndex()
const scheduleStore = useSchedule()
const route = useRoute()
const router = useRouter()
const { i } = storeToRefs(configStore)

useHead({ title: computed(() => t('button.schedule')) })

const tab = ref<'storage' | 'lives'>('storage')
const zoom = ref(360)
const date = ref(channelNow().format('YYYY-MM-DD'))
const nowSeconds = ref(secondsOfDay())
const timeline = ref<InstanceType<typeof ScheduleTimeline>>()
const mediaDraft = ref<MediaDraft | null>(null)
const liveDraft = ref<LiveDraft | null>(null)
const deleting = ref<ScheduleEvent | null>(null)
const graphicsDraft = ref<{ event: GraphicsEvent | null; date: string; time: string } | null>(null)
const deletingGraphics = ref<GraphicsEvent | null>(null)

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
/** The day's programme from the guide: program names and times moved by events. */
const gradeBlocks = computed<ScheduleGradeBlock[]>(() =>
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

async function refresh() {
    try {
        await Promise.all([scheduleStore.loadDay(date.value), scheduleStore.loadStatus(), scheduleStore.loadGraphics()])
    } catch (err) {
        indexStore.msgAlert('error', err instanceof Error ? err.message : String(err), 4)
    }
}

function scrollToFocus() {
    nextTick(() => {
        const focus = isToday.value ? nowSeconds.value : (gradeBlocks.value[0]?.start ?? 6 * 3600)
        timeline.value?.scrollToFocus(focus)
    })
}

function changeDay(days: number) {
    date.value = days === 0 ? today.value : addDays(date.value, days)
}

async function drop(payload: ScheduleDragItem, seconds: number) {
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

/** Graphics events that happen on the day shown, as spans of the timeline. */
const graphicsBlocks = computed(() =>
    scheduleStore.graphics
        .filter((event) => {
            if (!event.enabled) {
                return false
            }

            if (!event.repeat_days) {
                return event.start_date === date.value
            }

            return date.value >= event.start_date && Boolean(event.repeat_days & (1 << dayjs(date.value).day()))
        })
        .map((event) => {
            const start = timeToSeconds(event.start_time)
            let end = event.end_time ? timeToSeconds(event.end_time) : start + 600

            if (end <= start) {
                end = DAY
            }

            return { event, start, end, open: !event.end_time }
        })
        .sort((a, b) => a.start - b.start),
)

function newGraphics() {
    graphicsDraft.value = {
        event: null,
        date: date.value,
        time: secondsToTime(isToday.value ? Math.ceil((nowSeconds.value + 300) / 300) * 300 : 12 * 3600),
    }
}

function editGraphics(event: GraphicsEvent) {
    graphicsDraft.value = { event, date: event.start_date, time: event.start_time }
}

async function graphicsSaved() {
    graphicsDraft.value = null
    await refresh()
}

async function confirmDeleteGraphics(confirmed: boolean) {
    const event = deletingGraphics.value
    deletingGraphics.value = null

    if (!confirmed || !event) {
        return
    }

    try {
        await scheduleStore.deleteGraphics(event.id)
        indexStore.msgAlert('success', t('schedule.graphics.removed'), 2)
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

/** The material dialog hands over to a live: the one in `folder`, or a new one. */
function liveFromMaterial(folder: string | null, day: string, time: string) {
    const start = timeToSeconds(time || '12:00:00')

    mediaDraft.value = null
    liveDraft.value = {
        live: folder
            ? scheduleStore.lives.find((live) => live.path.split('/').slice(0, -1).join('/') === folder)
            : undefined,
        date: day,
        start: secondsToTime(start),
        end: secondsToTime(start + 3600),
    }
}

async function saved() {
    mediaDraft.value = null
    liveDraft.value = null
    await refresh()
}

/** A link from Storage (`?live=<path>`) opens the live, ready to schedule. */
function openLinkedLive() {
    const path = route.query.live

    if (typeof path !== 'string') {
        return
    }

    const live = scheduleStore.lives.find((item) => item.path === path)

    router.replace({ query: { ...route.query, live: undefined } })

    if (live) {
        newLive()

        if (liveDraft.value) {
            liveDraft.value = { ...liveDraft.value, live }
        }
    }
}

onMounted(async () => {
    await Promise.all([refresh(), scheduleStore.loadLives().catch(() => undefined)])
    openLinkedLive()
    scrollToFocus()
    if (route.hash === '#live-api') {
        // From the Player's live controls: straight to the API tutorial.
        await nextTick()
        document.getElementById('live-api')?.scrollIntoView({ block: 'start' })
    }
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
    <div class="max-w-[1440px] mx-auto px-4 sm:px-6 pt-5 pb-8 flex flex-col gap-4">
        <div class="flex flex-wrap items-center gap-x-3 gap-y-2.5">
            <div class="me-2">
                <h1 class="m-0 text-xl font-semibold">{{ t('button.schedule') }}</h1>
                <p class="m-0 text-muted">{{ t('schedule.intro') }}</p>
            </div>
            <div class="flex items-center bg-base-100 border border-line-strong rounded-[10px]">
                <button
                    class="btn-icon min-w-10 min-h-10 rounded-e-none rounded-s-[10px]"
                    :aria-label="t('schedule.previousDay')"
                    @click="changeDay(-1)"
                >
                    <i class="bi bi-chevron-left" aria-hidden="true" />
                </button>
                <label
                    class="flex items-center gap-2 px-3 min-h-10 border-x border-line-strong text-[15px] cursor-pointer"
                >
                    <i class="bi bi-calendar3 text-muted" aria-hidden="true" />
                    <span class="capitalize">{{ dateLabel }}</span>
                    <input v-model="date" type="date" class="sr-only" :aria-label="t('schedule.day')" />
                </label>
                <button
                    class="btn-icon min-w-10 min-h-10 rounded-s-none rounded-e-[10px]"
                    :aria-label="t('schedule.nextDay')"
                    @click="changeDay(1)"
                >
                    <i class="bi bi-chevron-right" aria-hidden="true" />
                </button>
            </div>
            <button class="btn-quiet rounded-[10px]" :disabled="isToday" @click="changeDay(0)">
                {{ t('schedule.today') }}
            </button>
            <div
                class="flex bg-base-100 border border-line-strong rounded-[10px] p-[3px]"
                role="group"
                :aria-label="t('schedule.zoom')"
            >
                <button
                    v-for="option in zoomOptions"
                    :key="option.minutes"
                    class="border-0 rounded-lg min-w-13 min-h-8.5 cursor-pointer"
                    :class="
                        zoom === option.minutes
                            ? 'bg-base-300 text-base-content font-semibold'
                            : 'bg-transparent text-muted hover:text-base-content'
                    "
                    :aria-pressed="zoom === option.minutes"
                    @click="zoom = option.minutes"
                >
                    {{ option.label }}
                </button>
            </div>
            <div class="flex-auto" />
            <button class="btn-quiet rounded-[10px]" @click="newLive">
                <i class="bi bi-broadcast" aria-hidden="true" />{{ t('schedule.newLive') }}
            </button>
            <button class="btn-quiet rounded-[10px]" @click="newGraphics">
                <i class="bi bi-layers" aria-hidden="true" />{{ t('schedule.graphics.new') }}
            </button>
            <button class="btn-main rounded-[10px] px-4.5" @click="newMaterial">
                <i class="bi bi-plus-lg" aria-hidden="true" />{{ t('schedule.scheduleMaterial') }}
            </button>
        </div>

        <div class="flex flex-wrap gap-4 items-start">
            <aside
                :aria-label="t('schedule.materials')"
                class="card-box flex-[1_1_280px] w-full lg:max-w-[340px] flex flex-col lg:sticky lg:top-[72px] max-h-[70vh] lg:max-h-[calc(100vh-96px)]"
            >
                <div role="tablist" class="grid grid-cols-2 border-b border-line">
                    <button
                        role="tab"
                        :aria-selected="tab === 'storage'"
                        class="min-h-11.5 border-0 border-b-2 bg-transparent cursor-pointer"
                        :class="
                            tab === 'storage'
                                ? 'border-accent text-base-content font-semibold'
                                : 'border-transparent text-muted'
                        "
                        @click="tab = 'storage'"
                    >
                        {{ t('button.media') }}
                    </button>
                    <button
                        role="tab"
                        :aria-selected="tab === 'lives'"
                        class="min-h-11.5 border-0 border-b-2 bg-transparent cursor-pointer"
                        :class="
                            tab === 'lives'
                                ? 'border-accent text-base-content font-semibold'
                                : 'border-transparent text-muted'
                        "
                        @click="tab = 'lives'"
                    >
                        {{ t('schedule.lives') }}
                    </button>
                </div>
                <div class="p-3.5 flex flex-col gap-2.5 min-h-0 overflow-auto">
                    <StoragePicker v-if="tab === 'storage'" draggable hide-lives programs />
                    <template v-else>
                        <ul class="m-0 p-0 list-none flex flex-col gap-0.5">
                            <li
                                v-for="live in scheduleStore.lives"
                                :key="live.path"
                                class="flex items-center gap-2.5 px-2.5 min-h-10 rounded-lg hover:bg-raised cursor-grab"
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
                                    class="text-xs border rounded-md px-1.5 leading-5"
                                    :style="{ borderColor: SCHEDULE_COLORS.live }"
                                >
                                    {{ live.protocol }}
                                </span>
                            </li>
                            <li v-if="!scheduleStore.lives.length" class="text-muted px-2 py-3">
                                {{ t('schedule.noLives') }}
                            </li>
                        </ul>
                        <button class="btn-quiet" @click="newLive">
                            <i class="bi bi-plus-lg" aria-hidden="true" />{{ t('schedule.newLive') }}
                        </button>
                    </template>
                    <p class="m-0 text-[13px] text-subtle leading-relaxed">{{ t('schedule.dragHint') }}</p>
                </div>
            </aside>

            <main class="flex-[999_1_560px] min-w-0 flex flex-col gap-4">
                <ScheduleTimeline
                    ref="timeline"
                    :grade-blocks="gradeBlocks"
                    :event-blocks="eventBlocks"
                    :graphics-blocks="graphicsBlocks"
                    :zoom="zoom"
                    :is-today="isToday"
                    :now-seconds="nowSeconds"
                    :active-event-id="scheduleStore.status.active?.event_id"
                    @drop="drop"
                    @edit="edit"
                    @edit-graphics="editGraphics"
                />

                <ScheduleEventList
                    :blocks="eventBlocks"
                    :lives="scheduleStore.lives"
                    :date="date"
                    :today="today"
                    :now-seconds="nowSeconds"
                    :active-event-id="scheduleStore.status.active?.event_id"
                    @trigger="trigger"
                    @edit="edit"
                    @remove="deleting = $event"
                />

                <ScheduleGraphicsList
                    :blocks="graphicsBlocks"
                    @create="newGraphics"
                    @edit="editGraphics"
                    @remove="deletingGraphics = $event"
                />

                <div id="live-api" class="scroll-mt-20">
                    <LiveRemote :open="route.hash === '#live-api'" />
                </div>
            </main>
        </div>

        <GraphicsEventDialog :draft="graphicsDraft" @close="graphicsDraft = null" @saved="graphicsSaved" />
        <GenericModal
            :show="Boolean(deletingGraphics)"
            :title="t('schedule.graphics.remove')"
            :text="t('schedule.graphics.removeConfirm', { title: deletingGraphics?.title ?? '' })"
            :ok-label="t('schedule.remove')"
            danger
            :modal-action="confirmDeleteGraphics"
        />

        <ScheduleDialog
            :draft="mediaDraft"
            :program="scheduleStore.program"
            :grade="scheduleStore.grade"
            @close="mediaDraft = null"
            @saved="saved"
            @live="liveFromMaterial"
        />
        <LiveDialog :draft="liveDraft" @close="liveDraft = null" @saved="saved" />
        <GenericModal
            :show="Boolean(deleting)"
            :title="t('schedule.removeTitle')"
            :text="t('schedule.removeText', { title: deleting?.title ?? '' })"
            :ok-label="t('schedule.remove')"
            danger
            :modal-action="confirmDelete"
        />
    </div>
</template>
