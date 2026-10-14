<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { storeToRefs } from 'pinia'

import { SCHEDULE_COLORS, eventColor, fileStem, formatLength, splitDateTime } from '@/composables/schedule'
import { useConfig } from '@/stores/config'
import { useIndex } from '@/stores/index'
import { useSchedule } from '@/stores/schedule'

const { t } = useI18n()
const configStore = useConfig()
const indexStore = useIndex()
const scheduleStore = useSchedule()
const { i } = storeToRefs(configStore)

const stopping = ref(false)
let timer: ReturnType<typeof setInterval> | undefined

const active = computed(() => scheduleStore.status.active)
const upcoming = computed(() => [...scheduleStore.status.queued, ...scheduleStore.status.next].slice(0, 3))
const visible = computed(() => Boolean(active.value) || upcoming.value.length > 0)
const progress = computed(() => {
    const event = active.value

    if (!event || event.planned_seconds <= 0) {
        return 0
    }

    return Math.min(100, (event.elapsed_seconds / event.planned_seconds) * 100)
})
const activeColor = computed(() =>
    active.value
        ? eventColor({ kind: active.value.kind, entry_mode: active.value.entry_mode })
        : SCHEDULE_COLORS.interrupt,
)

async function load() {
    try {
        await scheduleStore.loadStatus()
    } catch {
        // The panel stays quiet when the scheduler cannot be reached.
    }
}

async function backToProgramme() {
    stopping.value = true

    try {
        await scheduleStore.stop()
        await load()
    } catch (err) {
        indexStore.msgAlert('error', err instanceof Error ? err.message : String(err), 4)
    } finally {
        stopping.value = false
    }
}

async function playNow(occurrence: ScheduleOccurrence) {
    try {
        await scheduleStore.trigger(occurrence.event.id)
        await load()
    } catch (err) {
        indexStore.msgAlert('error', err instanceof Error ? err.message : String(err), 4)
    }
}

function startsIn(occurrence: ScheduleOccurrence): string {
    const { time } = splitDateTime(occurrence.start)

    return occurrence.status === 'running' ? t('schedule.waitingClip') : time.slice(0, 5)
}

onMounted(() => {
    load()
    timer = setInterval(load, 2000)
})

onBeforeUnmount(() => clearInterval(timer))
watch(i, load)
</script>

<template>
    <section
        v-if="visible"
        :aria-label="t('schedule.onAirPanel')"
        class="mx-1 mt-1 grid gap-3 grid-cols-1 items-start lg:grid-cols-[minmax(0,1fr)_340px]"
    >
        <div
            v-if="active"
            class="rounded-lg border-2 px-4 py-3 flex flex-col gap-2"
            :style="{ borderColor: activeColor, background: `color-mix(in srgb, ${activeColor} 14%, transparent)` }"
        >
            <div class="flex flex-wrap items-center justify-between gap-3">
                <span class="font-bold flex items-center gap-2">
                    <i class="bi" :class="active.kind === 'live' ? 'bi-broadcast' : 'bi-lightning-charge'" />
                    {{
                        active.interrupted
                            ? t('schedule.interruptedFor', { title: active.title })
                            : t('schedule.onAirNow', { title: active.title })
                    }}
                </span>
                <span class="tabular-nums text-sm">
                    {{ formatLength(active.elapsed_seconds) }} / {{ formatLength(active.planned_seconds) }}
                </span>
            </div>
            <div class="h-1.5 bg-base-300 rounded overflow-hidden">
                <div class="h-full transition-all" :style="{ width: `${progress}%`, background: activeColor }" />
            </div>
            <div class="flex flex-wrap items-center justify-between gap-3 text-sm">
                <span class="text-base-content/80">
                    <template v-if="active.after_mode === 'continue' && active.interrupted">
                        {{
                            t('schedule.returnsTo', {
                                title: active.interrupted.title || fileStem(active.interrupted.source),
                                position: formatLength(active.interrupted.position),
                            })
                        }}
                    </template>
                    <template v-else-if="active.after_mode === 'continue'">{{
                        t('schedule.returnsWhereStopped')
                    }}</template>
                    <template v-else>{{ t('schedule.returnsOnClock') }}</template>
                    <template v-if="active.item"> · {{ active.item }}</template>
                </span>
                <div class="flex gap-2">
                    <button
                        class="btn btn-sm btn-outline border-base-content/30"
                        :disabled="stopping"
                        @click="backToProgramme"
                    >
                        {{ t('schedule.backToProgramme') }}
                    </button>
                    <RouterLink to="/schedule" class="btn btn-sm btn-ghost">{{
                        t('schedule.openScheduler')
                    }}</RouterLink>
                </div>
            </div>
        </div>
        <div v-else class="hidden lg:block" />

        <div v-if="upcoming.length" class="rounded-lg border border-base-300 bg-base-100 px-4 py-3 flex flex-col gap-2">
            <div class="text-sm font-semibold">{{ t('schedule.upNext') }}</div>
            <div
                v-for="occurrence in upcoming"
                :key="`${occurrence.event.id}-${occurrence.start}`"
                class="flex items-center gap-3"
            >
                <span
                    class="w-2.5 h-2.5 rounded-sm flex-none"
                    :style="{ background: eventColor(occurrence.event) }"
                    aria-hidden="true"
                />
                <div class="flex-1 min-w-0">
                    <div class="text-sm font-semibold truncate">{{ occurrence.event.title }}</div>
                    <div class="text-xs text-base-content/60">{{ startsIn(occurrence) }}</div>
                </div>
                <button
                    class="btn btn-sm btn-square btn-outline border-base-content/30"
                    :aria-label="t('schedule.playNowTitle', { title: occurrence.event.title })"
                    :title="t('schedule.playNow')"
                    @click="playNow(occurrence)"
                >
                    <i class="bi bi-lightning-charge" />
                </button>
            </div>
        </div>
    </section>
</template>
