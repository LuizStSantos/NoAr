<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'

import { secondsFromDay, secondsToTime, splitDateTime, formatLength, useScheduleText } from '@/composables/schedule'
import type { ScheduleEventBlock } from '@/composables/schedulePresentation'

/** Occurrence status and actions for the selected day; mutations are page-owned. */
const props = defineProps<{
    blocks: ScheduleEventBlock[]
    lives: LiveSource[]
    date: string
    today: string
    nowSeconds: number
    activeEventId?: number
}>()
const emit = defineEmits<{
    edit: [occurrence: ScheduleOccurrence]
    trigger: [occurrence: ScheduleOccurrence]
    remove: [event: ScheduleEvent]
}>()
const { t } = useI18n()
const { repeatLabel, entryLabel, afterLabel } = useScheduleText()
const isToday = computed(() => props.date === props.today)
const nextUp = computed(() =>
    props.blocks
        .map((block) => block.occurrence)
        .filter(
            (occurrence) =>
                !occurrence.status && isToday.value && secondsFromDay(occurrence.start, props.date) >= props.nowSeconds,
        ),
)

function occurrenceState(occurrence: ScheduleOccurrence): { label: string; tone: string } {
    const activeId = props.activeEventId
    const start = secondsFromDay(occurrence.start, props.date)

    if (activeId === occurrence.event.id && occurrence.status === 'running') {
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

    if (props.date < props.today || (isToday.value && start < props.nowSeconds - 60)) {
        return { label: t('schedule.notAired'), tone: 'muted' }
    }

    if (isToday.value && start - props.nowSeconds < 3600) {
        return {
            label: t('schedule.waitingIn', { minutes: Math.max(0, Math.ceil((start - props.nowSeconds) / 60)) }),
            tone: 'waiting',
        }
    }

    return { label: t('schedule.scheduled'), tone: 'neutral' }
}

function stateClass(tone: string): string {
    return (
        {
            accent: 'bg-accent text-accent-content font-semibold',
            waiting: 'bg-accent-soft text-accent-text',
            error: 'bg-on-air-soft text-on-air-text',
            muted: 'bg-base-200 text-muted',
            neutral: 'bg-base-200 text-base-content/85',
        }[tone] ?? ''
    )
}

function sourcesLabel(event: ScheduleEvent): string {
    if (event.kind === 'live') {
        const live = props.lives.find((item) => item.path === event.sources[0])

        return live ? `${live.protocol} · ${live.url}` : (event.sources[0] ?? '')
    }

    return [
        t('schedule.files', { count: event.sources.length }, event.sources.length),
        formatLength(event.duration),
        repeatLabel(event.repeat_days),
    ].join(' · ')
}
</script>
<template>
    <section :aria-label="t('schedule.todayEvents')" class="card-box">
        <div class="flex flex-wrap items-center justify-between gap-2 px-4.5 py-3.5 border-b border-line">
            <h2 class="m-0 text-base font-semibold">
                {{ isToday ? t('schedule.todayEvents') : t('schedule.dayEvents') }}
            </h2>
            <span class="text-[13px] text-muted">
                {{ t('schedule.countScheduled', { count: blocks.length }, blocks.length) }}
                <template v-if="nextUp[0]">
                    ·
                    {{
                        t('schedule.nextIn', {
                            minutes: Math.max(0, Math.ceil((secondsFromDay(nextUp[0].start, date) - nowSeconds) / 60)),
                        })
                    }}
                </template>
            </span>
        </div>
        <div class="overflow-x-auto">
            <table class="w-full min-w-215 text-sm">
                <thead>
                    <tr class="text-left text-[13px] text-muted">
                        <th class="px-4.5 py-2.5 font-medium">{{ t('schedule.time') }}</th>
                        <th class="px-2 py-2.5 font-medium">{{ t('schedule.material') }}</th>
                        <th class="px-2 py-2.5 font-medium">{{ t('schedule.howEnters') }}</th>
                        <th class="px-2 py-2.5 font-medium">{{ t('schedule.afterwards') }}</th>
                        <th class="px-2 py-2.5 font-medium">{{ t('schedule.inGuide') }}</th>
                        <th class="px-2 py-2.5 font-medium">{{ t('schedule.state') }}</th>
                        <th class="px-4.5 py-2.5 font-medium text-right">{{ t('schedule.actions') }}</th>
                    </tr>
                </thead>
                <tbody>
                    <tr
                        v-for="block in blocks"
                        :key="`row-${block.occurrence.event.id}-${block.occurrence.start}`"
                        class="border-t border-line"
                    >
                        <td class="px-4.5 py-3 font-mono align-top">
                            <template v-if="block.occurrence.event.kind === 'live'">
                                {{ splitDateTime(block.occurrence.start).time.slice(0, 5) }} –
                                {{ splitDateTime(block.occurrence.end).time.slice(0, 5) }}
                            </template>
                            <template v-else>{{ splitDateTime(block.occurrence.start).time }}</template>
                            <div v-if="block.start !== block.nominal" class="text-xs" :style="{ color: '#9cc0e6' }">
                                {{ t('schedule.expected', { time: secondsToTime(block.start) }) }}
                            </div>
                        </td>
                        <td class="px-2 py-3 align-top">
                            {{ block.occurrence.event.kind === 'live' ? `${t('schedule.live')} · ` : ''
                            }}{{ block.occurrence.event.title }}
                            <div class="text-[13px] text-muted truncate max-w-72">
                                {{ sourcesLabel(block.occurrence.event) }}
                            </div>
                        </td>
                        <td class="px-2 py-3 align-top">
                            <span class="inline-flex items-center gap-1.5">
                                <span
                                    class="w-2.5 h-2.5 rounded-[3px]"
                                    :style="{ background: block.color }"
                                    aria-hidden="true"
                                />
                                {{ entryLabel(block.occurrence.event) }}
                            </span>
                        </td>
                        <td class="px-2 py-3 align-top">{{ afterLabel(block.occurrence.event) }}</td>
                        <td
                            class="px-2 py-3 align-top"
                            :class="{ 'text-muted': !block.occurrence.event.show_in_guide }"
                        >
                            {{ block.occurrence.event.show_in_guide ? t('schedule.yes') : t('schedule.no') }}
                        </td>
                        <td class="px-2 py-3 align-top">
                            <span
                                class="rounded-md px-2 py-0.5 text-[13px] whitespace-nowrap"
                                :class="stateClass(occurrenceState(block.occurrence).tone)"
                            >
                                {{ occurrenceState(block.occurrence).label }}
                            </span>
                        </td>
                        <td class="px-4.5 py-2 text-right whitespace-nowrap align-top">
                            <button
                                class="btn-icon min-w-10 min-h-10 border border-line-strong"
                                :aria-label="t('schedule.playNow')"
                                :title="t('schedule.playNow')"
                                @click="emit('trigger', block.occurrence)"
                            >
                                <i class="bi bi-lightning-charge" aria-hidden="true" />
                            </button>
                            <button
                                class="btn-icon min-w-10 min-h-10 border border-line-strong ms-1.5"
                                :aria-label="t('schedule.edit')"
                                :title="t('schedule.edit')"
                                @click="emit('edit', block.occurrence)"
                            >
                                <i class="bi bi-pencil" aria-hidden="true" />
                            </button>
                            <button
                                class="btn-icon min-w-10 min-h-10 border border-line-strong text-error ms-1.5"
                                :aria-label="t('schedule.remove')"
                                :title="t('schedule.remove')"
                                @click="emit('remove', block.occurrence.event)"
                            >
                                <i class="bi bi-trash" aria-hidden="true" />
                            </button>
                        </td>
                    </tr>
                    <tr v-if="!blocks.length">
                        <td colspan="7" class="px-4 py-7 text-center text-muted">
                            {{ t('schedule.empty') }}
                        </td>
                    </tr>
                </tbody>
            </table>
        </div>
    </section>
</template>
