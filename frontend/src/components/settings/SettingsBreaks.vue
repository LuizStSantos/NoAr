<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import SaveBar from '@/components/settings/SaveBar.vue'
import Scte35Remote from '@/components/settings/Scte35Remote.vue'
import SectionCard from '@/components/settings/SectionCard.vue'
import UnitInput from '@/components/settings/UnitInput.vue'
import { usePlayoutPage } from '@/composables/settingsPage'
import { SCTE35_COPIES, clockLength, secondsUntil, useScte35, type BreakStatus } from '@/composables/scte35'
import { useIndex } from '@/stores/index'

/**
 * SCTE-35 ad breaks: whether NoAr marks them, how it does, and what it is
 * doing right now.
 */
const { t, locale } = useI18n()
const indexStore = useIndex()
const { ready, changes, alsoSaves, saveLabel, save, discard, settings, configStore } = usePlayoutPage('breaks')

const scte35 = computed(() => configStore.playout.scte35)
const output = computed(() => configStore.playout.output)
const channelId = computed(() => configStore.channels[configStore.i]?.id)
const { status, now, busy, insertBreak, cancelBreak } = useScte35(channelId)

const manualDuration = ref(120)

/** SCTE-35 is carried by MPEG-TS over SRT/UDP and native HLS. */
const outputCarries = computed(
    () => output.value.mode === 'hls'
        || (output.value.mode === 'stream' && ['srt', 'udp'].includes(output.value.stream_type)),
)

const state = computed(() => {
    if (!scte35.value.enable) {
        return { tone: 'off', text: t('scte35.state.off') }
    }

    if (status.value?.error) {
        return { tone: 'error', text: status.value.error }
    }

    if (status.value?.active) {
        return { tone: 'on', text: t('scte35.state.on', { pid: status.value.pid }) }
    }

    return { tone: 'waiting', text: t('scte35.waiting') }
})

const clock = computed(
    () =>
        new Intl.DateTimeFormat(locale.value, {
            hour: '2-digit',
            minute: '2-digit',
            second: '2-digit',
            timeZone: configStore.timezone || undefined,
        }),
)

const rows = computed(() => [...(status.value?.breaks ?? []), ...(status.value?.recent ?? [])])

function keyFrame(value: boolean | null): string {
    if (value === null) {
        return '—'
    }

    return value ? '✓' : '✗'
}

function cancellable(item: BreakStatus): boolean {
    return item.state === 'scheduled' || item.state === 'announced'
}

async function signal() {
    try {
        await insertBreak(manualDuration.value)
        indexStore.msgAlert('success', t('scte35.signalled', { seconds: manualDuration.value }), 3)
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
    <template v-if="ready">
        <div class="flex flex-wrap items-end justify-between gap-3">
            <div>
                <h1 class="m-0 text-xl font-semibold">{{ t('settings.sections.breaks') }}</h1>
                <p class="mt-1 mb-0 text-muted max-w-2xl">{{ t('scte35.intro') }}</p>
            </div>
            <label
                class="flex items-center gap-3 min-h-11 bg-base-100 border border-line rounded-field px-3.5 cursor-pointer"
            >
                {{ t('scte35.enable') }}
                <input v-model="scte35.enable" type="checkbox" role="switch" class="toggle toggle-accent" />
            </label>
        </div>

        <section
            :aria-label="t('scte35.stateLabel')"
            class="card-box px-4.5 py-3.5 flex flex-wrap items-center gap-x-5 gap-y-2 text-[13px]"
        >
            <span
                class="inline-flex items-center gap-2"
                :class="{
                    'font-semibold text-ok': state.tone === 'on',
                    'text-accent-text': state.tone === 'error',
                    'text-muted': state.tone === 'off' || state.tone === 'waiting',
                }"
            >
                <span
                    class="dot"
                    :class="state.tone === 'on' ? 'bg-ok' : state.tone === 'error' ? 'bg-[#ff5a5f]' : 'bg-subtle'"
                    aria-hidden="true"
                />{{ state.text }}
            </span>
            <span v-if="scte35.enable" class="text-muted">
                {{ t('scte35.summary', { preroll: scte35.preroll, min: scte35.min_break, max: scte35.max_break }) }}
            </span>
        </section>

        <p v-if="scte35.enable && !outputCarries" class="m-0 text-[13px] text-accent-text">
            <i class="bi bi-exclamation-triangle me-1" aria-hidden="true" />{{ t('scte35.outputMismatch') }}
        </p>

        <SectionCard :title="t('scte35.how.title')" :description="t('scte35.how.intro')">
            <ol class="m-0 p-0 list-none grid grid-cols-1 md:grid-cols-3 gap-3 text-[13px] leading-relaxed">
                <li class="bg-raised rounded-[10px] px-3.5 py-3 flex flex-col gap-1">
                    <span class="font-mono text-xs text-muted">{{
                        t('scte35.how.beforeTime', { seconds: scte35.preroll })
                    }}</span>
                    <strong class="font-semibold">{{ t('scte35.how.announce') }}</strong>
                    <span class="text-muted">{{ t('scte35.how.announceText', { copies: SCTE35_COPIES }) }}</span>
                </li>
                <li class="bg-raised rounded-[10px] px-3.5 py-3 flex flex-col gap-1">
                    <span class="font-mono text-xs text-muted">{{ t('scte35.how.startTime') }}</span>
                    <strong class="font-semibold">{{ t('scte35.how.start') }}</strong>
                    <span class="text-muted">{{ t('scte35.how.startText') }}</span>
                </li>
                <li class="bg-raised rounded-[10px] px-3.5 py-3 flex flex-col gap-1">
                    <span class="font-mono text-xs text-muted">{{ t('scte35.how.endTime') }}</span>
                    <strong class="font-semibold">{{ t('scte35.how.end') }}</strong>
                    <span class="text-muted">{{ t('scte35.how.endText') }}</span>
                </li>
            </ol>
            <ul class="m-0 ps-5 flex flex-col gap-1.5 text-[13px] leading-relaxed">
                <li>{{ t('scte35.how.which') }}</li>
                <li>{{ t('scte35.how.manual') }}</li>
                <li>{{ t('scte35.how.downstream') }}</li>
            </ul>
        </SectionCard>

        <SectionCard :title="t('scte35.settings')">
            <div class="grid grid-cols-1 sm:grid-cols-2 xl:grid-cols-4 gap-3.5">
                <label class="field-label">
                    {{ t('scte35.preroll') }}
                    <UnitInput v-model.number="scte35.preroll" unit="s" type="number" min="0" max="30" step="0.5" />
                    <span class="field-hint">{{ t('scte35.prerollHint') }}</span>
                </label>
                <label class="field-label">
                    {{ t('scte35.minBreak') }}
                    <UnitInput v-model.number="scte35.min_break" unit="s" type="number" min="1" />
                </label>
                <label class="field-label">
                    {{ t('scte35.maxBreak') }}
                    <UnitInput v-model.number="scte35.max_break" unit="s" type="number" min="1" />
                    <span class="field-hint">{{ t('scte35.lengthHint') }}</span>
                </label>
                <label class="field-label">
                    {{ t('scte35.pid') }}
                    <input
                        v-model.number="scte35.pid"
                        type="number"
                        min="32"
                        max="8190"
                        class="field-input font-mono"
                    />
                    <span class="field-hint">{{ t('scte35.pidHint') }}</span>
                </label>
            </div>
        </SectionCard>

        <SectionCard :title="t('scte35.breaks')" :description="t('scte35.breaksIntro')">
            <div v-if="status?.active && !status.error" class="flex flex-wrap items-end gap-2.5">
                <label class="field-label">
                    {{ t('scte35.manualLength') }}
                    <UnitInput
                        v-model.number="manualDuration"
                        unit="s"
                        type="number"
                        :min="scte35.min_break"
                        :max="scte35.max_break"
                    />
                </label>
                <button class="btn-quiet min-h-11" :disabled="busy" @click="signal">
                    <i class="bi bi-signpost-split" aria-hidden="true" />{{ t('scte35.manual') }}
                </button>
                <span class="field-hint basis-full">{{ t('scte35.manualHint', { seconds: scte35.preroll }) }}</span>
            </div>

            <p v-if="!rows.length" class="m-0 text-[13px] text-muted">{{ t('scte35.noBreaks') }}</p>
            <div v-else class="overflow-x-auto">
                <table class="w-full text-[13px] border-collapse">
                    <thead>
                        <tr class="text-left text-xs text-muted">
                            <th class="font-medium py-1.5 pe-3">{{ t('scte35.col.start') }}</th>
                            <th class="font-medium py-1.5 pe-3">{{ t('scte35.col.length') }}</th>
                            <th class="font-medium py-1.5 pe-3">{{ t('scte35.col.source') }}</th>
                            <th class="font-medium py-1.5 pe-3">{{ t('scte35.col.state') }}</th>
                            <th class="font-medium py-1.5 pe-3" :title="t('scte35.col.messagesHint')">
                                {{ t('scte35.col.messages') }}
                            </th>
                            <th class="font-medium py-1.5 pe-3" :title="t('scte35.col.keyFrameHint')">
                                {{ t('scte35.col.keyFrame') }}
                            </th>
                            <th class="font-medium py-1.5 pe-3">ID</th>
                            <th />
                        </tr>
                    </thead>
                    <tbody>
                        <tr v-for="item in rows" :key="`${item.event_id}-${item.state}`" class="border-t border-line">
                            <td class="py-1.5 pe-3 font-mono tabular whitespace-nowrap">
                                {{ clock.format(new Date(item.starts_at)) }}
                                <span
                                    v-if="item.state === 'scheduled' || item.state === 'announced'"
                                    class="text-muted"
                                >
                                    · {{ t('scte35.inTime', { time: clockLength(secondsUntil(item.starts_at, now)) }) }}
                                </span>
                            </td>
                            <td class="py-1.5 pe-3 font-mono tabular">{{ clockLength(item.duration) }}</td>
                            <td class="py-1.5 pe-3">{{ t(`scte35.source.${item.source}`) }}</td>
                            <td
                                class="py-1.5 pe-3 whitespace-nowrap"
                                :class="{
                                    'font-semibold text-live': item.state === 'on_air',
                                    'text-ok': item.state === 'announced',
                                    'text-muted': item.state === 'ended' || item.state === 'cancelled',
                                }"
                            >
                                {{ t(`scte35.states.${item.state}`) }}
                            </td>
                            <td class="py-1.5 pe-3 font-mono tabular">
                                {{ item.out_messages }} / {{ item.in_messages }}
                            </td>
                            <td class="py-1.5 pe-3 font-mono">
                                {{ keyFrame(item.out_key_frame) }} / {{ keyFrame(item.in_key_frame) }}
                            </td>
                            <td class="py-1.5 pe-3 font-mono text-xs text-muted">{{ item.event_id }}</td>
                            <td class="py-1.5 text-right">
                                <button
                                    v-if="cancellable(item)"
                                    class="btn-quiet min-h-8 text-xs"
                                    :disabled="busy"
                                    @click="cancel(item.event_id)"
                                >
                                    {{ t('scte35.cancel') }}
                                </button>
                            </td>
                        </tr>
                    </tbody>
                </table>
            </div>
        </SectionCard>

        <Scte35Remote />

        <SaveBar
            :changes="changes"
            :also-saves="alsoSaves"
            :saving="settings.saving"
            :save-label="saveLabel"
            @save="save"
            @discard="discard"
        />
    </template>
</template>
