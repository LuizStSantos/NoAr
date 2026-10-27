<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import RemoteKeyField from '@/components/settings/RemoteKeyField.vue'
import SectionCard from '@/components/settings/SectionCard.vue'
import { copyText } from '@/composables/browser'
import { useConfig } from '@/stores/config'
import { useIndex } from '@/stores/index'

/**
 * Marking SCTE-35 breaks from an encoder, a switcher or an automation with
 * the channel's SCTE-35 key, and how to set that up, like the logo key.
 */
const { t } = useI18n()
const configStore = useConfig()
const indexStore = useIndex()

const showGuide = ref(false)
const example = ref<'curl' | 'vmix' | 'obs' | 'companion' | 'api'>('curl')
/** Break length the examples use, in seconds. */
const duration = ref(120)

const scte35 = computed(() => configStore.playout.scte35)
const channelId = computed(() => configStore.channels[configStore.i]?.id)
const key = computed(() => scte35.value.key?.trim() ?? '')
const keyPlaceholder = computed(() => key.value || t('scte35.remote.keyPlaceholder'))

function actionUrl(action: 'break' | 'cancel'): string {
    const params = new URLSearchParams({ key: keyPlaceholder.value })

    if (action === 'break') {
        params.set('duration', String(duration.value || 120))
    }

    return `${window.location.origin}/scte35/${channelId.value}/${action}?${params.toString()}`
}

const urls = computed(() => [
    { action: 'break', url: actionUrl('break') },
    { action: 'cancel', url: actionUrl('cancel') },
])

const snippet = computed(() => {
    const mark = actionUrl('break')
    const cancel = actionUrl('cancel')

    switch (example.value) {
        case 'vmix':
            return [
                `' ${t('scte35.remote.vmixBreak')}`,
                'Dim web As New System.Net.WebClient()',
                `web.UploadString("${mark}", "")`,
            ].join('\n')
        case 'obs':
        case 'companion':
            return `POST ${mark}\nPOST ${cancel}`
        case 'api':
            return [
                `curl -X POST ${window.location.origin}/api/control/${channelId.value}/scte35/break \\`,
                "  -H 'Authorization: Bearer <TOKEN>' -H 'Content-Type: application/json' \\",
                `  -d '{"duration": ${duration.value || 120}}'`,
                '',
                `curl ${window.location.origin}/api/control/${channelId.value}/scte35 -H 'Authorization: Bearer <TOKEN>'`,
            ].join('\n')
        default:
            return `curl -X POST "${mark}"\ncurl -X POST "${cancel}"`
    }
})

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
    <SectionCard :title="t('scte35.remote.title')" :description="t('scte35.remote.intro')">
        <RemoteKeyField
            kind="scte35"
            :label="t('scte35.remote.key')"
            :no-key="t('scte35.remote.noKey')"
            :hint="t('scte35.remote.keyHint')"
        />

        <button
            class="flex items-center gap-2 bg-transparent border-0 p-0 min-h-8 cursor-pointer text-accent-text text-left font-medium"
            :aria-expanded="showGuide"
            @click="showGuide = !showGuide"
        >
            <i class="bi text-xs" :class="showGuide ? 'bi-chevron-down' : 'bi-chevron-right'" aria-hidden="true" />
            {{ t('scte35.remote.howTo') }}
        </button>

        <ol v-if="showGuide" class="m-0 ps-5 flex flex-col gap-4 text-[13px] leading-relaxed">
            <li>{{ t('scte35.remote.step1') }}</li>
            <li class="flex flex-col gap-2">
                <span>{{ t('scte35.remote.step2', { min: scte35.min_break, max: scte35.max_break }) }}</span>
                <label class="flex items-center gap-2 text-xs text-muted">
                    {{ t('scte35.remote.exampleDuration') }}
                    <input
                        v-model.number="duration"
                        type="number"
                        :min="scte35.min_break"
                        :max="scte35.max_break"
                        class="field-input font-mono text-[13px] w-24 min-h-8"
                    />
                    s
                </label>
                <div v-for="item in urls" :key="item.action" class="flex flex-col gap-1">
                    <span class="text-xs text-muted">{{ t(`scte35.remote.url.${item.action}`) }}</span>
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
                <span class="field-hint">{{ t('scte35.remote.cancelHint') }}</span>
            </li>
            <li class="flex flex-col gap-2">
                <span>{{ t('scte35.remote.step3') }}</span>
                <div role="tablist" class="flex flex-wrap gap-1.5">
                    <button
                        v-for="tool in ['curl', 'vmix', 'obs', 'companion', 'api'] as const"
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
                        {{ t(`scte35.remote.tool.${tool}`) }}
                    </button>
                </div>
                <p class="m-0 text-muted">{{ t(`scte35.remote.toolHelp.${example}`) }}</p>
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
            <li>{{ t('scte35.remote.step4') }}</li>
        </ol>
    </SectionCard>
</template>
