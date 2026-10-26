<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'

import { clock, duration, type ReportSummary } from '@/composables/report'

/** Totals of what is listed: tiles for the whole list, a card for chosen items. */
const props = defineProps<{
    summary: ReportSummary
    picks: ReportPick[]
    /** Paths or ids of the chosen items, as the card's second line. */
    pickDetail: string
    period: boolean
    report: Report
    /** Start and end of the listed range, Unix milliseconds. */
    range: [number, number]
}>()

const { t } = useI18n()

const nowInRange = computed(() => props.report.now >= props.range[0] && props.report.now < props.range[1])
const occurrences = computed(() => props.summary.cut + props.summary.failed)

const pickKicker = computed(() => {
    const groups = new Set(props.picks.map((pick) => pick.group))
    const group = groups.size === 1 ? [...groups][0] : 'mixed'

    return t(`report.summary.kicker.${group}`)
})
</script>

<template>
    <section
        v-if="!picks.length"
        :aria-label="t('report.summary.label')"
        class="grid gap-3 grid-cols-[repeat(auto-fit,minmax(160px,1fr))]"
    >
        <div class="card-box min-w-0 px-4 py-3.5 flex flex-col gap-0.5">
            <span class="text-xs text-muted">{{ t('report.summary.aired') }}</span>
            <span class="font-mono text-lg sm:text-[22px] font-medium tabular [overflow-wrap:anywhere]">{{
                duration(summary.aired.seconds)
            }}</span>
            <span class="text-xs text-muted">
                {{ t('report.summary.items', { count: summary.aired.count }, summary.aired.count) }}
                <template v-if="nowInRange && !period">
                    · {{ clock(range[0], report.timezone).slice(0, 5) }} →
                    {{ clock(report.now, report.timezone).slice(0, 5) }}
                </template>
            </span>
        </div>
        <div class="card-box min-w-0 px-4 py-3.5 flex flex-col gap-0.5">
            <span class="text-xs text-muted">{{ t('report.summary.planned') }}</span>
            <span
                class="font-mono text-lg sm:text-[22px] font-medium tabular [overflow-wrap:anywhere] text-base-content/80"
            >
                {{ duration(summary.planned.seconds) }}
            </span>
            <span class="text-xs text-muted">
                {{ t('report.summary.items', { count: summary.planned.count }, summary.planned.count) }}
            </span>
        </div>
        <div class="card-box min-w-0 px-4 py-3.5 flex flex-col gap-0.5">
            <span class="text-xs text-after-clip">{{ t('report.summary.commercials') }}</span>
            <span class="font-mono text-lg sm:text-[22px] font-medium tabular [overflow-wrap:anywhere]">
                {{ summary.commercials.count }} · {{ duration(summary.commercials.seconds) }}
            </span>
            <span class="text-xs text-muted">
                {{
                    t('report.summary.commercialsText', {
                        aired: summary.commercials.aired,
                        planned: summary.commercials.planned,
                    })
                }}
            </span>
        </div>
        <div class="card-box min-w-0 px-4 py-3.5 flex flex-col gap-0.5">
            <span class="text-xs text-live">{{ t('report.summary.lives') }}</span>
            <span class="font-mono text-lg sm:text-[22px] font-medium tabular [overflow-wrap:anywhere]">
                {{ summary.lives.count }} · {{ duration(summary.lives.seconds) }}
            </span>
            <span class="text-xs text-muted truncate" :title="summary.lives.names.join(', ')">
                {{ summary.lives.names.join(' · ') || t('report.summary.noLives') }}
            </span>
        </div>
        <div class="card-box min-w-0 px-4 py-3.5 flex flex-col gap-0.5" :class="occurrences ? 'border-accent/50' : ''">
            <span class="text-xs" :class="occurrences ? 'text-accent-text' : 'text-muted'">
                {{ t('report.summary.occurrences') }}
            </span>
            <span class="font-mono text-lg sm:text-[22px] font-medium tabular [overflow-wrap:anywhere]">{{
                occurrences
            }}</span>
            <span class="text-xs text-muted">
                {{ t('report.summary.occurrencesText', { cut: summary.cut, failed: summary.failed }) }}
            </span>
        </div>
    </section>

    <section
        v-else
        aria-labelledby="report-item-title"
        class="card-box px-4.5 py-4 flex flex-wrap items-center gap-x-7 gap-y-3.5"
    >
        <div class="flex-[1_1_300px] min-w-0 flex flex-col gap-0.5">
            <span class="text-xs tracking-[0.06em] uppercase text-after-clip">{{ pickKicker }}</span>
            <h2 id="report-item-title" class="m-0 text-xl font-semibold truncate">
                {{ picks.map((pick) => pick.label).join(' · ') }}
            </h2>
            <span v-if="pickDetail" class="font-mono text-[13px] text-muted truncate">{{ pickDetail }}</span>
        </div>
        <div class="flex flex-col">
            <span class="text-xs text-muted">{{
                period ? t('report.summary.inPeriod') : t('report.summary.inDay')
            }}</span>
            <span class="font-mono text-xl font-medium tabular">
                {{ summary.aired.count + summary.planned.count }}× ·
                {{ duration(summary.aired.seconds + summary.planned.seconds) }}
            </span>
        </div>
        <div class="flex flex-col">
            <span class="text-xs text-muted">{{ t('report.summary.aired') }}</span>
            <span class="font-mono text-xl font-medium tabular">
                {{ summary.aired.count }}× · {{ duration(summary.aired.seconds) }}
            </span>
        </div>
        <div class="flex flex-col">
            <span class="text-xs text-muted">{{ t('report.summary.planned') }}</span>
            <span class="font-mono text-xl font-medium tabular text-base-content/80">
                {{ summary.planned.count }}× · {{ duration(summary.planned.seconds) }}
            </span>
        </div>
        <div class="flex flex-col">
            <span class="text-xs text-muted">{{ t('report.summary.origin') }}</span>
            <span class="text-sm">
                {{
                    t('report.summary.originText', {
                        playlist: summary.fromPlaylist,
                        schedule: summary.fromSchedule,
                    })
                }}
            </span>
        </div>
        <div class="flex flex-col">
            <span class="text-xs text-muted">{{ t('report.summary.occurrences') }}</span>
            <span class="text-sm" :class="occurrences ? 'text-accent-text' : 'text-ok'">
                {{
                    occurrences
                        ? t('report.summary.occurrencesText', { cut: summary.cut, failed: summary.failed })
                        : t('report.summary.allWhole')
                }}
            </span>
        </div>
    </section>
</template>
