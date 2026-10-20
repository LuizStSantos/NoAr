<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'

import { dayLabel, dayText, duration, type DaySummary } from '@/composables/report'

/** A period day by day: how many items, how long, how they went. */
const props = defineProps<{
    days: DaySummary[]
    report: Report
    picked: boolean
}>()

const emit = defineEmits<{ open: [date: string] }>()

const { t, locale } = useI18n()

/** The days added up, written like a day. */
const total = computed(() => {
    const sum = (key: 'count' | 'seconds' | 'aired' | 'unrecorded' | 'planned' | 'cut' | 'failed') =>
        props.days.reduce((total, day) => total + day[key], 0)
    const summary: DaySummary = {
        day: { date: '', start: 0, end: 0, playlist: true },
        count: sum('count'),
        seconds: sum('seconds'),
        aired: sum('aired'),
        unrecorded: sum('unrecorded'),
        planned: sum('planned'),
        cut: sum('cut'),
        failed: sum('failed'),
    }

    return { ...summary, text: dayText(summary, t) }
})

function isToday(day: DaySummary): boolean {
    return props.report.now >= day.day.start && props.report.now < day.day.end
}

function isFuture(day: DaySummary): boolean {
    return day.day.start > props.report.now
}

type Tone = 'ok' | 'warn' | 'muted' | 'mixed'

function dayStatus(day: DaySummary): { text: string; note: string; tone: Tone } {
    const text = dayText(day, t)

    if (!day.count) {
        return { text, note: '', tone: 'muted' }
    }

    if (!day.day.playlist && day.planned === day.count) {
        return { text, note: t('report.days.noPlaylistNote'), tone: 'warn' }
    }

    if (day.cut || day.failed) {
        return { text, note: '', tone: 'warn' }
    }

    if (day.aired === day.count) {
        return { text, note: '', tone: 'ok' }
    }

    return { text, note: '', tone: day.aired ? 'mixed' : 'muted' }
}

function rowClass(day: DaySummary): string {
    if (isToday(day)) {
        return 'bg-base-200'
    }

    return day.cut || day.failed ? 'bg-accent-soft' : ''
}
</script>

<template>
    <div class="overflow-x-auto">
        <table class="w-full border-collapse text-[13px] min-w-[760px]">
            <thead>
                <tr class="text-left text-muted bg-base-200">
                    <th class="ps-4 pe-2.5 py-2.5 w-44 font-medium">{{ t('report.days.day') }}</th>
                    <th class="px-2.5 py-2.5 w-24 font-medium text-right">
                        {{ picked ? t('report.days.times') : t('report.days.items') }}
                    </th>
                    <th class="px-2.5 py-2.5 w-28 font-medium text-right">{{ t('report.days.time') }}</th>
                    <th class="ps-6 pe-2.5 py-2.5 font-medium">{{ t('report.days.status') }}</th>
                    <th class="ps-2.5 pe-4 py-2.5 w-32 font-medium">
                        <span class="sr-only">{{ t('report.days.open') }}</span>
                    </th>
                </tr>
            </thead>
            <tbody>
                <tr v-for="day in days" :key="day.day.date" class="border-t border-raised" :class="rowClass(day)">
                    <td class="ps-4 pe-2.5 py-2" :class="isToday(day) ? 'font-semibold' : ''">
                        {{ dayLabel(day.day.date, locale) }}
                        <span v-if="isToday(day)" class="font-normal text-muted"> · {{ t('report.days.today') }}</span>
                    </td>
                    <td class="px-2.5 py-2 font-mono tabular text-right" :class="isFuture(day) ? 'text-muted' : ''">
                        {{ day.count }}
                    </td>
                    <td class="px-2.5 py-2 font-mono tabular text-right" :class="isFuture(day) ? 'text-muted' : ''">
                        {{ duration(day.seconds) }}
                    </td>
                    <td class="ps-6 pe-2.5 py-2">
                        <span
                            class="inline-flex items-center gap-1.5"
                            :class="{
                                'text-ok': dayStatus(day).tone === 'ok',
                                'text-accent-text': dayStatus(day).tone === 'warn',
                                'text-muted': dayStatus(day).tone === 'muted',
                            }"
                        >
                            <i
                                class="bi"
                                :class="{
                                    'bi-check2': dayStatus(day).tone === 'ok',
                                    'bi-exclamation-triangle': dayStatus(day).tone === 'warn',
                                    'bi-clock': dayStatus(day).tone === 'muted',
                                    'bi-circle-half': dayStatus(day).tone === 'mixed',
                                }"
                                aria-hidden="true"
                            />
                            {{ dayStatus(day).text }}
                        </span>
                        <span v-if="dayStatus(day).note" class="text-muted"> · {{ dayStatus(day).note }}</span>
                    </td>
                    <td class="ps-2.5 pe-4 py-1">
                        <button type="button" class="link-accent min-h-9" @click="emit('open', day.day.date)">
                            {{ t('report.days.open') }}
                        </button>
                    </td>
                </tr>
                <tr class="border-t-2 border-line-strong font-semibold">
                    <td class="ps-4 pe-2.5 py-2.5">{{ t('report.days.total') }}</td>
                    <td class="px-2.5 py-2.5 font-mono tabular text-right">{{ total.count }}</td>
                    <td class="px-2.5 py-2.5 font-mono tabular text-right">{{ duration(total.seconds) }}</td>
                    <td class="ps-6 pe-2.5 py-2.5 font-normal text-base-content/80">{{ total.text }}</td>
                    <td />
                </tr>
            </tbody>
        </table>
    </div>
</template>
