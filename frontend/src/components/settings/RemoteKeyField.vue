<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import GenericModal from '@/components/utils/GenericModal.vue'
import { authFetch } from '@/composables/authFetch'
import { copyText } from '@/composables/browser'
import { useAuth } from '@/stores/auth'
import { useConfig } from '@/stores/config'
import { useIndex } from '@/stores/index'

/**
 * A remote-control key (the logo's or the SCTE-35 one). The server makes and
 * saves it at once, and it stays the same until it is renewed or turned
 * off here: saving the other settings never changes it.
 */
const props = defineProps<{
    kind: 'logo' | 'scte35'
    label: string
    noKey: string
    hint: string
}>()

const { t } = useI18n()
const authStore = useAuth()
const configStore = useConfig()
const indexStore = useIndex()

const busy = ref(false)
const confirming = ref<'renew' | 'clear' | null>(null)

const key = computed(() =>
    ((props.kind === 'logo' ? configStore.playout.processing?.logo_key : configStore.playout.scte35?.key) ?? '').trim(),
)

/** The key in the settings on screen and in the saved copy, so it is no unsaved change. */
function apply(value: string) {
    for (const config of [configStore.playout, configStore.playoutSaved]) {
        if (props.kind === 'logo' && config.processing) {
            config.processing.logo_key = value
        } else if (props.kind === 'scte35' && config.scte35) {
            config.scte35.key = value
        }
    }
}

async function send(action: 'renew' | 'clear') {
    const id = configStore.channels[configStore.i]?.id
    busy.value = true

    try {
        const response = await authFetch<{ key: string }>(`/api/playout/config/${id}/keys/${props.kind}`, {
            method: 'POST',
            headers: { ...configStore.contentType, ...authStore.authHeader },
            body: JSON.stringify({ action }),
        })
        apply(response.key)
        indexStore.msgAlert('success', t(action === 'renew' ? 'remoteKey.renewed' : 'remoteKey.cleared'), 3)
    } catch (error: any) {
        indexStore.msgAlert('error', error?.data ?? t('remoteKey.failed'), 4)
    } finally {
        busy.value = false
    }
}

function ask(action: 'renew' | 'clear') {
    // A first key replaces nothing: no need to ask.
    if (action === 'renew' && !key.value) {
        send('renew')
    } else {
        confirming.value = action
    }
}

function answer(ok: boolean) {
    const action = confirming.value
    confirming.value = null

    if (ok && action) {
        send(action)
    }
}

async function copy() {
    try {
        await copyText(key.value)
        indexStore.msgAlert('success', t('settings.logo.remote.copied'), 2)
    } catch {
        indexStore.msgAlert('error', t('settings.logo.remote.copyFailed'), 3)
    }
}
</script>
<template>
    <div class="flex flex-col gap-1.5">
        <span class="text-xs text-muted">{{ label }}</span>
        <div class="flex flex-wrap gap-2">
            <input
                :value="key"
                readonly
                class="field-input font-mono text-[13px] flex-[1_1_260px] min-w-0"
                :placeholder="noKey"
                :aria-label="label"
            />
            <button class="btn-quiet" :disabled="busy" @click="ask('renew')">
                <i class="bi bi-key" aria-hidden="true" />{{
                    key ? t('settings.logo.remote.newKey') : t('settings.logo.remote.createKey')
                }}
            </button>
            <button v-if="key" class="btn-quiet" @click="copy">
                <i class="bi bi-clipboard" aria-hidden="true" />{{ t('settings.logo.remote.copy') }}
            </button>
            <button v-if="key" class="btn-quiet" :disabled="busy" @click="ask('clear')">
                {{ t('settings.logo.remote.removeKey') }}
            </button>
        </div>
        <span class="field-hint">{{ hint }}</span>

        <GenericModal
            :show="confirming !== null"
            :title="confirming === 'renew' ? t('remoteKey.renewTitle') : t('remoteKey.clearTitle')"
            :text="confirming === 'renew' ? t('remoteKey.renewText') : t('remoteKey.clearText')"
            :ok-label="confirming === 'renew' ? t('settings.logo.remote.newKey') : t('settings.logo.remote.removeKey')"
            danger
            :modal-action="answer"
        />
    </div>
</template>
