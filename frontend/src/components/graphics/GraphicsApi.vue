<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import LogoRemote from '@/components/settings/LogoRemote.vue'
import SectionCard from '@/components/settings/SectionCard.vue'
import { type ChannelGraphic, type GraphicsTemplate, useGraphicsApi } from '@/composables/graphicsTemplates'
import { copyText } from '@/composables/browser'
import { useConfig } from '@/stores/config'
import { useIndex } from '@/stores/index'

/** External control: layer-model semantics, authenticated examples and legacy keyed links. */
const { t } = useI18n()
const configStore = useConfig()
const indexStore = useIndex()

const channelId = computed(() => configStore.channels[configStore.i]?.id)
const key = computed(
    () => configStore.playout?.processing?.logo_key?.trim() || t('settings.logo.remote.keyPlaceholder'),
)
const scenes = computed(() => configStore.playoutSaved?.processing?.graphics_scenes ?? [])
const api = useGraphicsApi()
const channelModels = ref<ChannelGraphic[]>([])
const templates = ref<GraphicsTemplate[]>([])
const selectedChannel = ref<number>()
const selectedTemporary = ref<number>()
const loading = ref(false)
const loadError = ref(false)
let loadVersion = 0

const roles = computed(() => channelModels.value.filter((model) => model.kind !== 'scene'))
const temporary = computed(() => {
    const assigned = new Set(channelModels.value.map((model) => model.template.id))

    return templates.value.filter((template) => !assigned.has(template.id))
})
const selectedRole = computed(() => roles.value.find((model) => model.template.id === selectedChannel.value))
const selectedTemplate = computed(() => temporary.value.find((template) => template.id === selectedTemporary.value))
const base = computed(() => `${window.location.origin}/api/graphics/${channelId.value}`)

/** Keep JSON and URLs literal when the copied command is run by a POSIX shell. */
function shellQuote(value: string) {
    return "'" + value.replaceAll("'", "'\"'\"'") + "'"
}

/** Examples deliberately use a shell variable, never the operator's browser token. */
function curl(path: string, body?: object) {
    const headers = '-H "Authorization: Bearer $TOKEN"'
    const data =
        body === undefined ? '' : ` -H 'Content-Type: application/json' --data ${shellQuote(JSON.stringify(body))}`

    return `curl --fail-with-body ${headers}${data} ${shellQuote(`${base.value}/${path}`)}`
}

const readExamples = computed(() => [
    { label: t('graphics.api.listTemplates'), command: curl('templates') },
    { label: t('graphics.api.channelState'), command: curl('channel') },
])
const channelExamples = computed(() => {
    const model = selectedRole.value

    if (!model) {
        return []
    }

    return [
        { label: t('graphics.api.readModel'), command: curl(`templates/${model.template.id}`) },
        { label: t('graphics.api.modelOn'), command: curl(`channel/${model.template.id}`, { on: true }) },
        { label: t('graphics.api.modelOff'), command: curl(`channel/${model.template.id}`, { on: false }) },
    ]
})
const permanentExample = computed(() => {
    const model = selectedRole.value

    if (model?.template.always_on == null) {
        return ''
    }

    const url = shellQuote(`${base.value}/templates/${model.template.id}`)

    return [
        `curl --fail-with-body -H "Authorization: Bearer $TOKEN" ${url} > channel-model.json`,
        `jq '.always_on = true' channel-model.json > channel-model-enabled.json`,
        `curl --fail-with-body -X PUT -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' --data-binary @channel-model-enabled.json ${url}`,
    ].join('\n')
})
const temporaryExamples = computed(() => {
    const template = selectedTemplate.value

    if (!template) {
        return []
    }

    const fields = Object.fromEntries(template.fields.map((name) => [name, t('graphics.api.sampleValue')]))

    return [
        { label: t('graphics.api.takeTemplate'), command: curl('take', { template_id: template.id, fields }) },
        { label: t('graphics.api.temporaryState'), command: curl('on-air') },
        { label: t('graphics.api.outTemplates'), command: curl('out', {}) },
    ]
})

async function loadModels() {
    const version = ++loadVersion
    channelModels.value = []
    templates.value = []
    selectedChannel.value = selectedTemporary.value = undefined
    loadError.value = false
    loading.value = false

    if (!channelId.value) {
        return
    }

    loading.value = true

    try {
        const [models, saved] = await Promise.all([api.channel(), api.templates()])

        if (version !== loadVersion) {
            return
        }

        channelModels.value = models
        templates.value = saved
        selectedChannel.value = roles.value[0]?.template.id
        selectedTemporary.value = temporary.value[0]?.id
    } catch {
        if (version === loadVersion) {
            loadError.value = true
        }
    } finally {
        if (version === loadVersion) {
            loading.value = false
        }
    }
}

watch(channelId, loadModels, { immediate: true })

/** A scene's name as written in its address. */
function slug(name: string): string {
    return name
        .trim()
        .toLowerCase()
        .replace(/[^\p{L}\p{N}]+/gu, '-')
        .replace(/^-+|-+$/g, '')
}

function sceneUrl(name: string): string {
    return `${window.location.origin}/scene/${channelId.value}/${encodeURIComponent(slug(name))}?key=${encodeURIComponent(key.value)}`
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
    <div>
        <h1 class="m-0 text-xl font-semibold">{{ t('graphics.sections.api') }}</h1>
        <p class="mt-1 mb-0 text-muted">{{ t('graphics.intro.api') }}</p>
    </div>

    <template v-if="configStore.playout?.processing">
        <SectionCard :title="t('graphics.api.modelsTitle')" :description="t('graphics.api.modelsIntro')">
            <ul class="m-0 ps-5 flex flex-col gap-2 text-sm leading-relaxed">
                <li>{{ t('graphics.api.logoLayers') }}</li>
                <li>{{ t('graphics.api.lshapeLayers') }}</li>
                <li>{{ t('graphics.api.tickerLayers') }}</li>
                <li>{{ t('graphics.api.permanentState') }}</li>
            </ul>
            <p class="m-0 text-sm text-muted">{{ t('graphics.api.legacyModels') }}</p>
            <p class="m-0 text-sm text-muted">{{ t('graphics.api.xmltvSeparate') }}</p>
            <RouterLink :to="{ name: 'graphics-templates', query: { channel: channelId } }" class="text-accent-text">
                {{ t('graphics.onAir.library') }}
            </RouterLink>
        </SectionCard>

        <SectionCard :title="t('graphics.api.authTitle')" :description="t('graphics.api.authIntro')">
            <p class="m-0 text-sm">{{ t('graphics.api.tokenHelp') }}</p>
            <p class="m-0 text-sm text-muted">{{ t('graphics.api.permissions') }}</p>
            <div v-for="example in readExamples" :key="example.label" class="flex flex-col gap-1">
                <h3 class="m-0 text-sm font-medium">{{ example.label }}</h3>
                <div class="flex gap-2 items-start">
                    <pre
                        class="flex-1 min-w-0 m-0 whitespace-pre-wrap break-all font-mono text-xs bg-field border border-line rounded-md px-2.5 py-2"
                        >{{ example.command }}</pre>
                    <button
                        class="btn-icon min-w-8 min-h-8 flex-none"
                        :aria-label="t('settings.logo.remote.copy')"
                        @click="copy(example.command)"
                    >
                        <i class="bi bi-clipboard" aria-hidden="true" />
                    </button>
                </div>
            </div>
            <p v-if="loading" class="m-0 text-sm text-muted">{{ t('graphics.api.loading') }}</p>
            <div v-if="loadError" role="alert" class="flex flex-col gap-2 items-start">
                <p class="m-0 text-sm">{{ t('graphics.templates.loadFailed') }}</p>
                <button class="btn-quiet" @click="loadModels">{{ t('graphics.composer.retry') }}</button>
            </div>
            <template v-if="roles.length">
                <label class="field-label">
                    {{ t('graphics.api.channelModel') }}
                    <select v-model="selectedChannel" class="field-input">
                        <option v-for="model in roles" :key="model.template.id" :value="model.template.id">
                            {{ model.template.name }} · #{{ model.template.id }}
                        </option>
                    </select>
                </label>
                <div v-for="example in channelExamples" :key="example.label" class="flex flex-col gap-1">
                    <h3 class="m-0 text-sm font-medium">{{ example.label }}</h3>
                    <div class="flex gap-2 items-start">
                        <pre
                            class="flex-1 min-w-0 m-0 whitespace-pre-wrap break-all font-mono text-xs bg-field border border-line rounded-md px-2.5 py-2"
                            >{{ example.command }}</pre>
                        <button
                            class="btn-icon min-w-8 min-h-8 flex-none"
                            :aria-label="t('settings.logo.remote.copy')"
                            @click="copy(example.command)"
                        >
                            <i class="bi bi-clipboard" aria-hidden="true" />
                        </button>
                    </div>
                </div>
                <details v-if="permanentExample" class="text-sm">
                    <summary class="cursor-pointer font-medium">{{ t('graphics.api.savePermanent') }}</summary>
                    <p class="text-muted">{{ t('graphics.api.savePermanentHelp') }}</p>
                    <pre
                        class="min-w-0 m-0 whitespace-pre-wrap break-all font-mono text-xs bg-field border border-line rounded-md px-2.5 py-2"
                        >{{ permanentExample }}</pre>
                </details>
            </template>
            <p class="m-0 text-sm text-muted">{{ t('graphics.api.verifyOutput') }}</p>
        </SectionCard>

        <SectionCard :title="t('graphics.api.temporaryTitle')" :description="t('graphics.api.temporaryIntro')">
            <p class="m-0 text-sm text-muted">{{ t('graphics.api.assignedNotTake') }}</p>
            <label v-if="temporary.length" class="field-label">
                {{ t('graphics.api.temporaryModel') }}
                <select v-model="selectedTemporary" class="field-input">
                    <option v-for="template in temporary" :key="template.id" :value="template.id">
                        {{ template.name }} · #{{ template.id }}
                    </option>
                </select>
            </label>
            <p v-else-if="!loading && !loadError" class="m-0 text-sm text-muted">
                {{ t('graphics.api.createTemporary') }}
            </p>
            <div v-for="example in temporaryExamples" :key="example.label" class="flex flex-col gap-1">
                <h3 class="m-0 text-sm font-medium">{{ example.label }}</h3>
                <div class="flex gap-2 items-start">
                    <pre
                        class="flex-1 min-w-0 m-0 whitespace-pre-wrap break-all font-mono text-xs bg-field border border-line rounded-md px-2.5 py-2"
                        >{{ example.command }}</pre>
                    <button
                        class="btn-icon min-w-8 min-h-8 flex-none"
                        :aria-label="t('settings.logo.remote.copy')"
                        @click="copy(example.command)"
                    >
                        <i class="bi bi-clipboard" aria-hidden="true" />
                    </button>
                </div>
            </div>
            <p class="m-0 text-sm text-muted">{{ t('graphics.api.takeResult') }}</p>
        </SectionCard>

        <LogoRemote part="api" />

        <SectionCard :title="t('graphics.api.scenesTitle')" :description="t('graphics.api.scenesIntro')">
            <div v-for="scene in scenes" :key="scene.name" class="flex flex-col gap-1">
                <span class="text-xs text-muted">{{ scene.name }}</span>
                <div class="flex gap-2 items-start">
                    <code
                        class="flex-1 min-w-0 break-all font-mono text-xs bg-field border border-line rounded-md px-2.5 py-1.5"
                        >{{ sceneUrl(scene.name) }}</code
                    >
                    <button
                        class="btn-icon min-w-8 min-h-8 flex-none"
                        :aria-label="t('settings.logo.remote.copy')"
                        @click="copy(sceneUrl(scene.name))"
                    >
                        <i class="bi bi-clipboard" aria-hidden="true" />
                    </button>
                </div>
            </div>
            <p v-if="!scenes.length" class="m-0 text-muted text-sm">{{ t('graphics.scenes.none') }}</p>
        </SectionCard>
    </template>
</template>
