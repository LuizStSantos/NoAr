<script setup lang="ts">
import dayjs from 'dayjs'
import timezone from 'dayjs/plugin/timezone.js'
import utc from 'dayjs/plugin/utc.js'
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useHead } from '@unhead/vue'

import ReportDays from '@/components/report/ReportDays.vue'
import ReportList from '@/components/report/ReportList.vue'
import ReportPicker from '@/components/report/ReportPicker.vue'
import ReportSummary from '@/components/report/ReportSummary.vue'
import { authFetch } from '@/composables/authFetch'
import {
    REPORT_KINDS,
    fileName,
    filterItems,
    pickGroups,
    previousProgrammes,
    shortDate,
    summarize,
    summarizeDays,
    type ReportPart,
} from '@/composables/report'
import { downloadCsv, downloadPdf, type ReportExport } from '@/composables/reportExport'
import { useConfig } from '@/stores/config'
import { useIndex } from '@/stores/index'

dayjs.extend(utc)
dayjs.extend(timezone)

const { t, locale } = useI18n()
const configStore = useConfig()
const indexStore = useIndex()

useHead({ title: computed(() => t('report.title')) })

/** Longest period, as the server allows. */
const MAX_DAYS = 62
/** How often a range that includes now is read again. */
const REFRESH_SECONDS = 60

const channel = computed(() => configStore.channels[configStore.i])
const zone = computed(() => configStore.timezone)

/** The playlist day running now: before the day start, it is yesterday's. */
function currentDay(): string {
    const now = dayjs().tz(zone.value)
    const seconds = now.hour() * 3600 + now.minute() * 60 + now.second()
    const start = configStore.playout.playlist?.startInSec ?? 0

    return (seconds < start ? now.subtract(1, 'day') : now).format('YYYY-MM-DD')
}

function readSetting(key: string, fallback: string): string {
    try {
        return localStorage.getItem(key) ?? fallback
    } catch {
        return fallback
    }
}

function writeSetting(key: string, value: string) {
    try {
        localStorage.setItem(key, value)
    } catch {
        // Only a convenience: the choice is not kept.
    }
}

const mode = ref<'day' | 'period'>(readSetting('report-mode', 'day') === 'period' ? 'period' : 'day')
const day = ref(currentDay())
const from = ref(dayjs(currentDay()).startOf('month').format('YYYY-MM-DD'))
const to = ref(dayjs(currentDay()).endOf('month').format('YYYY-MM-DD'))
const part = ref<ReportPart>('all')
const kinds = ref<ReportKind[]>([...REPORT_KINDS])
const picks = ref<ReportPick[]>([])
const periodView = ref<'days' | 'items'>('days')
const report = ref<Report | null>(null)
const loading = ref(false)
const exporting = ref(false)
const list = ref<InstanceType<typeof ReportList>>()
let timer: ReturnType<typeof setInterval> | undefined
let request = 0

const range = computed<[string, string]>(() => (mode.value === 'day' ? [day.value, day.value] : [from.value, to.value]))

const rangeError = computed(() => {
    const [first, last] = range.value

    if (!first || !last) {
        return t('report.errors.dates')
    }

    if (last < first) {
        return t('report.errors.order')
    }

    if (dayjs(last).diff(dayjs(first), 'day') + 1 > MAX_DAYS) {
        return t('report.errors.tooLong', { days: MAX_DAYS })
    }

    return ''
})

const items = computed(() => report.value?.items ?? [])
const rows = computed(() =>
    filterItems(items.value, { part: part.value, kinds: kinds.value, search: '', picks: picks.value }),
)
const previous = computed(() => previousProgrammes(items.value))
const summary = computed(() => summarize(rows.value, report.value?.now ?? Date.now()))
const daySummaries = computed(() => (report.value ? summarizeDays(report.value.days, rows.value) : []))
const groups = computed(() =>
    pickGroups(items.value, (folder) => t('report.picker.wholeFolder', { folder: fileName(folder) })),
)
const bounds = computed<[number, number]>(() => {
    const days = report.value?.days ?? []

    return [days[0]?.start ?? 0, days[days.length - 1]?.end ?? 0]
})
const includesNow = computed(() => {
    const now = report.value?.now ?? Date.now()

    return now >= bounds.value[0] && now < bounds.value[1]
})
const unrecordedSince = computed(() => {
    const since = report.value?.recorded_since

    if (!rows.value.some((item) => item.state === 'unrecorded')) {
        return ''
    }

    if (!since) {
        return t('report.notice.neverRecorded')
    }

    const time = dayjs(since).tz(zone.value)

    return t('report.notice.unrecorded', {
        date: shortDate(time.format('YYYY-MM-DD'), locale.value),
        time: time.format('HH:mm'),
    })
})

const dayWindow = computed(() => {
    const first = report.value?.days[0]

    if (!first || mode.value !== 'day') {
        return ''
    }

    const start = dayjs(first.start).tz(zone.value)
    const end = dayjs(first.end).tz(zone.value)

    return t('report.dayWindow', {
        start: start.format('HH:mm'),
        startDate: start.format('DD/MM'),
        end: end.format('HH:mm'),
        endDate: end.format('DD/MM'),
    })
})

const rangeText = computed(() => {
    const [first, last] = range.value

    return first === last
        ? shortDate(first, locale.value)
        : t('report.rangePeriod', { from: shortDate(first, locale.value), to: shortDate(last, locale.value) })
})

const filtersText = computed(() => {
    if (picks.value.length) {
        return picks.value.map((pick) => pick.label).join(', ')
    }

    const kindsText =
        kinds.value.length === REPORT_KINDS.length
            ? t('report.filters.allKinds')
            : kinds.value.map((kind) => t(`report.kinds.${kind}`)).join(', ')

    return `${t(`report.parts.${part.value}`)} · ${kindsText}`
})

const downloadHint = computed(() =>
    picks.value.length
        ? t('report.downloadPicked', {
              items: picks.value.map((pick) => pick.label).join(', '),
              range: rangeText.value,
          })
        : t('report.downloadHint'),
)

/** How many items the list holds, of how many in the range. */
const listedCount = computed(() => {
    const listed = rows.value.filter((item) => item.kind !== 'gap').length
    const total = items.value.filter((item) => item.kind !== 'gap').length

    return picks.value.length
        ? t('report.list.countOf', { count: listed, total })
        : t('report.summary.items', { count: listed }, listed)
})

const pickDetail = computed(() => {
    const [pick] = picks.value

    return picks.value.length === 1 && pick && pick.type !== 'event' ? pick.value : ''
})

async function load(quiet = false) {
    if (!channel.value || rangeError.value) {
        return
    }

    const ticket = ++request
    const [first, last] = range.value

    loading.value = !quiet

    try {
        const query = new URLSearchParams({ from: first, to: last })
        const data = await authFetch<Report>(`/api/report/${channel.value.id}?${query}`)

        if (ticket === request) {
            report.value = data
        }
    } catch (error) {
        if (ticket === request && !quiet) {
            report.value = null
            indexStore.msgAlert('error', error instanceof Error ? error.message : String(error), 5)
        }
    } finally {
        if (ticket === request) {
            loading.value = false
        }
    }
}

function shiftDay(days: number) {
    day.value = dayjs(day.value).add(days, 'day').format('YYYY-MM-DD')
}

function setPeriod(which: 'week' | 'month' | 'lastMonth') {
    const today = dayjs(currentDay())

    switch (which) {
        case 'week': {
            // Weeks start on Monday.
            const monday = today.subtract((today.day() + 6) % 7, 'day')

            from.value = monday.format('YYYY-MM-DD')
            to.value = monday.add(6, 'day').format('YYYY-MM-DD')
            break
        }
        case 'month':
            from.value = today.startOf('month').format('YYYY-MM-DD')
            to.value = today.endOf('month').format('YYYY-MM-DD')
            break
        case 'lastMonth': {
            const month = today.subtract(1, 'month')

            from.value = month.startOf('month').format('YYYY-MM-DD')
            to.value = month.endOf('month').format('YYYY-MM-DD')
            break
        }
    }
}

const quickPeriod = computed(() => {
    const today = dayjs(currentDay())
    const monday = today.subtract((today.day() + 6) % 7, 'day')
    const lastMonth = today.subtract(1, 'month')

    if (from.value === monday.format('YYYY-MM-DD') && to.value === monday.add(6, 'day').format('YYYY-MM-DD')) {
        return 'week'
    }

    if (
        from.value === today.startOf('month').format('YYYY-MM-DD') &&
        to.value === today.endOf('month').format('YYYY-MM-DD')
    ) {
        return 'month'
    }

    if (
        from.value === lastMonth.startOf('month').format('YYYY-MM-DD') &&
        to.value === lastMonth.endOf('month').format('YYYY-MM-DD')
    ) {
        return 'lastMonth'
    }

    return ''
})

function openDay(date: string) {
    day.value = date
    mode.value = 'day'
}

function slug(text: string): string {
    return text
        .normalize('NFD')
        .replace(/[̀-ͯ]/g, '')
        .toLowerCase()
        .replace(/[^a-z0-9]+/g, '-')
        .replace(/^-|-$/g, '')
}

function exportData(): ReportExport | null {
    if (!report.value) {
        return null
    }

    const [first, last] = range.value
    const name = [
        'noar-relatorio',
        slug(channel.value?.name ?? ''),
        first === last ? first : `${first}_${last}`,
        ...picks.value.slice(0, 2).map((pick) => slug(pick.label)),
    ]
        .filter(Boolean)
        .join('-')

    return {
        t,
        locale: locale.value,
        report: report.value,
        channel: channel.value?.name ?? '',
        rows: rows.value,
        previous: previous.value,
        summary: summary.value,
        days: mode.value === 'period' ? daySummaries.value : null,
        picks: picks.value,
        range: rangeText.value,
        filters: filtersText.value,
        file: name,
    }
}

function sheet() {
    const data = exportData()

    if (data) {
        downloadCsv(data)
    }
}

async function pdf() {
    const data = exportData()

    if (!data) {
        return
    }

    exporting.value = true

    try {
        await downloadPdf(data)
    } catch (error) {
        indexStore.msgAlert('error', error instanceof Error ? error.message : String(error), 5)
    } finally {
        exporting.value = false
    }
}

function toggleKind(kind: ReportKind) {
    kinds.value = kinds.value.includes(kind) ? kinds.value.filter((other) => other !== kind) : [...kinds.value, kind]
}

function kindChipClass(kind: ReportKind): string {
    switch (kind) {
        case 'commercial':
            return 'border-after-clip/50 text-after-clip'
        case 'live':
        case 'ingest':
            return 'border-live/50 text-live'
        case 'filler':
            return 'border-accent/50 text-accent-text'
        default:
            return 'border-line-strong'
    }
}

watch([range, () => channel.value?.id], () => load())
watch(mode, (value) => writeSetting('report-mode', value))

onMounted(() => {
    load()
    timer = setInterval(() => {
        if (includesNow.value && !document.hidden) {
            load(true)
        }
    }, REFRESH_SECONDS * 1000)
})

onBeforeUnmount(() => clearInterval(timer))
</script>

<template>
    <div class="max-w-[1440px] mx-auto px-4 sm:px-6 py-6 flex flex-col gap-4">
        <div class="flex flex-wrap items-end gap-x-4 gap-y-3">
            <div class="flex-[1_1_320px] min-w-0">
                <h1 class="m-0 text-2xl font-semibold">{{ t('report.title') }}</h1>
                <p class="mt-1 mb-0 text-muted">
                    {{ t('report.intro', { channel: channel?.name ?? '', timezone: zone }) }}
                </p>
            </div>
            <div class="flex flex-col items-end gap-1.5">
                <div class="flex flex-wrap justify-end gap-2">
                    <button type="button" class="btn-quiet" :disabled="!report || loading" @click="sheet">
                        <i class="bi bi-table" aria-hidden="true" />{{ t('report.downloadSheet') }}
                    </button>
                    <button type="button" class="btn-main" :disabled="!report || loading || exporting" @click="pdf">
                        <span v-if="exporting" class="loading loading-spinner loading-sm" />
                        <i v-else class="bi bi-download" aria-hidden="true" />{{ t('report.downloadPdf') }}
                    </button>
                </div>
                <span class="text-xs text-right" :class="picks.length ? 'text-accent-text' : 'text-muted'">
                    {{ downloadHint }}
                </span>
            </div>
        </div>

        <section
            :aria-label="t('report.filters.label')"
            class="card-box px-3.5 py-3 flex flex-wrap items-center gap-2.5"
        >
            <div
                role="radiogroup"
                :aria-label="t('report.filters.range')"
                class="inline-flex bg-base-200 border border-line-strong rounded-field p-[3px]"
            >
                <label
                    v-for="option in ['day', 'period'] as const"
                    :key="option"
                    class="inline-flex items-center px-3 min-h-8.5 rounded-md text-[13px] cursor-pointer focus-within:outline-2 focus-within:outline-accent"
                    :class="mode === option ? 'bg-base-300 text-base-content' : 'text-muted'"
                >
                    <input v-model="mode" type="radio" name="report-mode" :value="option" class="sr-only" />
                    {{ t(`report.filters.${option}`) }}
                </label>
            </div>

            <template v-if="mode === 'day'">
                <div class="inline-flex items-center">
                    <button
                        type="button"
                        class="btn-quiet rounded-e-none px-0 min-w-10"
                        :aria-label="t('report.filters.previousDay')"
                        @click="shiftDay(-1)"
                    >
                        <i class="bi bi-chevron-left" aria-hidden="true" />
                    </button>
                    <label class="relative">
                        <span class="sr-only">{{ t('report.filters.dayLabel') }}</span>
                        <input
                            v-model="day"
                            type="date"
                            required
                            class="field-input rounded-none border-x-0 font-mono w-auto min-w-40 bg-raised"
                        />
                    </label>
                    <button
                        type="button"
                        class="btn-quiet rounded-s-none px-0 min-w-10"
                        :aria-label="t('report.filters.nextDay')"
                        @click="shiftDay(1)"
                    >
                        <i class="bi bi-chevron-right" aria-hidden="true" />
                    </button>
                </div>
                <button type="button" class="btn-quiet" :disabled="day === currentDay()" @click="day = currentDay()">
                    {{ t('report.filters.today') }}
                </button>
                <span v-if="dayWindow" class="text-[13px] text-muted">{{ dayWindow }}</span>
            </template>

            <template v-else>
                <label class="inline-flex items-center gap-2 text-[13px] text-muted">
                    {{ t('report.filters.from') }}
                    <input
                        v-model="from"
                        type="date"
                        required
                        class="field-input font-mono w-auto min-w-40 bg-raised"
                    />
                </label>
                <label class="inline-flex items-center gap-2 text-[13px] text-muted">
                    {{ t('report.filters.to') }}
                    <input v-model="to" type="date" required class="field-input font-mono w-auto min-w-40 bg-raised" />
                </label>
                <div class="inline-flex flex-wrap gap-1.5">
                    <button
                        v-for="option in ['week', 'month', 'lastMonth'] as const"
                        :key="option"
                        type="button"
                        class="rounded-full border px-3 min-h-8.5 text-[13px] cursor-pointer"
                        :class="
                            quickPeriod === option
                                ? 'bg-base-300 border-line-strong text-base-content'
                                : 'bg-transparent border-line text-base-content/80'
                        "
                        :aria-pressed="quickPeriod === option"
                        @click="setPeriod(option)"
                    >
                        {{ t(`report.filters.${option}`) }}
                    </button>
                </div>
            </template>

            <div class="flex-auto" />

            <div
                role="radiogroup"
                :aria-label="t('report.filters.part')"
                class="inline-flex bg-base-200 border border-line-strong rounded-field p-[3px]"
            >
                <label
                    v-for="option in ['all', 'aired', 'planned'] as const"
                    :key="option"
                    class="inline-flex items-center px-3 min-h-8.5 rounded-md text-[13px] cursor-pointer focus-within:outline-2 focus-within:outline-accent"
                    :class="part === option ? 'bg-base-300 text-base-content' : 'text-muted'"
                >
                    <input v-model="part" type="radio" name="report-part" :value="option" class="sr-only" />
                    {{ t(`report.parts.${option}`) }}
                </label>
            </div>

            <ReportPicker v-model="picks" :groups="groups" :range="mode" />

            <fieldset v-if="!picks.length" class="border-0 m-0 p-0 inline-flex flex-wrap gap-1.5 basis-full">
                <legend class="sr-only">{{ t('report.filters.kinds') }}</legend>
                <label
                    v-for="kind in REPORT_KINDS"
                    :key="kind"
                    class="inline-flex items-center gap-1.5 border rounded-full px-3 min-h-8.5 text-[13px] cursor-pointer"
                    :class="kindChipClass(kind)"
                >
                    <input
                        type="checkbox"
                        class="checkbox checkbox-xs checkbox-accent"
                        :checked="kinds.includes(kind)"
                        @change="toggleKind(kind)"
                    />
                    {{ t(`report.kinds.${kind}`) }}
                </label>
            </fieldset>

            <p v-if="rangeError" class="m-0 basis-full text-sm text-on-air-text" role="alert">{{ rangeError }}</p>
        </section>

        <p
            v-if="unrecordedSince"
            class="m-0 flex items-start gap-2 text-[13px] text-muted bg-base-200 rounded-field px-3.5 py-2.5"
        >
            <i class="bi bi-info-circle mt-px" aria-hidden="true" />{{ unrecordedSince }}
        </p>

        <ReportSummary
            v-if="report"
            :summary="summary"
            :picks="picks"
            :pick-detail="pickDetail"
            :period="mode === 'period'"
            :report="report"
            :range="bounds"
        />

        <section aria-labelledby="report-list-title" class="card-box overflow-hidden relative">
            <div class="flex flex-wrap items-center gap-x-3 gap-y-2 px-4 py-3.5 border-b border-line">
                <h2 id="report-list-title" class="m-0 text-[15px] font-semibold">
                    <template v-if="picks.length">
                        {{ t('report.list.timesTitle', { range: rangeText }) }}
                    </template>
                    <template v-else>{{ t('report.list.title', { range: rangeText }) }}</template>
                </h2>
                <span class="font-mono text-xs text-base-content/80 bg-raised rounded-md px-2 py-0.5">
                    {{ listedCount }}
                </span>
                <div class="flex-auto" />
                <div
                    v-if="mode === 'period'"
                    role="radiogroup"
                    :aria-label="t('report.list.show')"
                    class="inline-flex bg-base-200 border border-line-strong rounded-field p-[3px]"
                >
                    <label
                        v-for="option in ['days', 'items'] as const"
                        :key="option"
                        class="inline-flex items-center px-3 min-h-8.5 rounded-md text-[13px] cursor-pointer focus-within:outline-2 focus-within:outline-accent"
                        :class="periodView === option ? 'bg-base-300 text-base-content' : 'text-muted'"
                    >
                        <input v-model="periodView" type="radio" name="report-view" :value="option" class="sr-only" />
                        {{ t(`report.list.view.${option}`) }}
                    </label>
                </div>
                <span v-else-if="includesNow" class="text-[13px] text-muted hidden md:inline">
                    {{ t('report.list.legend') }}
                </span>
                <button
                    v-if="mode === 'day' && includesNow && !picks.length"
                    type="button"
                    class="btn-quiet min-h-8.5 text-[13px]"
                    @click="list?.toNow()"
                >
                    {{ t('report.list.toNow') }}
                </button>
            </div>

            <div v-if="loading" class="absolute inset-0 z-10 flex justify-center pt-16 bg-base-100/60">
                <span class="loading loading-spinner loading-lg" />
            </div>

            <template v-if="report">
                <ReportDays
                    v-if="mode === 'period' && periodView === 'days'"
                    :days="daySummaries"
                    :report="report"
                    :picked="picks.length > 0"
                    @open="openDay"
                />
                <ReportList
                    v-else-if="rows.length"
                    ref="list"
                    :items="rows"
                    :report="report"
                    :previous="previous"
                    :picked="picks.length > 0"
                    :period="mode === 'period'"
                />
                <p v-else class="m-0 px-4 py-8 text-muted">
                    {{ items.length ? t('report.list.noMatch') : t('report.list.empty') }}
                </p>
            </template>
            <p v-else-if="!loading" class="m-0 px-4 py-8 text-muted">{{ t('report.list.empty') }}</p>
        </section>
    </div>
</template>
