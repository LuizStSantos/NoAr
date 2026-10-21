<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import StoragePicker from '@/components/schedule/StoragePicker.vue'
import {
    EVERY_DAY,
    MONDAY_TO_FRIDAY,
    SCHEDULE_COLORS,
    WEEKDAY_BITS,
    fileName,
    fileStem,
    formatLength,
    secondsFromDay,
    secondsToTime,
    splitDateTime,
    timeToSeconds,
    useScheduleText,
} from '@/composables/schedule'
import { commercialPaths, guideEntries, type GuideEntry } from '@/composables/storage'
import { useConfig } from '@/stores/config'
import { useIndex } from '@/stores/index'
import { useSchedule } from '@/stores/schedule'

const props = defineProps<{ draft: MediaDraft | null; program: ProgramItem[]; grade: GuideBlock[] }>()
const emit = defineEmits<{ close: []; saved: [] }>()

const { t } = useI18n()
const { weekdayLetters } = useScheduleText()
const configStore = useConfig()
const indexStore = useIndex()
const scheduleStore = useSchedule()

const sources = ref<{ path: string; name: string; duration: number }[]>([])
const title = ref('')
const date = ref('')
const time = ref('')
const repeatMode = ref<'once' | 'daily' | 'weekdays'>('once')
const repeatDays = ref(MONDAY_TO_FRIDAY)
const entryMode = ref<EntryMode>('after_clip')
const afterMode = ref<AfterMode>('continue')
const showInGuide = ref(false)
const showPicker = ref(false)
const saving = ref(false)
const error = ref('')

watch(
    () => props.draft,
    (draft) => {
        if (!draft) {
            return
        }

        const event = draft.event
        sources.value = [...draft.sources]
        title.value = event?.title ?? ''
        date.value = draft.date
        time.value = draft.time
        repeatDays.value = event?.repeat_days || MONDAY_TO_FRIDAY
        repeatMode.value = !event?.repeat_days ? 'once' : event.repeat_days === EVERY_DAY ? 'daily' : 'weekdays'
        entryMode.value = event?.entry_mode ?? 'after_clip'
        afterMode.value = event?.after_mode ?? 'continue'
        showInGuide.value = event?.show_in_guide ?? false
        showPicker.value = !draft.sources.length
        error.value = ''
    },
    { immediate: true },
)

/** Paths of the chosen material that lie in a commercials folder. */
const commercials = ref<Set<string>>(new Set())
const allCommercial = computed(
    () => sources.value.length > 0 && sources.value.every((source) => commercials.value.has(source.path)),
)

watch(
    () => sources.value.map((source) => source.path).join('\n'),
    async () => {
        const paths = sources.value.map((source) => source.path)
        const id = configStore.channels[configStore.i]?.id

        if (!id) {
            return
        }

        try {
            const flags = await commercialPaths(id, paths)

            commercials.value = new Set(paths.filter((_, index) => flags[index]))
        } catch {
            commercials.value = new Set()
        }
    },
    { immediate: true },
)

/** How each chosen clip shows up in the guide, from its sheets, by path. */
const guide = ref<Record<string, GuideEntry>>({})
/** The guide names the event after the first clip's sheet, not its own name. */
const sheetTitle = computed(() => (sources.value[0] ? guide.value[sources.value[0].path]?.title : '') || '')

watch(
    () => sources.value.map((source) => source.path).join('\n'),
    async () => {
        const paths = [...new Set(sources.value.map((source) => source.path))]
        const channel = configStore.channels[configStore.i]

        if (!channel?.id || !channel.storage || !paths.length) {
            guide.value = {}
            return
        }

        try {
            const found = await guideEntries(channel.id, channel.storage, paths)

            guide.value = Object.fromEntries(paths.map((path, index) => [path, found[index]]))
        } catch {
            guide.value = {}
        }
    },
    { immediate: true },
)

// Commercials never show in the guide.
watch(allCommercial, (all) => {
    if (all) {
        showInGuide.value = false
    }
})

const total = computed(() => {
    const known = sources.value.reduce((sum, source) => sum + (source.duration || 0), 0)

    return known || props.draft?.event?.duration || 0
})

const repeatBits = computed(() => {
    if (repeatMode.value === 'once') {
        return 0
    }

    return repeatMode.value === 'daily' ? EVERY_DAY : repeatDays.value
})

/** Program name at a time of the chosen day, from the guide. */
function programName(seconds: number, item: ProgramItem): string {
    const block = props.grade.find(
        (entry) =>
            secondsFromDay(entry.start, date.value) <= seconds && seconds < secondsFromDay(entry.stop, date.value),
    )

    return block?.title || item.title || fileStem(item.source)
}

/** The playlist clip on air at the chosen time, from the day's programme. */
const clipAtTime = computed(() => {
    const at = timeToSeconds(time.value || '00:00:00')

    for (const [index, item] of props.program.entries()) {
        const { date: itemDate, time: itemTime } = splitDateTime(item.start)

        if (itemDate !== date.value) {
            continue
        }

        const start = timeToSeconds(itemTime)
        const end = start + (item.out - item.in)

        if (start <= at && at < end) {
            return { item, start, end, next: props.program[index + 1] }
        }
    }

    return null
})

/**
 * An event listed in the guide (a live, for example) on air at the chosen
 * time. The player never cuts a running event, so a new one waits for its end.
 */
const eventOnAir = computed(() => {
    const at = timeToSeconds(time.value || '00:00:00')
    const ownStart = props.draft?.event ? `${date.value}T${props.draft.event.start_time}` : ''

    return (
        props.grade.find(
            (entry) =>
                entry.scheduled &&
                entry.start !== ownStart &&
                secondsFromDay(entry.start, date.value) <= at &&
                at < secondsFromDay(entry.stop, date.value),
        ) ?? null
    )
})

const entryPreview = computed(() => {
    const block = eventOnAir.value

    if (block) {
        return t('schedule.waitsFor', {
            time: secondsToTime(secondsFromDay(block.stop, date.value)),
            title: block.title,
        })
    }

    const clip = clipAtTime.value

    if (!clip) {
        return ''
    }

    const name = programName(clip.start, clip.item)

    return entryMode.value === 'after_clip'
        ? t('schedule.entersAt', { time: secondsToTime(clip.end), clip: name })
        : t('schedule.cutsAt', { time: secondsToTime(timeToSeconds(time.value)), clip: name })
})

/** Before/after bars: the clip on air, the event and what follows. */
const preview = computed(() => {
    const clip = clipAtTime.value

    if (!clip || !total.value || eventOnAir.value) {
        return null
    }

    const at = timeToSeconds(time.value)
    const start = entryMode.value === 'after_clip' ? clip.end : at
    const clipName = programName(clip.start, clip.item)
    const nextName = clip.next ? programName(clip.end, clip.next) : ''
    const nextLength = clip.next ? clip.next.out - clip.next.in : 0
    // Show at least half a minute of what airs before the event enters.
    const windowStart = Math.min(Math.max(clip.start, at - 300), start - 30)
    const leftOfClip = clip.end - at
    const event = (color: string) => ({ label: formatLength(total.value), seconds: total.value, color, event: true })
    const programme = (label: string, seconds: number, color = '#4a4a4a') => ({ label, seconds, color, event: false })
    const before = [programme(clipName, clip.end - windowStart), programme(nextName, nextLength, '#5a5a5a')]
    let after

    if (entryMode.value === 'after_clip') {
        after = [
            programme(clipName, clip.end - windowStart),
            event(SCHEDULE_COLORS.after_clip),
            programme(nextName, nextLength, '#5a5a5a'),
        ]
    } else if (afterMode.value === 'continue') {
        after = [
            programme(clipName, at - windowStart),
            event(SCHEDULE_COLORS.interrupt),
            programme(clipName, leftOfClip),
            programme(nextName, nextLength, '#5a5a5a'),
        ]
    } else {
        // Back on the clock: the event's length is skipped from what follows.
        after = [
            programme(clipName, at - windowStart),
            event(SCHEDULE_COLORS.interrupt),
            programme(clipName, Math.max(0, leftOfClip - total.value)),
            programme(nextName, nextLength - Math.max(0, total.value - leftOfClip), '#5a5a5a'),
        ]
    }

    const sum = (parts: { seconds: number }[]) => parts.reduce((all, part) => all + part.seconds, 0)
    const span = Math.max(sum(before), sum(after))
    // The event stays visible even when it is short next to the programme.
    const bars = (parts: { label: string; seconds: number; color: string; event: boolean }[]) =>
        parts
            .filter((part) => part.seconds > 0 && part.label)
            .map((part) => ({
                label: part.label,
                color: part.color,
                width: `${Math.max(part.event ? 6 : 0, (part.seconds / span) * 100)}%`,
            }))

    return {
        before: bars(before),
        after: bars(after),
        ticks: [windowStart, start, start + total.value].map(secondsToTime),
    }
})

function toggleDay(bit: number) {
    repeatDays.value = repeatDays.value & bit ? repeatDays.value & ~bit : repeatDays.value | bit
}

function addSource(source: { path: string; name: string; duration: number }) {
    sources.value.push(source)
}

function removeSource(index: number) {
    sources.value.splice(index, 1)
}

function moveSource(index: number, direction: -1 | 1) {
    const target = index + direction

    if (target < 0 || target >= sources.value.length) {
        return
    }

    const [moved] = sources.value.splice(index, 1)
    sources.value.splice(target, 0, moved!)
}

async function save() {
    error.value = ''

    if (!sources.value.length) {
        error.value = t('schedule.needMaterial')
        return
    }

    if (!date.value || !time.value) {
        error.value = t('schedule.needTime')
        return
    }

    if (repeatMode.value === 'weekdays' && !repeatDays.value) {
        error.value = t('schedule.needWeekday')
        return
    }

    saving.value = true

    try {
        await scheduleStore.saveEvent(
            {
                kind: 'media',
                title: title.value.trim(),
                sources: sources.value.map((source) => source.path),
                start_date: date.value,
                start_time: time.value.length === 5 ? `${time.value}:00` : time.value,
                end_time: null,
                repeat_days: repeatBits.value,
                entry_mode: entryMode.value,
                after_mode: afterMode.value,
                show_in_guide: showInGuide.value,
                retry_seconds: 120,
                enabled: true,
            },
            props.draft?.event?.id,
        )
        indexStore.msgAlert('success', t('schedule.saved'), 2)
        emit('saved')
    } catch (err) {
        error.value = err instanceof Error ? err.message : String(err)
    } finally {
        saving.value = false
    }
}
</script>

<template>
    <div v-if="draft" class="z-50 fixed inset-0 bg-black/50 overflow-auto flex justify-center items-start py-8 px-3">
        <div
            role="dialog"
            aria-modal="true"
            aria-labelledby="schedule-dialog-title"
            class="w-full max-w-170 bg-base-100 border border-base-content/20 rounded-lg shadow-xl flex flex-col"
        >
            <div class="flex items-center justify-between px-5 py-3 border-b border-base-300">
                <h2 id="schedule-dialog-title" class="text-lg font-semibold">
                    {{ draft.event ? t('schedule.editMaterial') : t('schedule.scheduleMaterial') }}
                </h2>
                <button class="btn-icon" :aria-label="t('schedule.close')" @click="emit('close')">
                    <i class="bi bi-x-lg" />
                </button>
            </div>

            <div class="px-5 py-4 flex flex-col gap-5">
                <div class="flex flex-col gap-2">
                    <div class="text-xs text-muted">{{ t('schedule.materialOrder') }}</div>
                    <ul class="flex flex-col gap-1.5">
                        <li
                            v-for="(source, index) in sources"
                            :key="`${source.path}-${index}`"
                            class="flex items-center gap-2 bg-base-200 rounded-md px-3 py-2"
                        >
                            <span class="text-base-content/60 w-4">{{ index + 1 }}</span>
                            <span class="flex-1 min-w-0 flex flex-col">
                                <span class="truncate" :title="source.path">{{
                                    source.name || fileName(source.path)
                                }}</span>
                                <span
                                    v-if="!commercials.has(source.path) && guide[source.path]?.title"
                                    class="truncate text-xs text-muted"
                                >
                                    {{ t('storage.guideColumn') }}: {{ guide[source.path]?.title }}
                                    <template v-if="guide[source.path]?.sub_title">
                                        · {{ guide[source.path]?.sub_title }}
                                    </template>
                                </span>
                            </span>
                            <span
                                v-if="commercials.has(source.path)"
                                class="text-[11px] text-after-clip border border-after-clip/50 rounded px-1.5 flex-none"
                                :title="t('onair.commercialHint')"
                            >
                                {{ t('onair.commercialTag') }}
                            </span>
                            <span class="text-base-content/60 tabular-nums">
                                {{ source.duration ? formatLength(source.duration) : '' }}
                            </span>
                            <button
                                class="btn-icon min-w-8 min-h-8"
                                :aria-label="t('schedule.moveUp')"
                                :disabled="index === 0"
                                @click="moveSource(index, -1)"
                            >
                                <i class="bi bi-arrow-up" />
                            </button>
                            <button
                                class="btn-icon min-w-8 min-h-8"
                                :aria-label="t('schedule.moveDown')"
                                :disabled="index === sources.length - 1"
                                @click="moveSource(index, 1)"
                            >
                                <i class="bi bi-arrow-down" />
                            </button>
                            <button
                                class="btn-icon min-w-8 min-h-8"
                                :aria-label="t('schedule.remove')"
                                @click="removeSource(index)"
                            >
                                <i class="bi bi-x-lg" />
                            </button>
                        </li>
                    </ul>
                    <div class="flex items-center justify-between">
                        <button
                            class="text-accent hover:underline cursor-pointer text-sm min-h-9"
                            @click="showPicker = !showPicker"
                        >
                            <i class="bi" :class="showPicker ? 'bi-chevron-up' : 'bi-plus-lg'" />
                            {{ t('schedule.addAnother') }}
                        </button>
                        <span v-if="total" class="text-sm text-base-content/70">
                            {{ t('schedule.total') }}
                            <strong class="tabular-nums text-base-content">{{ formatLength(total) }}</strong>
                        </span>
                    </div>
                    <div v-if="showPicker" class="border border-base-300 rounded-md p-2 max-h-64 flex flex-col">
                        <StoragePicker @pick="addSource" />
                    </div>
                </div>

                <label class="field-label">
                    {{ t('schedule.name') }}
                    <input
                        v-model="title"
                        class="field-input"
                        :placeholder="
                            sheetTitle || (sources[0] ? fileStem(sources[0].path) : t('schedule.namePlaceholder'))
                        "
                    />
                    <span v-if="sheetTitle && !allCommercial" class="field-hint">
                        {{ t('schedule.nameFromSheet', { title: sheetTitle }) }}
                    </span>
                </label>

                <div class="grid grid-cols-2 gap-3">
                    <label class="field-label">
                        {{ t('schedule.day') }}
                        <input v-model="date" type="date" class="field-input" required />
                    </label>
                    <label class="field-label">
                        {{ t('schedule.time') }}
                        <input v-model="time" type="time" step="1" class="field-input font-mono" required />
                    </label>
                </div>

                <div class="flex flex-col gap-2">
                    <div class="text-xs text-muted">{{ t('schedule.repeat') }}</div>
                    <div class="flex flex-wrap items-center gap-1.5">
                        <button
                            v-for="mode in ['once', 'daily', 'weekdays'] as const"
                            :key="mode"
                            class="rounded-field border min-h-9 px-3.5 cursor-pointer"
                            :class="
                                repeatMode === mode
                                    ? 'bg-accent text-accent-content border-accent font-semibold'
                                    : 'bg-transparent border-line-strong text-base-content'
                            "
                            :aria-pressed="repeatMode === mode"
                            @click="repeatMode = mode"
                        >
                            {{ t(`schedule.repeat_${mode}`) }}
                        </button>
                        <template v-if="repeatMode === 'weekdays'">
                            <span class="w-2" />
                            <button
                                v-for="(bit, index) in WEEKDAY_BITS"
                                :key="bit"
                                class="rounded-full border w-9 h-9 cursor-pointer"
                                :class="
                                    repeatDays & bit
                                        ? 'bg-accent text-accent-content border-accent font-semibold'
                                        : 'bg-transparent border-line-strong text-base-content'
                                "
                                :aria-pressed="Boolean(repeatDays & bit)"
                                :aria-label="t('schedule.weekdayShort').split(',')[index]"
                                @click="toggleDay(bit)"
                            >
                                {{ weekdayLetters()[index] }}
                            </button>
                        </template>
                    </div>
                </div>

                <fieldset class="flex flex-col gap-2">
                    <legend class="text-xs text-muted mb-2">{{ t('schedule.howEnters') }}</legend>
                    <label
                        class="flex gap-3 items-start rounded-lg px-4 py-3 cursor-pointer border"
                        :style="
                            entryMode === 'after_clip'
                                ? {
                                      borderColor: SCHEDULE_COLORS.after_clip,
                                      borderWidth: '2px',
                                      background: 'color-mix(in srgb, #436e9c 18%, transparent)',
                                  }
                                : {}
                        "
                        :class="entryMode === 'after_clip' ? '' : 'border-base-content/20'"
                    >
                        <input v-model="entryMode" type="radio" value="after_clip" class="radio radio-sm mt-0.5" />
                        <span class="flex flex-col gap-1">
                            <span class="font-semibold">{{ t('schedule.afterClip') }}</span>
                            <span class="text-sm text-base-content/75 leading-snug">{{
                                t('schedule.afterClipHelp')
                            }}</span>
                            <span
                                v-if="entryMode === 'after_clip' && entryPreview"
                                class="text-sm"
                                :style="{ color: '#9cc0e6' }"
                            >
                                {{ entryPreview }}
                            </span>
                        </span>
                    </label>
                    <label
                        class="flex gap-3 items-start rounded-lg px-4 py-3 cursor-pointer border"
                        :style="
                            entryMode === 'interrupt'
                                ? {
                                      borderColor: SCHEDULE_COLORS.interrupt,
                                      borderWidth: '2px',
                                      background: 'color-mix(in srgb, #f28c1b 14%, transparent)',
                                  }
                                : {}
                        "
                        :class="entryMode === 'interrupt' ? '' : 'border-base-content/20'"
                    >
                        <input v-model="entryMode" type="radio" value="interrupt" class="radio radio-sm mt-0.5" />
                        <span class="flex flex-col gap-1">
                            <span class="font-semibold">{{ t('schedule.interrupt') }}</span>
                            <span class="text-sm text-base-content/75 leading-snug">{{
                                t('schedule.interruptHelp')
                            }}</span>
                            <span v-if="entryMode === 'interrupt' && entryPreview" class="text-sm text-accent">
                                {{ entryPreview }}
                            </span>
                        </span>
                    </label>
                </fieldset>

                <fieldset class="flex flex-col gap-2">
                    <legend class="text-xs text-muted mb-2">{{ t('schedule.afterwards') }}</legend>
                    <label class="flex gap-2 items-center cursor-pointer">
                        <input v-model="afterMode" type="radio" value="continue" class="radio radio-sm" />
                        <span>
                            {{ t('schedule.afterContinue') }}
                            <span class="text-base-content/60">{{
                                t('schedule.delaysBy', { length: formatLength(total) })
                            }}</span>
                        </span>
                    </label>
                    <label class="flex gap-2 items-center cursor-pointer">
                        <input v-model="afterMode" type="radio" value="clock" class="radio radio-sm" />
                        <span>
                            {{ t('schedule.afterClock') }}
                            <span class="text-base-content/60">{{
                                t('schedule.skipsBy', { length: formatLength(total) })
                            }}</span>
                        </span>
                    </label>
                </fieldset>

                <div v-if="preview" class="flex flex-col gap-2">
                    <div class="text-xs text-muted">{{ t('schedule.howItAirs') }}</div>
                    <div class="grid grid-cols-[56px_minmax(0,1fr)] gap-x-3 gap-y-1.5 items-center text-xs">
                        <span class="text-base-content/60">{{ t('schedule.before') }}</span>
                        <div class="flex h-7 rounded overflow-hidden">
                            <div
                                v-for="(part, index) in preview.before"
                                :key="`b${index}`"
                                class="px-2 flex items-center truncate text-white border-l border-base-100 first:border-l-0"
                                :style="{ width: part.width, background: part.color }"
                            >
                                {{ part.label }}
                            </div>
                        </div>
                        <span class="text-base-content/60">{{ t('schedule.after') }}</span>
                        <div class="flex h-7 rounded overflow-hidden">
                            <div
                                v-for="(part, index) in preview.after"
                                :key="`a${index}`"
                                class="px-2 flex items-center truncate text-white border-l border-base-100 first:border-l-0"
                                :class="index === 1 ? 'font-semibold' : ''"
                                :style="{
                                    width: part.width,
                                    background: part.color,
                                    color: index === 1 && entryMode === 'interrupt' ? '#1c1c1c' : undefined,
                                }"
                            >
                                {{ part.label }}
                            </div>
                        </div>
                        <span />
                        <div class="flex justify-between text-base-content/60 tabular-nums">
                            <span v-for="tick in preview.ticks" :key="tick">{{ tick }}</span>
                        </div>
                    </div>
                </div>

                <label
                    class="flex items-center justify-between gap-4 border-t border-base-300 pt-4"
                    :class="allCommercial ? '' : 'cursor-pointer'"
                >
                    <span class="flex flex-col gap-0.5">
                        <span :class="allCommercial ? 'text-muted' : ''">{{ t('schedule.showInGuide') }}</span>
                        <span class="text-xs text-subtle">{{
                            allCommercial ? t('schedule.commercialsNotInGuide') : t('schedule.showInGuideHelp')
                        }}</span>
                    </span>
                    <input
                        v-model="showInGuide"
                        type="checkbox"
                        class="toggle toggle-accent"
                        :disabled="allCommercial"
                    />
                </label>

                <p v-if="error" class="text-error text-sm" role="alert">{{ error }}</p>
            </div>

            <div class="flex justify-end gap-2 px-5 py-3 border-t border-base-300">
                <button class="btn-quiet" @click="emit('close')">{{ t('cancel') }}</button>
                <button class="btn-main" :disabled="saving" @click="save">
                    <span v-if="saving" class="loading loading-spinner loading-sm" />
                    {{ t('schedule.schedule') }}
                </button>
            </div>
        </div>
    </div>
</template>
