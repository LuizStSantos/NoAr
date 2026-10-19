<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import RestartDialog from '@/components/settings/RestartDialog.vue'
import { authFetch } from '@/composables/authFetch'
import { uptimeLabel } from '@/composables/player'
import { useConfig } from '@/stores/config'
import { useIndex } from '@/stores/index'
import { usePlaylist } from '@/stores/playlist'
import { useSettings } from '@/stores/settings'

/** The selected channel's playout, with a restart that is always at hand. */
const { t } = useI18n()
const configStore = useConfig()
const indexStore = useIndex()
const playlistStore = usePlaylist()
const settings = useSettings()

const asking = ref(false)
const busy = ref(false)

const channel = computed(() => configStore.channels[configStore.i])
const status = computed(() => {
    if (!playlistStore.playoutIsRunning) {
        return t('settings.restart.stopped')
    }

    return playlistStore.uptime !== null
        ? t('settings.restart.running', { time: uptimeLabel(playlistStore.uptime) })
        : t('settings.restart.onAir')
})

async function send(command: 'start' | 'restart') {
    busy.value = true

    try {
        await authFetch(`/api/control/${channel.value?.id}/process`, {
            method: 'POST',
            headers: configStore.contentType,
            body: JSON.stringify({ command }),
        })
        settings.restartPending = false
        indexStore.msgAlert(
            'success',
            command === 'start' ? t('settings.restart.started') : t('settings.restart.restarted'),
            3,
        )
    } catch (e: any) {
        indexStore.msgAlert('error', e?.data ?? e?.message ?? String(e), 5)
    } finally {
        busy.value = false
        asking.value = false
    }
}
</script>
<template>
    <section
        :aria-label="t('settings.restart.playout', { name: channel?.name ?? '' })"
        class="card-box px-3.5 py-3 flex flex-col gap-2.5"
    >
        <h2 class="m-0 text-[13px] font-semibold">
            {{ t('settings.restart.playout', { name: channel?.name ?? '' }) }}
        </h2>
        <span class="inline-flex items-center gap-2 text-[13px] text-base-content/85">
            <span class="dot" :class="playlistStore.playoutIsRunning ? 'bg-ok' : 'bg-subtle'" aria-hidden="true" />{{
                status
            }}
        </span>
        <button
            v-if="playlistStore.playoutIsRunning"
            type="button"
            class="btn-quiet"
            :disabled="busy"
            @click="asking = true"
        >
            <i class="bi bi-arrow-repeat" aria-hidden="true" />{{ t('settings.restartNow') }}
        </button>
        <button v-else type="button" class="btn-quiet" :disabled="busy" @click="send('start')">
            <span v-if="busy" class="loading loading-spinner loading-xs" />
            <i v-else class="bi bi-play-fill" aria-hidden="true" />{{ t('settings.restart.start') }}
        </button>
    </section>

    <RestartDialog
        :show="asking"
        :busy="busy"
        :title="t('settings.restart.playoutTitle', { name: channel?.name ?? '' })"
        :text="t('settings.restart.playoutText')"
        :hint="t('settings.restart.playoutHint')"
        @close="asking = false"
        @confirm="send('restart')"
    />
</template>
