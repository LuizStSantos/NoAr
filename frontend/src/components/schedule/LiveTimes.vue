<script setup lang="ts">
import { useI18n } from 'vue-i18n'

import { WEEKDAY_BITS, useScheduleText } from '@/composables/schedule'

/** When a live airs: day, start and end, repetition, how it enters and leaves. */
const date = defineModel<string>('date', { required: true })
const start = defineModel<string>('start', { required: true })
const end = defineModel<string>('end', { required: true })
const repeatMode = defineModel<'once' | 'daily' | 'weekdays'>('repeatMode', { required: true })
const repeatDays = defineModel<number>('repeatDays', { required: true })
const entryMode = defineModel<EntryMode>('entryMode', { required: true })
const afterMode = defineModel<AfterMode>('afterMode', { required: true })
const retrySeconds = defineModel<number>('retrySeconds', { required: true })

const { t } = useI18n()
const { weekdayLetters } = useScheduleText()

const RETRY_OPTIONS = [60, 120, 300, 600]

function toggleDay(bit: number) {
    repeatDays.value = repeatDays.value & bit ? repeatDays.value & ~bit : repeatDays.value | bit
}
</script>

<template>
    <div class="grid grid-cols-3 gap-3">
        <label class="field-label">
            {{ t('schedule.day') }}
            <input v-model="date" type="date" class="field-input" />
        </label>
        <label class="field-label">
            {{ t('schedule.starts') }}
            <input v-model="start" type="time" step="1" class="field-input font-mono" />
        </label>
        <label class="field-label">
            {{ t('schedule.ends') }}
            <input v-model="end" type="time" step="1" class="field-input font-mono" />
        </label>
    </div>

    <div class="flex flex-wrap items-center gap-1.5">
        <button
            v-for="mode in ['once', 'daily', 'weekdays'] as const"
            :key="mode"
            type="button"
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
                type="button"
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

    <div class="grid grid-cols-2 gap-4">
        <fieldset class="flex flex-col gap-2">
            <legend class="text-xs text-muted mb-2">{{ t('schedule.whenStarting') }}</legend>
            <label class="flex gap-2 items-center cursor-pointer">
                <input v-model="entryMode" type="radio" value="interrupt" class="radio radio-sm" />
                {{ t('schedule.liveOnTime') }}
            </label>
            <label class="flex gap-2 items-center cursor-pointer">
                <input v-model="entryMode" type="radio" value="after_clip" class="radio radio-sm" />
                {{ t('schedule.afterClip') }}
            </label>
        </fieldset>
        <fieldset class="flex flex-col gap-2">
            <legend class="text-xs text-muted mb-2">{{ t('schedule.whenEnding') }}</legend>
            <label class="flex gap-2 items-center cursor-pointer">
                <input v-model="afterMode" type="radio" value="clock" class="radio radio-sm" />
                {{ t('schedule.liveBackOnClock') }}
            </label>
            <label class="flex gap-2 items-center cursor-pointer">
                <input v-model="afterMode" type="radio" value="continue" class="radio radio-sm" />
                {{ t('schedule.liveBackWhereStopped') }}
            </label>
        </fieldset>
    </div>

    <label class="field-label">
        {{ t('schedule.ifSignalDrops') }}
        <select v-model.number="retrySeconds" class="field-input">
            <option v-for="seconds in RETRY_OPTIONS" :key="seconds" :value="seconds">
                {{ t('schedule.retryFor', { minutes: seconds / 60 }) }}
            </option>
        </select>
    </label>
</template>
