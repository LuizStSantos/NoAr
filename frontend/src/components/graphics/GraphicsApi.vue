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
 * External control with the graphics key: put a template on air (with an
 * optional time), take it off, take everything off (except chosen templates)
 * and list what is on air.
 */
/** `embedded`: inside Settings › External control, which has its own heading. */
const props = withDefaults(defineProps<{ embedded?: boolean }>(), { embedded: false })
const { t } = useI18n()
const { templateName } = useGraphicsDisplayNames()
const configStore = useConfig()
const indexStore = useIndex()
const api = useGraphicsApi()

const templates = ref<GraphicsTemplate[]>([])
const selectedId = ref<number>()
const seconds = ref('50')
const loadError = ref(false)
/** Templates that stay on air in the take-everything-off example. */
const keep = ref<number[]>([])

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

/** Search texts: the template the commands name, and the ones that stay on air. */
const templateSearch = ref('')
const keepSearch = ref('')

function matches(template: GraphicsTemplate, text: string): boolean {
    const needle = text.trim().toLocaleLowerCase()

    return (
        !needle ||
        `${templateName(template)} ${template.name} ${slug(template.name)}`.toLocaleLowerCase().includes(needle)
    )
}

const templateMatches = computed(() => templates.value.filter((template) => matches(template, templateSearch.value)))
// A search that hides the chosen template moves the commands to its first match.
watch(templateMatches, (matches) => {
    if (matches.length && !matches.some((template) => template.id === selectedId.value)) {
        selectedId.value = matches[0]!.id
    }
})
const kept = computed(() => templates.value.filter((template) => keep.value.includes(template.id)))
/** Suggestions to keep on air: only while searching, at most eight. */
const keepMatches = computed(() =>
    keepSearch.value.trim()
        ? templates.value
              .filter((template) => !keep.value.includes(template.id) && matches(template, keepSearch.value))
              .slice(0, 8)
        : [],
)

function addKeep(template: GraphicsTemplate) {
    keep.value = [...keep.value, template.id]
    keepSearch.value = ''
}

function removeKeep(id: number) {
    keep.value = keep.value.filter((item) => item !== id)
}

const keyed = computed(() => `${window.location.origin}/graphics/${channelId.value}`)

/** Commands for the chosen template. */
const templateExamples = computed(() => {
    const template = selected.value

    if (!template) {
        return []
    }

    const name = slug(template.name)
    const fields = Object.fromEntries(template.fields.map((field) => [field, t('graphics.api.sampleValue')]))
    const query = `key=${key.value}&${new URLSearchParams({ duration: String(duration.value), ...fields })}`

    return [
        {
            label: t('graphics.api.takeWithKey'),
            command: `curl -X POST ${shellQuote(`${keyed.value}/take/${name}?${query}`)}`,
        },
        {
            label: t('graphics.api.outWithKey'),
            command: `curl -X POST ${shellQuote(`${keyed.value}/out/${name}?key=${key.value}`)}`,
        },
    ]
})

/** Commands for everything on air. */
const allExamples = computed(() => {
    const except = kept.value.map((item) => slug(item.name)).join(',')

    return [
        {
            label: t('graphics.api.allOut'),
            command: `curl -X POST ${shellQuote(`${keyed.value}/out?key=${key.value}`)}`,
        },
        {
            label: t('graphics.api.allOutExcept'),
            command: except
                ? `curl -X POST ${shellQuote(`${keyed.value}/out?key=${key.value}&except=${except}`)}`
                : t('graphics.api.chooseKeep'),
        },
        {
            label: t('graphics.api.onAirList'),
            command: `curl ${shellQuote(`${keyed.value}/on-air?key=${key.value}`)}`,
        },
    ]
})

async function load() {
    templates.value = []
    keep.value = []
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
    <div v-if="!props.embedded">
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

    <p v-if="loadError" role="alert" class="m-0 text-sm text-error">{{ t('graphics.templates.loadFailed') }}</p>
    <p v-else-if="!templates.length" class="m-0 text-sm text-muted">{{ t('graphics.templates.none') }}</p>
    <template v-else>
        <SectionCard :title="t('graphics.api.oneTitle')" :description="t('graphics.api.commandsHint')">
            <div class="grid grid-cols-1 md:grid-cols-[minmax(0,1fr)_200px] gap-3 items-start">
                <div class="flex flex-col gap-2 min-w-0">
                    <input
                        v-model="templateSearch"
                        type="search"
                        class="field-input"
                        :aria-label="t('graphics.api.searchTemplate')"
                        :placeholder="t('graphics.api.searchTemplate')"
                    />
                    <div
                        role="listbox"
                        :aria-label="t('graphics.api.template')"
                        class="max-h-48 overflow-y-auto flex flex-col gap-1 rounded-field border border-line p-1"
                    >
                        <button
                            v-for="template in templateMatches"
                            :key="template.id"
                            type="button"
                            role="option"
                            class="min-h-9 px-2.5 rounded-field text-left flex items-center gap-2 cursor-pointer"
                            :class="
                                selectedId === template.id
                                    ? 'bg-accent/15 text-base-content font-semibold'
                                    : 'hover:bg-raised'
                            "
                            :aria-selected="selectedId === template.id"
                            @click="selectedId = template.id"
                        >
                            <span class="truncate">{{ templateName(template) }}</span>
                            <span class="ml-auto font-mono text-xs text-muted truncate">{{ slug(template.name) }}</span>
                        </button>
                        <p v-if="!templateMatches.length" class="m-0 px-2.5 py-2 text-sm text-muted">
                            {{ t('graphics.onAir.noMatch') }}
                        </p>
                    </div>
                </div>
                <label class="field-label">
                    {{ t('graphics.api.seconds') }}
                    <input v-model="seconds" class="field-input font-mono" inputmode="decimal" />
                    <span class="text-xs text-muted">{{ t('graphics.templates.durationHint') }}</span>
                </label>
            </div>
            <div v-for="example in templateExamples" :key="example.label" class="flex flex-col gap-1">
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

        <SectionCard :title="t('graphics.api.allTitle')" :description="t('graphics.api.allIntro')">
            <div class="flex flex-col gap-2">
                <span class="text-sm font-medium">{{ t('graphics.api.keep') }}</span>
                <div class="flex flex-wrap gap-1.5">
                    <span
                        v-for="template in kept"
                        :key="template.id"
                        class="inline-flex items-center gap-1 pl-2.5 rounded-full border border-accent bg-accent/10 text-sm"
                    >
                        {{ templateName(template) }}
                        <button
                            type="button"
                            class="btn-icon min-h-8 min-w-8"
                            :aria-label="t('graphics.api.removeKeep', { name: templateName(template) })"
                            @click="removeKeep(template.id)"
                        >
                            <i class="bi bi-x-lg text-xs" aria-hidden="true" />
                        </button>
                    </span>
                    <span v-if="!kept.length" class="text-sm text-muted">{{ t('graphics.api.noneKept') }}</span>
                </div>
                <input
                    v-model="keepSearch"
                    type="search"
                    class="field-input md:max-w-96"
                    :aria-label="t('graphics.api.searchKeep')"
                    :placeholder="t('graphics.api.searchKeep')"
                    @keydown.enter.prevent="keepMatches[0] && addKeep(keepMatches[0])"
                />
                <div v-if="keepMatches.length" class="flex flex-wrap gap-1.5">
                    <button
                        v-for="template in keepMatches"
                        :key="template.id"
                        type="button"
                        class="btn-quiet min-h-9"
                        @click="addKeep(template)"
                    >
                        + {{ templateName(template) }}
                        <span class="font-mono text-xs text-muted">{{ slug(template.name) }}</span>
                    </button>
                </div>
                <p class="m-0 text-xs text-muted">{{ t('graphics.api.keepHint') }}</p>
            </div>
            <div v-for="example in allExamples" :key="example.label" class="flex flex-col gap-1">
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
</template>
