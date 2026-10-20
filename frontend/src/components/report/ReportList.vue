<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import {
    clock,
    dayLabel,
    displayTitle,
    duration,
    isPast,
    itemDetail,
    itemSeconds,
    itemTitle,
    kindText,
    originText,
    statusText,
    type StatusTone,
} from '@/composables/report'

/** The listed items, one row each, with a line where "now" falls. */
const props = defineProps<{
    items: ReportItem[]
    report: Report
    /** The programme on air before each item. */
    previous: Map<ReportItem, ReportItem>
    /** Narrowed to chosen items: numbered, with what aired before each. */
    picked: boolean
    /** Several days: a header row starts each day. */
    period: boolean
}>()

const { t, locale } = useI18n()

/** Rows drawn at once; more come on request. */
const PAGE = 600

const shown = ref(PAGE)
// Inside the row loop, so Vue collects it as a list.
const nowRow = ref<HTMLElement[]>([])

watch(
    () => props.items,
    () => (shown.value = PAGE),
)

const zone = computed(() => props.report.timezone)

const rows = computed(() => {
    const firstPlanned = props.items.findIndex((item) => !isPast(item))
    let day = ''

    return props.items.slice(0, shown.value).map((item, index) => {
        const startsDay = props.period && item.day !== day

        day = item.day

        return {
            item,
            number: index + 1,
            startsDay,
            // The line only means something with past rows above it.
            nowBefore: index === firstPlanned && index > 0,
            status: statusText(item, t, zone.value, props.report.now),
        }
    })
})

const hidden = computed(() => Math.max(0, props.items.length - shown.value))
const hasNow = computed(() => rows.value.some((row) => row.nowBefore))
/** Columns of the table, chosen items or not. */
const COLUMNS = 7

function toneClass(tone: StatusTone): string {
    switch (tone) {
        case 'ok':
            return 'text-ok'
        case 'warn':
            return 'text-accent-text'
        case 'error':
            return 'text-on-air-text'
        default:
            return 'text-muted'
    }
}

function rowClass(item: ReportItem): string {
    if (item.state === 'on_air') {
        return 'bg-base-200'
    }

    if (item.status === 'failed') {
        return 'bg-on-air-soft'
    }

    if (item.status === 'cut') {
        return 'bg-accent-soft'
    }

    return ''
}

function kindClass(kind: ReportKind): string {
    switch (kind) {
        case 'commercial':
            return 'text-after-clip border-after-clip/40'
        case 'live':
        case 'ingest':
            return 'text-live border-live/40'
        case 'filler':
            return 'text-accent-text border-accent/40'
        default:
            return 'text-base-content/80 border-line-strong'
    }
}

function progress(item: ReportItem): number {
    return Math.min(100, Math.max(0, ((props.report.now - item.start) / Math.max(1, item.end - item.start)) * 100))
}

function toNow() {
    nowRow.value[0]?.scrollIntoView({ block: 'center', behavior: 'smooth' })
}

defineExpose({ toNow, hasNow })
</script>

<template>
    <div class="overflow-x-auto">
        <table class="w-full border-collapse text-[13px] min-w-[960px]">
            <thead>
                <tr class="text-left text-muted bg-base-200">
                    <th v-if="picked" class="ps-4 pe-2.5 py-2.5 w-12 font-medium text-right">
                        {{ t('report.list.number') }}
                    </th>
                    <th class="py-2.5 w-22 font-medium" :class="picked ? 'px-2.5' : 'ps-4 pe-2.5'">
                        {{ t('report.list.start') }}
                    </th>
                    <th class="px-2.5 py-2.5 w-22 font-medium">{{ t('report.list.end') }}</th>
                    <th class="px-2.5 py-2.5 w-20 font-medium text-right">{{ t('report.list.duration') }}</th>
                    <th class="ps-5 pe-2.5 py-2.5 font-medium">
                        {{ picked ? t('report.list.after') : t('report.list.titleColumn') }}
                    </th>
                    <th v-if="!picked" class="px-2.5 py-2.5 w-32 font-medium">{{ t('report.list.kind') }}</th>
                    <th class="px-2.5 py-2.5 w-48 font-medium">{{ t('report.list.origin') }}</th>
                    <th class="ps-2.5 pe-4 py-2.5 w-64 font-medium">{{ t('report.list.status') }}</th>
                </tr>
            </thead>
            <tbody>
                <template v-for="row in rows" :key="`${row.item.start}-${row.item.source}-${row.number}`">
                    <tr v-if="row.startsDay" class="border-t border-line bg-base-200/60">
                        <td :colspan="COLUMNS" class="px-4 py-2 font-semibold">
                            {{ dayLabel(row.item.day, locale, true) }}
                        </td>
                    </tr>
                    <tr v-if="row.nowBefore">
                        <td :colspan="COLUMNS" class="p-0">
                            <div
                                ref="nowRow"
                                class="px-4 py-1.5 bg-accent-soft border-t-2 border-accent text-accent-text text-xs font-semibold"
                            >
                                {{ t('report.list.now', { time: clock(report.now, zone).slice(0, 5) }) }}
                            </div>
                        </td>
                    </tr>
                    <tr
                        class="border-t border-raised align-top"
                        :class="[rowClass(row.item), isPast(row.item) ? '' : 'text-base-content/80']"
                    >
                        <td v-if="picked" class="ps-4 pe-2.5 py-2.5 text-right text-muted tabular">{{ row.number }}</td>
                        <td
                            class="py-2.5 font-mono tabular"
                            :class="[picked ? 'px-2.5' : 'ps-4 pe-2.5', isPast(row.item) ? '' : 'text-muted']"
                        >
                            {{ clock(row.item.start, zone) }}
                        </td>
                        <td
                            class="px-2.5 py-2.5 font-mono tabular"
                            :class="isPast(row.item) && row.item.state !== 'on_air' ? '' : 'text-muted'"
                        >
                            {{ clock(row.item.end, zone) }}
                        </td>
                        <td
                            class="px-2.5 py-2.5 font-mono tabular text-right"
                            :class="isPast(row.item) ? '' : 'text-muted'"
                        >
                            {{ duration(itemSeconds(row.item)) }}
                        </td>
                        <td class="ps-5 pe-2.5 py-2.5 min-w-0">
                            <template v-if="picked">
                                {{ previous.get(row.item) ? displayTitle(previous.get(row.item)!) : '—' }}
                            </template>
                            <template v-else>
                                <div class="font-medium" :class="row.item.kind === 'gap' ? 'text-muted' : ''">
                                    {{ itemTitle(row.item, t) }}
                                </div>
                                <div v-if="itemDetail(row.item, t)" class="font-mono text-xs text-muted break-all">
                                    {{ itemDetail(row.item, t) }}
                                </div>
                                <div
                                    v-if="row.item.state === 'on_air'"
                                    class="mt-1.5 h-1 max-w-80 rounded-sm bg-line-strong"
                                    aria-hidden="true"
                                >
                                    <div
                                        class="h-1 rounded-sm bg-on-air"
                                        :style="{ width: `${progress(row.item)}%` }"
                                    />
                                </div>
                            </template>
                        </td>
                        <td v-if="!picked" class="px-2.5 py-2.5">
                            <span
                                v-if="row.item.kind !== 'gap'"
                                class="text-xs border rounded-md px-1.5 py-px whitespace-nowrap"
                                :class="kindClass(row.item.kind)"
                            >
                                {{ kindText(row.item, t) }}
                            </span>
                        </td>
                        <td class="px-2.5 py-2.5 text-base-content/80">{{ originText(row.item, t) }}</td>
                        <td class="ps-2.5 pe-4 py-2.5">
                            <span
                                v-if="row.item.state === 'on_air'"
                                class="inline-flex items-center gap-2 bg-on-air-soft text-on-air-text rounded-full px-2.5 py-0.5 text-xs font-semibold"
                            >
                                <span class="dot bg-on-air" aria-hidden="true" />{{ row.status.label }}
                            </span>
                            <span v-else class="inline-flex items-center gap-1.5" :class="toneClass(row.status.tone)">
                                <i
                                    class="bi"
                                    :class="{
                                        'bi-check2': row.status.tone === 'ok',
                                        'bi-exclamation-triangle': row.status.tone === 'warn',
                                        'bi-x-circle': row.status.tone === 'error',
                                        'bi-clock': row.status.tone === 'muted' && row.item.kind !== 'gap',
                                        'bi-dash-circle': row.item.kind === 'gap',
                                    }"
                                    aria-hidden="true"
                                />
                                {{ row.status.label }}
                            </span>
                            <div v-if="row.status.note" class="text-xs text-muted">{{ row.status.note }}</div>
                            <div
                                v-if="row.item.status === 'failed' && row.item.detail"
                                class="text-xs text-muted truncate max-w-64"
                                :title="row.item.detail"
                            >
                                {{ row.item.detail }}
                            </div>
                        </td>
                    </tr>
                </template>
                <tr v-if="hidden">
                    <td :colspan="COLUMNS" class="px-4 py-2 border-t border-raised">
                        <button type="button" class="link-accent" @click="shown += PAGE">
                            {{ t('report.list.more', { count: Math.min(hidden, PAGE) }) }}
                        </button>
                    </td>
                </tr>
            </tbody>
        </table>
    </div>
</template>
