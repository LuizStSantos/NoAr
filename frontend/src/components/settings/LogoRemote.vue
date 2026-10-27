<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import RemoteKeyField from '@/components/settings/RemoteKeyField.vue'
import SectionCard from '@/components/settings/SectionCard.vue'
import StoragePicker from '@/components/schedule/StoragePicker.vue'
import GenericModal from '@/components/utils/GenericModal.vue'
import { authFetch } from '@/composables/authFetch'
import { copyText } from '@/composables/browser'
import { useAuth } from '@/stores/auth'
import { useConfig } from '@/stores/config'
import { useIndex } from '@/stores/index'

/**
 * Hide, show or swap the logo on air, from here or from an encoder with the
 * channel's logo key, and how to set that up.
 */
interface LogoStatus {
    hidden: boolean
    replacement: string | null
}

const { t } = useI18n()
const authStore = useAuth()
const configStore = useConfig()
const indexStore = useIndex()

const status = ref<LogoStatus | null>(null)
const busy = ref(false)
/** What the storage picker is choosing: the logo to swap in now, or the one the examples use. */
const picking = ref<'now' | 'example' | null>(null)
const exampleLogo = ref('')
const showGuide = ref(false)
const example = ref<'curl' | 'vmix' | 'obs' | 'companion'>('curl')

const processing = computed(() => configStore.playout.processing)
const channelId = computed(() => configStore.channels[configStore.i]?.id)
const key = computed(() => processing.value.logo_key?.trim() ?? '')
const keyPlaceholder = computed(() => key.value || t('settings.logo.remote.keyPlaceholder'))

function actionUrl(action: 'hide' | 'show' | 'replace'): string {
    const params = new URLSearchParams({ key: keyPlaceholder.value })

    if (action === 'replace') {
        params.set('logo', exampleLogo.value || processing.value.logo || 'pasta/logo.png')
    }

    return `${window.location.origin}/logo/${channelId.value}/${action}?${params.toString().replace(/%2F/g, '/')}`
}

/** The L-shape is switched with the same key. */
function lshapeUrl(action: 'on' | 'off'): string {
    return `${window.location.origin}/lshape/${channelId.value}/${action}?${new URLSearchParams({ key: keyPlaceholder.value })}`
}

const urls = computed(() => [
    { action: 'hide', url: actionUrl('hide') },
    { action: 'show', url: actionUrl('show') },
    { action: 'replace', url: actionUrl('replace') },
    { action: 'lshapeOn', url: lshapeUrl('on') },
    { action: 'lshapeOff', url: lshapeUrl('off') },
])

const snippet = computed(() => {
    const hide = actionUrl('hide')
    const show = actionUrl('show')

    switch (example.value) {
        case 'vmix':
            return [
                `' ${t('settings.logo.remote.vmixHide')}`,
                'Dim web As New System.Net.WebClient()',
                `web.DownloadString("${hide}")`,
                '',
                `' ${t('settings.logo.remote.vmixShow')}`,
                'Dim web As New System.Net.WebClient()',
                `web.DownloadString("${show}")`,
            ].join('\n')
        case 'obs':
        case 'companion':
            return `GET ${hide}\nGET ${show}`
        default:
            return `curl -X POST "${hide}"\ncurl -X POST "${show}"`
    }
})

const stateText = computed(() => {
    if (!status.value) {
        return ''
    }

    if (status.value.hidden) {
        return t('settings.logo.remote.stateHidden')
    }

    return status.value.replacement
        ? t('settings.logo.remote.stateReplaced', { logo: status.value.replacement })
        : t('settings.logo.remote.stateNormal')
})

function headers() {
    return { ...configStore.contentType, ...authStore.authHeader }
}

async function load() {
    if (!channelId.value) {
        return
    }

    try {
        status.value = await authFetch<LogoStatus>(`/api/control/${channelId.value}/logo`, { headers: headers() })
    } catch {
        status.value = null
    }
}

async function send(action: 'hide' | 'show' | 'replace', logo?: string) {
    busy.value = true

    try {
        status.value = await authFetch<LogoStatus>(`/api/control/${channelId.value}/logo`, {
            method: 'POST',
            headers: headers(),
            body: JSON.stringify({ action, logo }),
        })
    } catch (error) {
        indexStore.msgAlert('error', error instanceof Error ? error.message : String(error), 4)
    } finally {
        busy.value = false
    }
}

function pick(file: { path: string }) {
    if (picking.value === 'now') {
        send('replace', file.path)
    } else {
        exampleLogo.value = file.path
    }

    picking.value = null
}

async function copy(text: string) {
    try {
        await copyText(text)
        indexStore.msgAlert('success', t('settings.logo.remote.copied'), 2)
    } catch {
        indexStore.msgAlert('error', t('settings.logo.remote.copyFailed'), 3)
    }
}

onMounted(load)
watch(channelId, load)
</script>
<template>
    <SectionCard :title="t('settings.logo.remote.title')" :description="t('settings.logo.remote.intro')">
        <div class="flex flex-wrap items-center gap-2.5">
            <span class="text-[13px] me-2" role="status">
                <span class="text-muted">{{ t('settings.logo.remote.now') }}</span>
                <strong class="font-semibold">{{ stateText || '—' }}</strong>
            </span>
            <button class="btn-quiet" :disabled="busy || status?.hidden" @click="send('hide')">
                <i class="bi bi-eye-slash" aria-hidden="true" />{{ t('settings.logo.remote.hide') }}
            </button>
            <button
                class="btn-quiet"
                :disabled="busy || (status !== null && !status.hidden && !status.replacement)"
                @click="send('show')"
            >
                <i class="bi bi-eye" aria-hidden="true" />{{ t('settings.logo.remote.show') }}
            </button>
            <button class="btn-quiet" :disabled="busy" @click="picking = 'now'">
                <i class="bi bi-arrow-left-right" aria-hidden="true" />{{ t('settings.logo.remote.replace') }}
            </button>
        </div>

        <RemoteKeyField
            kind="logo"
            :label="t('settings.logo.remote.key')"
            :no-key="t('settings.logo.remote.noKey')"
            :hint="t('settings.logo.remote.keyHint')"
        />

        <button
            class="flex items-center gap-2 bg-transparent border-0 p-0 min-h-8 cursor-pointer text-accent-text text-left font-medium"
            :aria-expanded="showGuide"
            @click="showGuide = !showGuide"
        >
            <i class="bi text-xs" :class="showGuide ? 'bi-chevron-down' : 'bi-chevron-right'" aria-hidden="true" />
            {{ t('settings.logo.remote.howTo') }}
        </button>

        <ol v-if="showGuide" class="m-0 ps-5 flex flex-col gap-4 text-[13px] leading-relaxed">
            <li>
                {{ t('settings.logo.remote.step1') }}
            </li>
            <li class="flex flex-col gap-2">
                <span>{{ t('settings.logo.remote.step2') }}</span>
                <div v-for="item in urls" :key="item.action" class="flex flex-col gap-1">
                    <span class="text-xs text-muted">{{ t(`settings.logo.remote.url.${item.action}`) }}</span>
                    <div class="flex gap-2 items-start">
                        <code
                            class="flex-1 min-w-0 break-all font-mono text-xs bg-field border border-line rounded-md px-2.5 py-1.5"
                            >{{ item.url }}</code
                        >
                        <button
                            class="btn-icon min-w-8 min-h-8 flex-none"
                            :aria-label="t('settings.logo.remote.copy')"
                            @click="copy(item.url)"
                        >
                            <i class="bi bi-clipboard" aria-hidden="true" />
                        </button>
                    </div>
                </div>
                <span class="field-hint">
                    {{ t('settings.logo.remote.replaceHint') }}
                    <button
                        class="bg-transparent border-0 p-0 text-accent-text cursor-pointer hover:underline"
                        @click="picking = 'example'"
                    >
                        {{ t('settings.logo.remote.chooseExample') }}
                    </button>
                </span>
            </li>
            <li class="flex flex-col gap-2">
                <span>{{ t('settings.logo.remote.step3') }}</span>
                <div role="tablist" class="flex flex-wrap gap-1.5">
                    <button
                        v-for="tool in ['curl', 'vmix', 'obs', 'companion'] as const"
                        :key="tool"
                        role="tab"
                        :aria-selected="example === tool"
                        class="px-3 min-h-8 rounded-md border text-xs cursor-pointer"
                        :class="
                            example === tool
                                ? 'bg-accent/15 border-accent text-accent-text'
                                : 'bg-field border-line-strong text-muted hover:text-base-content'
                        "
                        @click="example = tool"
                    >
                        {{ t(`settings.logo.remote.tool.${tool}`) }}
                    </button>
                </div>
                <p class="m-0 text-muted">{{ t(`settings.logo.remote.toolHelp.${example}`) }}</p>
                <div class="flex gap-2 items-start">
                    <pre
                        class="flex-1 min-w-0 m-0 whitespace-pre-wrap break-all font-mono text-xs bg-field border border-line rounded-md px-2.5 py-2"
                        >{{ snippet }}</pre>
                    <button
                        class="btn-icon min-w-8 min-h-8 flex-none"
                        :aria-label="t('settings.logo.remote.copy')"
                        @click="copy(snippet)"
                    >
                        <i class="bi bi-clipboard" aria-hidden="true" />
                    </button>
                </div>
            </li>
            <li>{{ t('settings.logo.remote.step4') }}</li>
        </ol>

        <GenericModal
            :show="picking !== null"
            :title="t('settings.choose')"
            hide-buttons
            :modal-action="() => (picking = null)"
        >
            <div class="w-[min(520px,90vw)] max-h-[60vh] flex flex-col">
                <StoragePicker @pick="pick" />
            </div>
        </GenericModal>
    </SectionCard>
</template>
