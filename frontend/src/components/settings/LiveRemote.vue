<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import RemoteKeyField from '@/components/settings/RemoteKeyField.vue'
import SectionCard from '@/components/settings/SectionCard.vue'
import { copyText } from '@/composables/browser'
import { useConfig } from '@/stores/config'
import { useIndex } from '@/stores/index'

/**
 * Extending or ending the live on air from an encoder, a Stream Deck or a
 * script with the channel's live key, and how to set that up, like the
 * SCTE-35 key.
 */
const props = withDefaults(defineProps<{ open?: boolean }>(), { open: false })

const { t } = useI18n()
const configStore = useConfig()
const indexStore = useIndex()

const showGuide = ref(props.open)
const example = ref<'curl' | 'vmix' | 'companion' | 'api'>('curl')
/** Minutes and end time the examples use. */
const minutes = ref(30)
const until = ref('13:30')

const channelId = computed(() => configStore.channels[configStore.i]?.id)
const key = computed(() => configStore.playout?.processing?.live_key?.trim() ?? '')
const keyPlaceholder = computed(() => key.value || t('schedule.liveRemote.keyPlaceholder'))

type Action = 'minutes' | 'until' | 'end'

function actionUrl(action: Action): string {
    // Written by hand so "until" keeps its colon readable (13:30).
    let query = `key=${encodeURIComponent(keyPlaceholder.value)}`

    if (action === 'minutes') {
        query += `&minutes=${minutes.value || 30}`
    } else if (action === 'until') {
        query += `&until=${encodeURIComponent(until.value || '13:30').replace('%3A', ':')}`
    }

    const path = action === 'end' ? 'end' : 'extend'

    return `${window.location.origin}/live/${channelId.value}/${path}?${query}`
}

const urls = computed(() =>
    (['minutes', 'until', 'end'] as const).map((action) => ({ action, url: actionUrl(action) })),
)

const snippet = computed(() => {
    const extend = actionUrl('minutes')
    const end = actionUrl('end')
    const api = `${window.location.origin}/api/schedule/${channelId.value}`

    switch (example.value) {
        case 'vmix':
            return [
                `' ${t('schedule.liveRemote.vmixExtend', { minutes: minutes.value || 30 })}`,
                'Dim web As New System.Net.WebClient()',
                `web.UploadString("${extend}", "")`,
            ].join('\n')
        case 'companion':
            return `POST ${extend}\nPOST ${end}`
        case 'api':
            return [
                `curl -X POST ${api}/extend \\`,
                "  -H 'Authorization: Bearer <TOKEN>' -H 'Content-Type: application/json' \\",
                `  -d '{"minutes": ${minutes.value || 30}}'`,
                '',
                `curl -X POST ${api}/extend \\`,
                "  -H 'Authorization: Bearer <TOKEN>' -H 'Content-Type: application/json' \\",
                `  -d '{"until": "${until.value || '13:30'}"}'`,
                '',
                `curl -X POST ${api}/stop -H 'Authorization: Bearer <TOKEN>'`,
            ].join('\n')
        default:
            return [
                `curl -X POST "${extend}"`,
                `curl -X POST "${actionUrl('until')}"`,
                `curl -X POST "${end}"`,
                '',
                `# ${t('schedule.liveRemote.headerComment')}`,
                `curl -X POST -H 'X-Live-Key: ${keyPlaceholder.value}' '${window.location.origin}/live/${channelId.value}/end'`,
            ].join('\n')
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
    <SectionCard :title="t('schedule.liveRemote.title')" :description="t('schedule.liveRemote.intro')">
        <RemoteKeyField
            kind="live"
            :label="t('schedule.liveRemote.key')"
            :no-key="t('schedule.liveRemote.noKey')"
            :hint="t('schedule.liveRemote.keyHint')"
        />

        <button
            class="flex items-center gap-2 bg-transparent border-0 p-0 min-h-8 cursor-pointer text-accent-text text-left font-medium"
            :aria-expanded="showGuide"
            @click="showGuide = !showGuide"
        >
            <i class="bi text-xs" :class="showGuide ? 'bi-chevron-down' : 'bi-chevron-right'" aria-hidden="true" />
            {{ t('schedule.liveRemote.howTo') }}
        </button>

        <ol v-if="showGuide" class="m-0 ps-5 flex flex-col gap-4 text-[13px] leading-relaxed">
            <li>{{ t('schedule.liveRemote.step1') }}</li>
            <li class="flex flex-col gap-2">
                <span>{{ t('schedule.liveRemote.step2') }}</span>
                <div class="flex flex-wrap gap-x-5 gap-y-2">
                    <label class="flex items-center gap-2 text-xs text-muted">
                        {{ t('schedule.liveRemote.exampleMinutes') }}
                        <input
                            v-model.number="minutes"
                            type="number"
                            min="1"
                            class="field-input font-mono text-[13px] w-20 min-h-8"
                        />
                        min
                    </label>
                    <label class="flex items-center gap-2 text-xs text-muted">
                        {{ t('schedule.liveRemote.exampleUntil') }}
                        <input v-model="until" type="time" class="field-input font-mono text-[13px] w-36 min-h-8" />
                    </label>
                </div>
                <div v-for="item in urls" :key="item.action" class="flex flex-col gap-1">
                    <span class="text-xs text-muted">{{ t(`schedule.liveRemote.url.${item.action}`) }}</span>
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
                <span class="field-hint">{{ t('schedule.liveRemote.rulesHint') }}</span>
            </li>
            <li class="flex flex-col gap-2">
                <span>{{ t('schedule.liveRemote.step3') }}</span>
                <div role="tablist" class="flex flex-wrap gap-1.5">
                    <button
                        v-for="tool in ['curl', 'vmix', 'companion', 'api'] as const"
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
                        {{ t(`schedule.liveRemote.tool.${tool}`) }}
                    </button>
                </div>
                <p class="m-0 text-muted">{{ t(`schedule.liveRemote.toolHelp.${example}`) }}</p>
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
            <li>{{ t('schedule.liveRemote.step4') }}</li>
        </ol>
    </SectionCard>
</template>
