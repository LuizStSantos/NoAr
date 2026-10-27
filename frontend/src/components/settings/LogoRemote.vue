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
 * The channel's graphics on air, the logo and the L-shape: changed from here
 * or from an encoder with the channel's graphics key, and how to set that up.
 */
interface LogoStatus {
    hidden: boolean
    replacement: string | null
    /** Where it was moved to (`X:Y`); null is the channel's place. */
    position?: string | null
}

interface LShapeStatus {
    on: boolean
    ready: boolean
    replacement: string | null
}

const CORNERS = ['top-left', 'top-right', 'bottom-left', 'bottom-right'] as const

const { t } = useI18n()
const authStore = useAuth()
const configStore = useConfig()
const indexStore = useIndex()

const status = ref<LogoStatus | null>(null)
const lshape = ref<LShapeStatus | null>(null)
const busy = ref(false)
/** What the storage picker is choosing: an image for now, or one the examples use. */
const picking = ref<'now' | 'example' | 'lshapeNow' | 'lshapeExample' | null>(null)
const exampleLogo = ref('')
const exampleGraphic = ref('')
const showGuide = ref(false)
const example = ref<'curl' | 'vmix' | 'obs' | 'companion'>('curl')

const processing = computed(() => configStore.playout.processing)
const channelId = computed(() => configStore.channels[configStore.i]?.id)
const key = computed(() => processing.value.logo_key?.trim() ?? '')
const keyPlaceholder = computed(() => key.value || t('settings.logo.remote.keyPlaceholder'))

/** An address of the keyed routes, with `params` after the key. */
function keyedUrl(route: 'logo' | 'lshape', action: string, params: Record<string, string> = {}): string {
    const query = new URLSearchParams({ key: keyPlaceholder.value, ...params })

    return `${window.location.origin}/${route}/${channelId.value}/${action}?${query.toString().replace(/%2F/g, '/')}`
}

const graphicExample = computed(() => exampleGraphic.value || 'pasta/lshape-patrocinio.png')

const groups = computed(() => [
    {
        name: 'logo',
        hint: t('settings.logo.remote.logoHint'),
        choose: 'example' as const,
        chooseLabel: t('settings.logo.remote.chooseExample'),
        urls: [
            { action: 'hide', url: keyedUrl('logo', 'hide') },
            { action: 'show', url: keyedUrl('logo', 'show') },
            {
                action: 'replace',
                url: keyedUrl('logo', 'replace', {
                    logo: exampleLogo.value || processing.value.logo || 'pasta/logo.png',
                }),
            },
            { action: 'move', url: keyedUrl('logo', 'move', { position: 'bottom-left' }) },
            { action: 'moveBack', url: keyedUrl('logo', 'move', { position: 'default' }) },
        ],
    },
    {
        name: 'lshape',
        hint: t('settings.logo.remote.lshapeHint'),
        choose: 'lshapeExample' as const,
        chooseLabel: t('settings.logo.remote.chooseGraphic'),
        urls: [
            { action: 'lshapeOn', url: keyedUrl('lshape', 'on') },
            { action: 'lshapeOff', url: keyedUrl('lshape', 'off') },
            { action: 'lshapeOnWith', url: keyedUrl('lshape', 'on', { graphic: graphicExample.value }) },
            { action: 'lshapeReplace', url: keyedUrl('lshape', 'replace', { graphic: graphicExample.value }) },
            { action: 'lshapeReset', url: keyedUrl('lshape', 'reset') },
        ],
    },
])

const snippet = computed(() => {
    const hide = keyedUrl('logo', 'hide')
    const show = keyedUrl('logo', 'show')
    const lshapeOn = keyedUrl('lshape', 'on')
    const lshapeOff = keyedUrl('lshape', 'off')

    switch (example.value) {
        case 'vmix':
            return [
                [t('settings.logo.remote.vmixHide'), hide],
                [t('settings.logo.remote.vmixShow'), show],
                [t('settings.logo.remote.vmixLshapeOn'), lshapeOn],
                [t('settings.logo.remote.vmixLshapeOff'), lshapeOff],
            ]
                .map(([note, url]) =>
                    [`' ${note}`, 'Dim web As New System.Net.WebClient()', `web.DownloadString("${url}")`].join('\n'),
                )
                .join('\n\n')
        case 'obs':
        case 'companion':
            return [hide, show, lshapeOn, lshapeOff].map((url) => `GET ${url}`).join('\n')
        default:
            return [hide, show, lshapeOn, lshapeOff].map((url) => `curl -X POST "${url}"`).join('\n')
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

const lshapeText = computed(() => {
    if (!lshape.value) {
        return ''
    }

    const state = lshape.value.on ? t('settings.logo.remote.lshapeStateOn') : t('settings.logo.remote.lshapeStateOff')

    return lshape.value.replacement
        ? `${state} · ${t('settings.logo.remote.lshapeStateReplaced', { graphic: lshape.value.replacement })}`
        : state
})

function headers() {
    return { ...configStore.contentType, ...authStore.authHeader }
}

async function load() {
    if (!channelId.value) {
        return
    }

    const [logo, shape] = await Promise.allSettled([
        authFetch<LogoStatus>(`/api/control/${channelId.value}/logo`, { headers: headers() }),
        authFetch<LShapeStatus>(`/api/control/${channelId.value}/lshape`, { headers: headers() }),
    ])

    status.value = logo.status === 'fulfilled' ? logo.value : null
    lshape.value = shape.status === 'fulfilled' ? shape.value : null
}

async function send(action: 'hide' | 'show' | 'replace' | 'move', logo?: string, position?: string) {
    busy.value = true

    try {
        status.value = await authFetch<LogoStatus>(`/api/control/${channelId.value}/logo`, {
            method: 'POST',
            headers: headers(),
            body: JSON.stringify({ action, logo, position }),
        })
    } catch (error) {
        indexStore.msgAlert('error', error instanceof Error ? error.message : String(error), 4)
    } finally {
        busy.value = false
    }
}

async function sendLShape(action: 'on' | 'off' | 'replace' | 'reset', graphic?: string) {
    busy.value = true

    try {
        lshape.value = await authFetch<LShapeStatus>(`/api/control/${channelId.value}/lshape`, {
            method: 'POST',
            headers: headers(),
            body: JSON.stringify({ action, graphic }),
        })
    } catch (error: any) {
        indexStore.msgAlert('error', error?.data ?? t('settings.logo.remote.lshapeFailed'), 4)
    } finally {
        busy.value = false
    }
}

function pick(file: { path: string }) {
    switch (picking.value) {
        case 'now':
            send('replace', file.path)
            break
        case 'lshapeNow':
            sendLShape('replace', file.path)
            break
        case 'lshapeExample':
            exampleGraphic.value = file.path
            break
        default:
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
// A save loads the channel's graphic on air.
watch(() => configStore.playoutSaved.processing?.lshape, load)
</script>
<template>
    <SectionCard :title="t('settings.logo.remote.title')" :description="t('settings.logo.remote.intro')">
        <div class="flex flex-col gap-3">
            <!-- The logo, now. -->
            <div class="flex flex-wrap items-center gap-2.5">
                <span class="w-20 flex-none text-[13px] font-semibold">{{ t('settings.logo.remote.logo') }}</span>
                <span class="text-[13px] me-2 min-w-36" role="status">
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
                <label class="inline-flex items-center gap-2 text-[13px]">
                    <span class="text-muted">{{ t('settings.logo.remote.moveTo') }}</span>
                    <select
                        class="field-input min-h-9 w-auto"
                        :disabled="busy"
                        :value="status?.position ? 'custom' : 'default'"
                        @change="send('move', undefined, ($event.target as HTMLSelectElement).value)"
                    >
                        <option value="default">{{ t('settings.logo.remote.moveDefault') }}</option>
                        <option v-for="corner in CORNERS" :key="corner" :value="corner">
                            {{ t(`lshape.corners.${corner}`) }}
                        </option>
                        <option v-if="status?.position" value="custom" disabled>{{ status.position }}</option>
                    </select>
                </label>
            </div>

            <!-- The L-shape, now. -->
            <div class="flex flex-wrap items-center gap-2.5 border-t border-line pt-3">
                <span class="w-20 flex-none text-[13px] font-semibold">{{ t('settings.logo.remote.lshape') }}</span>
                <span class="text-[13px] me-2 min-w-36" role="status">
                    <span class="text-muted">{{ t('settings.logo.remote.now') }}</span>
                    <strong class="font-semibold" :class="lshape?.on ? 'text-accent-text' : ''">{{
                        lshapeText || '—'
                    }}</strong>
                </span>
                <button class="btn-quiet" :disabled="busy || !lshape?.ready || lshape?.on" @click="sendLShape('on')">
                    <i class="bi bi-aspect-ratio" aria-hidden="true" />{{ t('settings.logo.remote.lshapeOn') }}
                </button>
                <button class="btn-quiet" :disabled="busy || !lshape?.on" @click="sendLShape('off')">
                    <i class="bi bi-fullscreen" aria-hidden="true" />{{ t('settings.logo.remote.lshapeOff') }}
                </button>
                <button class="btn-quiet" :disabled="busy" @click="picking = 'lshapeNow'">
                    <i class="bi bi-arrow-left-right" aria-hidden="true" />{{ t('settings.logo.remote.lshapeSwap') }}
                </button>
                <button v-if="lshape?.replacement" class="btn-quiet" :disabled="busy" @click="sendLShape('reset')">
                    <i class="bi bi-arrow-counterclockwise" aria-hidden="true" />{{
                        t('settings.logo.remote.lshapeReset')
                    }}
                </button>
                <span v-if="lshape && !lshape.ready" class="basis-full field-hint">{{
                    t('settings.logo.remote.lshapeNotReady')
                }}</span>
            </div>
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
            <li class="flex flex-col gap-3">
                <span>{{ t('settings.logo.remote.step2') }}</span>
                <section v-for="group in groups" :key="group.name" class="flex flex-col gap-2">
                    <h3 class="m-0 text-[13px] font-semibold flex items-center gap-1.5">
                        <i
                            class="bi"
                            :class="group.name === 'logo' ? 'bi-badge-tm' : 'bi-aspect-ratio'"
                            aria-hidden="true"
                        />{{ t(`settings.logo.remote.${group.name}`) }}
                    </h3>
                    <div v-for="item in group.urls" :key="item.action" class="flex flex-col gap-1">
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
                        {{ group.hint }}
                        <button
                            class="bg-transparent border-0 p-0 text-accent-text cursor-pointer hover:underline"
                            @click="picking = group.choose"
                        >
                            {{ group.chooseLabel }}
                        </button>
                    </span>
                </section>
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
