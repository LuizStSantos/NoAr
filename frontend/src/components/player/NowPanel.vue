<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import BreakStrip from '@/components/player/BreakStrip.vue'
import { useOnAir } from '@/composables/onAir'
import { clockShort, lengthLabel, meterPercent } from '@/composables/player'
import { fileStem, formatLength, splitDateTime } from '@/composables/schedule'
import { useConfig } from '@/stores/config'
import { useIndex } from '@/stores/index'
import { usePlaylist } from '@/stores/playlist'
import { useSchedule } from '@/stores/schedule'

const { t, locale } = useI18n()
const configStore = useConfig()
const indexStore = useIndex()
const playlistStore = usePlaylist()
const scheduleStore = useSchedule()
const { channelNow, playlistNow, clipLength, clipPosition, clipRemaining, currentTitle, nextGroup, onAirIndex } =
    useOnAir()

const stopping = ref(false)

const dateLabel = computed(() => {
    const label = channelNow.value.locale(locale.value.toLowerCase()).format('ddd, D MMM YYYY')

    return label.charAt(0).toUpperCase() + label.slice(1)
})

const progress = computed(() => (clipLength.value > 0 ? (clipPosition.value / clipLength.value) * 100 : 0))

const nextIn = computed(() => (nextGroup.value ? Math.max(0, nextGroup.value.start - playlistNow.value) : 0))

const sync = computed(() => {
    const shift = playlistStore.shift ?? 0

    return {
        ok: Math.abs(shift) <= 2,
        value: `${shift >= 0 ? '+' : '−'}${Math.abs(shift).toLocaleString(locale.value, {
            minimumFractionDigits: 2,
            maximumFractionDigits: 2,
        })} s`,
    }
})

const active = computed(() => scheduleStore.status.active)
const activeProgress = computed(() =>
    active.value && active.value.planned_seconds > 0
        ? Math.min(100, (active.value.elapsed_seconds / active.value.planned_seconds) * 100)
        : 0,
)
const nextEvent = computed(() => [...scheduleStore.status.queued, ...scheduleStore.status.next][0])

function eventMode(occurrence: ScheduleOccurrence): string {
    if (occurrence.status === 'running') {
        return t('onair.eventWaiting')
    }

    if (occurrence.event.kind === 'live') {
        return t('onair.eventLive')
    }

    return occurrence.event.entry_mode === 'interrupt' ? t('onair.eventInterrupts') : t('onair.eventAfterClip')
}

const loudnessLabel = computed(() => {
    const loudness = playlistStore.loudness
    const level = playlistStore.audioLevel
    const format = (value: number) =>
        `${value < 0 ? '−' : ''}${Math.abs(value).toLocaleString(locale.value, {
            minimumFractionDigits: 1,
            maximumFractionDigits: 1,
        })}`

    if (loudness?.short_term_lufs !== null && loudness?.short_term_lufs !== undefined) {
        const peak = loudness.true_peak_dbtp ?? level?.peak_db

        return `${format(loudness.short_term_lufs)} LUFS${peak !== null && peak !== undefined ? ` · ${t('onair.peak')} ${format(peak)} dBTP` : ''}`
    }

    if (level) {
        return `${format(level.rms_db)} dB RMS · ${t('onair.peak')} ${format(level.peak_db)} dB`
    }

    return t('onair.noAudio')
})

const meters = computed(() => [
    { label: 'RMS', value: meterPercent(playlistStore.audioLevel?.rms_db) },
    { label: t('onair.peakShort'), value: meterPercent(playlistStore.audioLevel?.peak_db) },
])

async function backToProgramme() {
    stopping.value = true

    try {
        await scheduleStore.stop()
        await scheduleStore.loadStatus()
    } catch (err) {
        indexStore.msgAlert('error', err instanceof Error ? err.message : String(err), 4)
    } finally {
        stopping.value = false
    }
}
</script>
<template>
    <section :aria-label="t('onair.now')" class="card-box px-5 py-4.5 flex flex-col gap-4.5">
        <div class="flex flex-wrap items-end justify-between gap-x-3 gap-y-1">
            <div>
                <div class="text-muted text-xs uppercase tracking-[0.08em]">{{ t('onair.now') }}</div>
                <div class="font-mono text-[40px] sm:text-[52px] font-medium leading-[1.05] tabular">
                    {{ channelNow.format('HH:mm:ss') }}
                </div>
            </div>
            <div class="ms-auto text-right text-muted text-[13px]">{{ dateLabel }}<br />{{ configStore.timezone }}</div>
        </div>

        <div
            v-if="active"
            class="rounded-[10px] border px-3.5 py-3 flex flex-col gap-2"
            :class="active.kind === 'live' ? 'border-live bg-live-soft' : 'border-accent bg-accent-soft'"
        >
            <div class="flex items-center justify-between gap-2">
                <span class="font-semibold truncate">
                    {{
                        active.interrupted
                            ? t('schedule.interruptedFor', { title: active.title })
                            : t('schedule.onAirNow', { title: active.title })
                    }}
                </span>
                <span class="font-mono text-xs tabular">
                    {{ formatLength(active.elapsed_seconds) }} / {{ formatLength(active.planned_seconds) }}
                </span>
            </div>
            <div class="h-1.5 rounded bg-base-300 overflow-hidden">
                <div
                    class="h-full"
                    :class="active.kind === 'live' ? 'bg-live' : 'bg-accent'"
                    :style="{ width: `${activeProgress}%` }"
                />
            </div>
            <div class="flex flex-wrap items-center justify-between gap-2 text-[13px]">
                <span class="text-muted">
                    <template v-if="active.after_mode === 'continue' && active.interrupted">
                        {{
                            t('schedule.returnsTo', {
                                title:
                                    active.interrupted.program ||
                                    active.interrupted.title ||
                                    fileStem(active.interrupted.source),
                                position: formatLength(active.interrupted.position),
                            })
                        }}
                    </template>
                    <template v-else-if="active.after_mode === 'continue'">{{
                        t('schedule.returnsWhereStopped')
                    }}</template>
                    <template v-else>{{ t('schedule.returnsOnClock') }}</template>
                </span>
                <button class="btn-quiet min-h-9" :disabled="stopping" @click="backToProgramme">
                    {{ t('schedule.backToProgramme') }}
                </button>
            </div>
        </div>

        <div class="flex flex-col gap-2">
            <div class="flex justify-between items-baseline gap-2">
                <span class="font-semibold text-[17px] truncate">
                    {{ playlistStore.playoutIsRunning ? currentTitle : t('onair.notRunning') }}
                </span>
                <span class="font-mono text-[26px] font-medium text-accent-text tabular">
                    −{{ lengthLabel(clipRemaining) }}
                </span>
            </div>
            <div class="h-2 rounded bg-base-300 overflow-hidden">
                <div
                    class="h-full bg-accent transition-[width] duration-1000 ease-linear"
                    :style="{ width: `${progress}%` }"
                />
            </div>
            <div class="flex justify-between text-muted font-mono text-xs">
                <span>{{
                    t('onair.positionOf', { position: lengthLabel(clipPosition), length: lengthLabel(clipLength) })
                }}</span>
                <span v-if="onAirIndex >= 0">
                    {{
                        t('onair.clipOf', {
                            index: (onAirIndex + 1).toLocaleString(locale),
                            total: playlistStore.playlist.length.toLocaleString(locale),
                        })
                    }}
                </span>
            </div>
        </div>

        <BreakStrip />

        <div class="grid grid-cols-1 xs:grid-cols-3 gap-2.5">
            <div class="bg-raised rounded-[10px] px-3 py-2.5 min-w-0">
                <div class="text-muted text-xs">{{ t('onair.upNext') }}</div>
                <template v-if="nextGroup">
                    <div class="font-semibold line-clamp-2 break-words">{{ nextGroup.title }}</div>
                    <div class="font-mono text-xs text-muted">
                        {{ clockShort(nextGroup.start) }} · {{ t('onair.inTime', { time: lengthLabel(nextIn) }) }}
                    </div>
                </template>
                <div v-else class="text-muted">—</div>
            </div>
            <div class="bg-raised rounded-[10px] px-3 py-2.5 min-w-0">
                <div class="text-muted text-xs">{{ t('onair.sync') }}</div>
                <template v-if="playlistStore.playoutIsRunning">
                    <div class="font-semibold" :class="sync.ok ? 'text-ok' : 'text-accent-text'">
                        {{ sync.ok ? t('onair.inSync') : t('onair.correcting') }}
                    </div>
                    <div class="font-mono text-xs text-muted">{{ sync.value }}</div>
                </template>
                <div v-else class="text-muted">—</div>
            </div>
            <div class="bg-raised rounded-[10px] px-3 py-2.5 min-w-0">
                <div class="text-muted text-xs">{{ t('onair.nextEvent') }}</div>
                <template v-if="nextEvent">
                    <div class="font-semibold line-clamp-2 break-words">{{ nextEvent.event.title }}</div>
                    <div class="font-mono text-xs text-muted">
                        {{ splitDateTime(nextEvent.start).time.slice(0, 5) }} · {{ eventMode(nextEvent) }}
                    </div>
                </template>
                <RouterLink v-else to="/schedule" class="link-accent text-[13px]">{{ t('onair.noEvent') }}</RouterLink>
            </div>
        </div>

        <div class="flex flex-col gap-1.5">
            <div class="flex justify-between text-muted text-xs gap-2">
                <span>{{ t('onair.audio') }}</span>
                <span class="font-mono truncate">{{ loudnessLabel }}</span>
            </div>
            <div class="grid grid-cols-[34px_minmax(0,1fr)] gap-1.5 items-center font-mono text-[11px] text-muted">
                <template v-for="meter in meters" :key="meter.label">
                    <span>{{ meter.label }}</span>
                    <div class="h-2 rounded bg-base-300 overflow-hidden flex">
                        <div class="bg-ok" :style="{ width: `${Math.min(meter.value, 85)}%` }" />
                        <div
                            class="bg-[#e0b43c]"
                            :style="{ width: `${Math.max(0, Math.min(meter.value, 95) - 85)}%` }"
                        />
                        <div class="bg-[#ff5a5f]" :style="{ width: `${Math.max(0, meter.value - 95)}%` }" />
                    </div>
                </template>
            </div>
        </div>
    </section>
</template>
