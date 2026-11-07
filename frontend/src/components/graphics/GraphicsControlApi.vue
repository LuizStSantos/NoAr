<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import RemoteKeyField from '@/components/settings/RemoteKeyField.vue'
import SectionCard from '@/components/settings/SectionCard.vue'
import StoragePicker from '@/components/schedule/StoragePicker.vue'
import GenericModal from '@/components/utils/GenericModal.vue'
import { copyText } from '@/composables/browser'
import { useConfig } from '@/stores/config'
import { useIndex } from '@/stores/index'

// Legacy control URLs remain supported examples within the Graphics API page.
// This component neither owns nor changes the channel's on-air model state.
const { t } = useI18n()
const configStore = useConfig()
const indexStore = useIndex()
const picking = ref<'example' | 'lshapeExample' | null>(null)
const exampleLogo = ref('')
const exampleGraphic = ref('')
const showGuide = ref(true)
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
            { action: 'moveShares', url: keyedUrl('logo', 'move', { x: '90', y: '5' }) },
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
            { action: 'lshapeMove', url: keyedUrl('lshape', 'move', { position: 'bottom-left', scale: '70' }) },
            { action: 'lshapeMoveBack', url: keyedUrl('lshape', 'move', { position: 'default' }) },
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

function pick(file: { path: string }) {
    if (picking.value === 'lshapeExample') {
        exampleGraphic.value = file.path
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
</script>
<template>
    <SectionCard :title="t('graphics.api.title')" :description="t('graphics.api.intro')">
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
