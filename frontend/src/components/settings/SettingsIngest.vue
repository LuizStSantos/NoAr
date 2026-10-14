<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import KeyValueEditor from '@/components/settings/KeyValueEditor.vue'
import SaveBar from '@/components/settings/SaveBar.vue'
import { usePlayoutPage } from '@/composables/settingsPage'
import { usePlaylist } from '@/stores/playlist'
import type { LiveInput } from '@/types/playout_config'

const { t } = useI18n()
const playlistStore = usePlaylist()
const { ready, changes, alsoSaves, saveLabel, save, discard, settings, configStore } = usePlayoutPage('ingest')

const DEFAULT_ADDRESS = {
    srt: 'srt://0.0.0.0:9000',
    rtmp: 'rtmp://0.0.0.0:1936/live/stream',
} as Record<string, string>

const open = ref<Set<number>>(new Set())

const listeners = computed(() => configStore.playout.ingest.listeners)
const channelName = computed(() => configStore.channels[configStore.i]?.name ?? '')

function toggle(index: number) {
    const next = new Set(open.value)

    if (next.has(index)) {
        next.delete(index)
    } else {
        next.add(index)
    }

    open.value = next
}

function addListener() {
    const used = listeners.value.map((listener) => portOf(listener.identifier))
    let port = 9000

    while (used.includes(port)) {
        port += 1
    }

    listeners.value.push({
        id: 0,
        priority: 50,
        enabled: true,
        name: t('settings.ingest.newName', { n: listeners.value.length + 1 }),
        backend: 'srt',
        identifier: `srt://0.0.0.0:${port}`,
        options: {},
        demuxer_options: {},
    })
    toggle(listeners.value.length - 1)
}

function removeListener(index: number) {
    listeners.value.splice(index, 1)
    open.value = new Set()
}

function setBackend(listener: LiveInput, backend: string) {
    if (listener.backend === backend) {
        return
    }

    listener.backend = backend
    listener.identifier = DEFAULT_ADDRESS[backend] ?? ''
    listener.options = {}
    listener.demuxer_options = {}
}

function portOf(address: string): number | null {
    const match = /:(\d+)(?:\/|\?|$)/.exec(address)

    return match ? Number(match[1]) : null
}

/** The address with a stream key hidden. */
function maskedAddress(listener: LiveInput): string {
    if (listener.backend !== 'rtmp') {
        return listener.identifier
    }

    return listener.identifier.replace(/^(rtmps?:\/\/[^/]+\/[^/]+\/).+$/, '$1••••••••')
}

function dockerHint(listener: LiveInput): string {
    const port = portOf(listener.identifier)

    if (!port) {
        return ''
    }

    return listener.backend === 'srt'
        ? t('settings.ingest.dockerUdp', { port })
        : t('settings.ingest.dockerTcp', { port })
}

type ListenerState = 'onAir' | 'waiting' | 'off' | 'stopped'

function stateOf(listener: LiveInput): ListenerState {
    if (!listener.enabled) {
        return 'off'
    }

    if (playlistStore.ingestRuns && listener.id && playlistStore.ingestListenerId === listener.id) {
        return 'onAir'
    }

    return playlistStore.playoutIsRunning ? 'waiting' : 'stopped'
}
</script>
<template>
    <template v-if="ready">
        <div class="flex flex-wrap items-end justify-between gap-3">
            <div>
                <h1 class="m-0 text-xl font-semibold">{{ t('settings.sections.ingest') }}</h1>
                <p class="mt-1 mb-0 text-muted max-w-2xl">{{ t('settings.ingest.intro', { name: channelName }) }}</p>
            </div>
            <button class="btn-main" @click="addListener">+ {{ t('settings.ingest.add') }}</button>
        </div>

        <section
            v-for="(listener, index) in listeners"
            :key="listener.id || `new-${index}`"
            :aria-label="listener.name"
            class="card-box px-4.5 py-3.5 flex flex-col gap-4"
            :class="[
                open.has(index) ? 'border-line-strong!' : '',
                listener.enabled ? '' : 'bg-[color-mix(in_srgb,var(--color-base-100)_85%,var(--color-base-200))]',
            ]"
        >
            <div class="flex flex-wrap items-center gap-x-4.5 gap-y-3">
                <input
                    v-model="listener.enabled"
                    type="checkbox"
                    role="switch"
                    class="toggle toggle-accent"
                    :aria-label="t('settings.ingest.enabled', { name: listener.name })"
                />
                <div class="flex-[1_1_260px] min-w-0">
                    <div class="font-semibold" :class="listener.enabled ? '' : 'text-muted'">
                        {{ listener.name || t('settings.ingest.unnamed') }}
                    </div>
                    <div class="font-mono text-[12.5px] text-muted truncate">{{ maskedAddress(listener) }}</div>
                </div>
                <span
                    class="text-xs border rounded-md px-2 py-0.5"
                    :class="listener.backend === 'srt' ? 'border-live text-live' : 'border-after-clip text-after-clip'"
                >
                    {{ listener.backend.toUpperCase() }}
                </span>
                <span class="text-[13px] text-muted">{{
                    t('settings.ingest.priorityShort', { n: listener.priority })
                }}</span>
                <span
                    v-if="stateOf(listener) === 'onAir'"
                    class="bg-on-air text-white text-xs font-semibold rounded-md px-2 py-1"
                >
                    ● {{ t('settings.ingest.state.onAir') }}
                </span>
                <span v-else class="inline-flex items-center gap-1.5 text-[13px] text-muted">
                    <span class="dot bg-subtle" aria-hidden="true" />{{
                        t(`settings.ingest.state.${stateOf(listener)}`)
                    }}
                </span>
                <button class="btn-quiet" :aria-expanded="open.has(index)" @click="toggle(index)">
                    {{ open.has(index) ? t('settings.close') : t('settings.edit') }}
                </button>
            </div>

            <template v-if="open.has(index)">
                <div class="grid grid-cols-1 md:grid-cols-[minmax(0,2fr)_auto] gap-3.5">
                    <label class="field-label">
                        {{ t('settings.ingest.name') }}
                        <input v-model="listener.name" class="field-input" />
                    </label>
                    <div class="field-label">
                        {{ t('settings.ingest.protocol') }}
                        <div
                            role="radiogroup"
                            :aria-label="t('settings.ingest.protocol')"
                            class="flex border border-line-strong rounded-field overflow-hidden min-h-10"
                        >
                            <label
                                v-for="backend in ['rtmp', 'srt']"
                                :key="backend"
                                class="flex-1 flex items-center justify-center gap-1.5 px-4 cursor-pointer text-[14px]"
                                :class="listener.backend === backend ? 'bg-base-300 text-base-content' : 'text-muted'"
                            >
                                <input
                                    type="radio"
                                    :checked="listener.backend === backend"
                                    class="radio radio-xs radio-accent"
                                    @change="setBackend(listener, backend)"
                                />
                                {{ backend.toUpperCase() }}
                            </label>
                        </div>
                    </div>
                    <label class="field-label md:col-span-2">
                        {{ t('settings.ingest.address') }}
                        <input
                            v-model.trim="listener.identifier"
                            class="field-input font-mono text-[13px]"
                            spellcheck="false"
                        />
                        <span class="field-hint">{{
                            listener.backend === 'srt'
                                ? t('settings.ingest.addressSrt')
                                : t('settings.ingest.addressRtmp')
                        }}</span>
                    </label>
                </div>
                <label class="field-label">
                    {{ t('settings.ingest.priority', { n: listener.priority }) }}
                    <input
                        v-model.number="listener.priority"
                        type="range"
                        min="0"
                        max="100"
                        class="range range-xs range-accent max-w-[420px]"
                    />
                </label>
                <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
                    <KeyValueEditor
                        v-model="listener.options"
                        :label="t('settings.ingest.inputOptions')"
                        :hint="t('config.liveListenerOptionsHelp')"
                    />
                    <KeyValueEditor
                        v-model="listener.demuxer_options"
                        :label="t('settings.ingest.demuxerOptions')"
                        :hint="t('config.liveDemuxerOptionsHelp')"
                    />
                </div>
                <div class="flex flex-wrap items-center gap-2.5 border-t border-line pt-3.5">
                    <button class="btn-quiet border-[#5a2a2d]! text-[#ff9da0]!" @click="removeListener(index)">
                        {{ t('settings.ingest.remove') }}
                    </button>
                    <div class="flex-auto" />
                    <span v-if="dockerHint(listener)" class="text-[13px] text-muted">{{ dockerHint(listener) }}</span>
                </div>
            </template>
        </section>

        <div v-if="!listeners.length" class="card-box px-6 py-10 text-center text-muted">
            {{ t('settings.ingest.empty') }}
        </div>

        <p class="m-0 text-[13px] text-muted">
            {{ t('settings.ingest.scheduledHint') }}
            <RouterLink
                :to="{ path: '/schedule', query: { channel: configStore.channels[configStore.i]?.id } }"
                class="link-accent"
            >
                {{ t('nav.schedule') }}</RouterLink
            >.
        </p>

        <SaveBar
            :changes="changes"
            :also-saves="alsoSaves"
            :saving="settings.saving"
            :save-label="saveLabel"
            :note="t('settings.ingest.restartNote')"
            @save="save"
            @discard="discard"
        />
    </template>
</template>
