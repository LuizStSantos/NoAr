<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import RemoteKeyField from '@/components/settings/RemoteKeyField.vue'
import SectionCard from '@/components/settings/SectionCard.vue'
import { type GraphicsTemplate, useGraphicsApi } from '@/composables/graphicsTemplates'
import { useGraphicsDisplayNames } from '@/composables/graphicsDisplayNames'
import { copyText } from '@/composables/browser'
import { useConfig } from '@/stores/config'
import { useIndex } from '@/stores/index'

/**
 * External control: two commands for every template, put on air (with an
 * optional time) and take off air, by login token or with the graphics key.
 */
const { t } = useI18n()
const { templateName } = useGraphicsDisplayNames()
const configStore = useConfig()
const indexStore = useIndex()
const api = useGraphicsApi()

const templates = ref<GraphicsTemplate[]>([])
const selectedId = ref<number>()
const seconds = ref('50')
const loadError = ref(false)

const channelId = computed(() => configStore.channels[configStore.i]?.id)
const key = computed(() => configStore.playout?.processing?.logo_key?.trim() || '<KEY>')
const selected = computed(() => templates.value.find((template) => template.id === selectedId.value))

/** A template's name as written in a command (`Comercial L-shape` → `comercial-l-shape`). */
function slug(name: string): string {
    return name
        .trim()
        .toLowerCase()
        .replace(/[^\p{L}\p{N}]+/gu, '-')
        .replace(/^-+|-+$/g, '')
}

/** Keep JSON and URLs literal when the copied command is run by a POSIX shell. */
function shellQuote(value: string) {
    return "'" + value.replaceAll("'", "'\"'\"'") + "'"
}

const duration = computed(() => {
    const value = Number(seconds.value.replace(',', '.'))

    return Number.isFinite(value) && value >= 0 ? value : 0
})

const examples = computed(() => {
    const template = selected.value

    if (!template) {
        return []
    }

    const name = slug(template.name)
    const fields = Object.fromEntries(template.fields.map((field) => [field, t('graphics.api.sampleValue')]))
    const keyed = `${window.location.origin}/graphics/${channelId.value}`
    const authed = `${window.location.origin}/api/graphics/${channelId.value}`
    const query = `key=${key.value}&${new URLSearchParams({ duration: String(duration.value), ...fields })}`
    const takeBody = { template: name, duration: duration.value, ...(template.fields.length ? { fields } : {}) }
    const bearer = `-H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json'`

    return [
        {
            label: t('graphics.api.takeWithKey'),
            command: `curl -X POST ${shellQuote(`${keyed}/take/${name}?${query}`)}`,
        },
        {
            label: t('graphics.api.outWithKey'),
            command: `curl -X POST ${shellQuote(`${keyed}/out/${name}?key=${key.value}`)}`,
        },
        {
            label: t('graphics.api.takeWithToken'),
            command: `curl -X POST ${bearer} --data ${shellQuote(JSON.stringify(takeBody))} ${shellQuote(`${authed}/take`)}`,
        },
        {
            label: t('graphics.api.outWithToken'),
            command: `curl -X POST ${bearer} --data ${shellQuote(JSON.stringify({ template: name }))} ${shellQuote(`${authed}/out`)}`,
        },
        {
            label: t('graphics.api.allOut'),
            command: `curl -X POST ${bearer} --data '{}' ${shellQuote(`${authed}/out`)}`,
        },
        {
            label: t('graphics.api.onAirList'),
            command: `curl -H "Authorization: Bearer $TOKEN" ${shellQuote(`${authed}/on-air`)}`,
        },
    ]
})

async function load() {
    templates.value = []
    loadError.value = false

    if (!channelId.value) {
        return
    }

    try {
        templates.value = await api.templates()
        selectedId.value = templates.value[0]?.id
    } catch {
        loadError.value = true
    }
}

watch(channelId, load, { immediate: true })

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
    <div>
        <h1 class="m-0 text-xl font-semibold">{{ t('graphics.sections.api') }}</h1>
        <p class="mt-1 mb-0 text-muted">{{ t('graphics.api.commandsIntro') }}</p>
    </div>

    <SectionCard :title="t('graphics.api.keyTitle')" :description="t('graphics.api.keyIntro')">
        <RemoteKeyField
            kind="logo"
            :label="t('graphics.api.key')"
            :no-key="t('settings.logo.remote.noKey')"
            :hint="t('graphics.api.keyHint')"
        />
    </SectionCard>

    <SectionCard :title="t('graphics.api.commandsTitle')" :description="t('graphics.api.commandsHint')">
        <p v-if="loadError" role="alert" class="m-0 text-sm text-error">{{ t('graphics.templates.loadFailed') }}</p>
        <p v-else-if="!templates.length" class="m-0 text-sm text-muted">{{ t('graphics.templates.none') }}</p>
        <div v-else class="grid grid-cols-[repeat(auto-fill,minmax(220px,1fr))] gap-3">
            <label class="field-label">
                {{ t('graphics.api.template') }}
                <select v-model="selectedId" class="field-input">
                    <option v-for="template in templates" :key="template.id" :value="template.id">
                        {{ templateName(template) }} · {{ slug(template.name) }}
                    </option>
                </select>
            </label>
            <label class="field-label">
                {{ t('graphics.api.seconds') }}
                <input v-model="seconds" class="field-input font-mono" inputmode="decimal" />
                <span class="text-xs text-muted">{{ t('graphics.templates.durationHint') }}</span>
            </label>
        </div>
        <div v-for="example in examples" :key="example.label" class="flex flex-col gap-1">
            <span class="text-sm font-medium">{{ example.label }}</span>
            <div class="flex items-start gap-2">
                <code class="flex-auto min-w-0 break-all rounded-field bg-base-200 px-3 py-2 text-xs">{{
                    example.command
                }}</code>
                <button type="button" class="btn-quiet min-h-9" @click="copy(example.command)">
                    {{ t('graphics.api.copy') }}
                </button>
            </div>
        </div>
    </SectionCard>
</template>
