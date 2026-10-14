<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { throttle } from 'es-toolkit/function'

import { authFetch } from '@/composables/authFetch'
import { useAuth } from '@/stores/auth'
import { useConfig } from '@/stores/config'
import { useIndex } from '@/stores/index'
import { usePlaylist } from '@/stores/playlist'

const { t } = useI18n()
const authStore = useAuth()
const configStore = useConfig()
const indexStore = useIndex()
const playlistStore = usePlaylist()

/** Controls start locked while the channel is on air, so a stray click cannot stop it. */
const locked = ref(playlistStore.playoutIsRunning)
const volume = ref(Math.round((configStore.playout.audio?.volume ?? 1) * 100))
const busy = ref('')

watch(
    () => playlistStore.playoutIsRunning,
    (running) => (locked.value = running),
)

watch(
    () => configStore.i,
    () => {
        locked.value = playlistStore.playoutIsRunning
        volume.value = Math.round((configStore.playout.audio?.volume ?? 1) * 100)
    },
)

const navigationBlocked = computed(() => locked.value || playlistStore.ingestRuns || !playlistStore.playoutIsRunning)

function channelId() {
    return configStore.channels[configStore.i]?.id
}

async function processCommand(command: 'start' | 'stop' | 'restart') {
    if (locked.value) {
        return
    }

    busy.value = command

    try {
        await authFetch(`/api/control/${channelId()}/process`, {
            method: 'POST',
            headers: { ...configStore.contentType, ...authStore.authHeader },
            body: JSON.stringify({ command }),
        })

        if (command === 'start') {
            playlistStore.scrollToItem = true
        }
    } catch (e: any) {
        indexStore.msgAlert('error', e?.data ?? e, 3)
    } finally {
        busy.value = ''
    }
}

async function playoutCommand(control: 'back' | 'next' | 'reset') {
    if (locked.value) {
        return
    }

    busy.value = control

    try {
        const result = await authFetch<{ operation?: string; reason?: string }>(`/api/control/${channelId()}/playout`, {
            method: 'POST',
            headers: { ...configStore.contentType, ...authStore.authHeader },
            body: JSON.stringify({ control }),
        })

        if (result?.reason === 'live_ingest_active') {
            indexStore.msgAlert('warning', t('control.navigationDisabledLive'), 3)
        }
    } catch (e: any) {
        indexStore.msgAlert('error', e?.data ?? e, 3)
    } finally {
        busy.value = ''
    }
}

const applyVolume = throttle(async () => {
    if (locked.value) {
        return
    }

    const value = Math.min(1.5, Math.max(0, volume.value / 100))
    configStore.playout.audio.volume = value

    try {
        await configStore.applyAudioEffects(value)
    } catch (error) {
        indexStore.msgAlert('error', error instanceof Error ? error.message : String(error), 3)
    }
}, 250)
</script>
<template>
    <section :aria-label="t('onair.controls')" class="card-box p-3 flex flex-wrap items-center gap-2.5">
        <div role="group" :aria-label="t('onair.transport')" class="flex flex-wrap gap-2 flex-auto">
            <button class="transport-btn" :disabled="navigationBlocked || !!busy" @click="playoutCommand('back')">
                <svg
                    width="18"
                    height="18"
                    viewBox="0 0 16 16"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="1.5"
                    aria-hidden="true"
                >
                    <path d="M3.5 3v10M12.5 3.5 6 8l6.5 4.5z" />
                </svg>
                {{ t('onair.previous') }}
            </button>
            <button
                v-if="playlistStore.playoutIsRunning"
                class="transport-btn bg-ok-soft! text-ok! border-ok/50! font-semibold"
                aria-pressed="true"
                :disabled="locked || !!busy"
                @click="processCommand('start')"
            >
                <svg width="18" height="18" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
                    <path d="M4.5 2.5v11l9-5.5z" />
                </svg>
                {{ t('onair.running') }}
            </button>
            <button
                v-else
                class="transport-btn font-semibold"
                :disabled="locked || !!busy"
                @click="processCommand('start')"
            >
                <svg width="18" height="18" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
                    <path d="M4.5 2.5v11l9-5.5z" />
                </svg>
                {{ t('onair.start') }}
            </button>
            <button
                class="transport-btn"
                :disabled="locked || !!busy || !playlistStore.playoutIsRunning"
                @click="processCommand('stop')"
            >
                <svg
                    width="18"
                    height="18"
                    viewBox="0 0 16 16"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="1.5"
                    aria-hidden="true"
                >
                    <rect x="3.5" y="3.5" width="9" height="9" rx="1.5" />
                </svg>
                {{ t('onair.stop') }}
            </button>
            <button class="transport-btn" :disabled="locked || !!busy" @click="processCommand('restart')">
                <svg
                    width="18"
                    height="18"
                    viewBox="0 0 16 16"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="1.5"
                    aria-hidden="true"
                >
                    <path d="M13 8a5 5 0 1 1-1.5-3.6M13 2.5v3h-3" />
                </svg>
                {{ t('onair.restart') }}
            </button>
            <button
                class="transport-btn"
                :disabled="navigationBlocked || !!busy"
                :title="t('onair.syncHint')"
                @click="playoutCommand('reset')"
            >
                <svg
                    width="18"
                    height="18"
                    viewBox="0 0 16 16"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="1.5"
                    aria-hidden="true"
                >
                    <path
                        d="M2.5 8a5.5 5.5 0 0 1 9.5-3.8M13.5 8a5.5 5.5 0 0 1-9.5 3.8M12 1.8v2.6H9.4M4 14.2v-2.6h2.6"
                    />
                </svg>
                {{ t('onair.resync') }}
            </button>
            <button class="transport-btn" :disabled="navigationBlocked || !!busy" @click="playoutCommand('next')">
                {{ t('onair.next') }}
                <svg
                    width="18"
                    height="18"
                    viewBox="0 0 16 16"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="1.5"
                    aria-hidden="true"
                >
                    <path d="M12.5 3v10M3.5 3.5 10 8l-6.5 4.5z" />
                </svg>
            </button>
        </div>
        <div class="flex flex-wrap items-center gap-3.5 px-1.5">
            <label
                class="flex items-center gap-2.5 min-h-11 cursor-pointer"
                :class="locked ? 'text-base-content' : 'text-muted'"
            >
                <i class="bi" :class="locked ? 'bi-lock' : 'bi-unlock'" aria-hidden="true" />
                {{ t('onair.locked') }}
                <input v-model="locked" type="checkbox" role="switch" class="toggle toggle-sm toggle-accent" />
            </label>
            <label class="flex items-center gap-2.5 text-muted">
                {{ t('onair.volume') }}
                <input
                    v-model.number="volume"
                    type="range"
                    min="0"
                    max="150"
                    step="5"
                    class="range range-xs range-accent w-28"
                    :disabled="locked"
                    @input="applyVolume"
                />
                <span class="font-mono text-xs w-10 tabular">{{ volume }}%</span>
            </label>
        </div>
    </section>
</template>
<style scoped>
@reference '@/assets/css/main.css';

.transport-btn {
    @apply inline-flex items-center gap-2.5 min-h-13 px-4 rounded-[10px] bg-raised text-base-content border border-line-strong cursor-pointer;
}

.transport-btn:hover:not(:disabled) {
    @apply bg-base-300;
}

.transport-btn:disabled {
    @apply opacity-45 cursor-not-allowed;
}
</style>
