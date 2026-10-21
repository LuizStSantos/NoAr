<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import { SCTE35_COPIES, clockLength, secondsUntil, useScte35 } from '@/composables/scte35'
import { useConfig } from '@/stores/config'
import { useIndex } from '@/stores/index'

/**
 * The SCTE-35 ad breaks on the Player: the next one and how far it is, the
 * one on air and when it returns, and a break by hand.
 */
const { t } = useI18n()
const configStore = useConfig()
const indexStore = useIndex()

const channelId = computed(() => configStore.channels[configStore.i]?.id)
const { status, now, busy, onAir, next, insertBreak, cancelBreak } = useScte35(channelId)

const DURATIONS = [30, 60, 90, 120, 180, 240]
const duration = ref(120)

const durations = computed(() => {
    const scte35 = status.value

    return scte35 ? DURATIONS.filter((value) => value >= scte35.min_break && value <= scte35.max_break) : DURATIONS
})

const onAirProgress = computed(() => {
    const item = onAir.value

    if (!item) {
        return 0
    }

    const elapsed = (now.value - item.starts_at) / 1000

    return Math.min(100, Math.max(0, (elapsed / item.duration) * 100))
})

async function signal() {
    try {
        await insertBreak(duration.value)
        indexStore.msgAlert('success', t('scte35.signalled', { seconds: duration.value }), 3)
    } catch (error) {
        indexStore.msgAlert('error', error instanceof Error ? error.message : String(error), 4)
    }
}

async function cancel(eventId: number) {
    try {
        await cancelBreak(eventId)
    } catch (error) {
        indexStore.msgAlert('error', error instanceof Error ? error.message : String(error), 4)
    }
}
</script>
<template>
    <div
        v-if="status?.enabled"
        role="status"
        :aria-label="t('scte35.title')"
        class="rounded-[10px] border px-3.5 py-3 flex flex-col gap-2"
        :class="onAir ? 'border-live bg-live-soft' : 'border-line bg-raised'"
    >
        <div class="flex flex-wrap items-center gap-x-3 gap-y-2">
            <span class="text-xs font-semibold tracking-[0.06em] text-muted">SCTE-35</span>

            <span v-if="status.error" class="text-[13px] text-accent-text">
                <i class="bi bi-exclamation-triangle me-1" aria-hidden="true" />{{ status.error }}
            </span>
            <span v-else-if="!status.active" class="text-[13px] text-muted">{{ t('scte35.waiting') }}</span>
            <span v-else-if="onAir" class="text-[13px] font-semibold">
                {{ t('scte35.onAir') }} ·
                <span class="font-mono tabular">{{
                    t('scte35.returnsIn', { time: clockLength(secondsUntil(onAir.ends_at, now)) })
                }}</span>
            </span>
            <span v-else-if="next" class="text-[13px]">
                {{ t('scte35.nextIn') }}
                <strong class="font-mono tabular">{{ clockLength(secondsUntil(next.starts_at, now)) }}</strong>
                · {{ clockLength(next.duration) }} · {{ t(`scte35.source.${next.source}`) }}
                <span
                    v-if="next.state === 'announced'"
                    class="ms-1 text-xs text-ok"
                    :title="t('scte35.announcedHint', { pid: status.pid })"
                >
                    <i class="bi bi-broadcast" aria-hidden="true" />
                    {{ t('scte35.announced', { sent: next.out_messages, copies: SCTE35_COPIES }) }}
                </span>
            </span>
            <span v-else class="text-[13px] text-muted">{{ t('scte35.noneAhead') }}</span>

            <div v-if="status.active && !status.error" class="ms-auto flex items-center gap-2">
                <button
                    v-if="next && next.state !== 'on_air'"
                    class="btn-quiet min-h-8 text-xs"
                    :disabled="busy"
                    @click="cancel(next.event_id)"
                >
                    {{ t('scte35.cancel') }}
                </button>
                <select
                    v-model.number="duration"
                    class="field-input min-h-8 py-0 text-xs w-auto"
                    :aria-label="t('scte35.manualLength')"
                >
                    <option v-for="value in durations" :key="value" :value="value">{{ clockLength(value) }}</option>
                </select>
                <button
                    class="btn-quiet min-h-8 text-xs"
                    :disabled="busy || Boolean(onAir)"
                    :title="t('scte35.manualHint', { seconds: status.preroll })"
                    @click="signal"
                >
                    <i class="bi bi-signpost-split" aria-hidden="true" />{{ t('scte35.manual') }}
                </button>
            </div>
        </div>
        <div v-if="onAir" class="h-1.5 rounded bg-base-300 overflow-hidden">
            <div
                class="h-full bg-live transition-[width] duration-1000 ease-linear"
                :style="{ width: `${onAirProgress}%` }"
            />
        </div>
    </div>
</template>
