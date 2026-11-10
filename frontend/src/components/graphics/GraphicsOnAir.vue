<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute } from 'vue-router'

import { sourceBindings, sourceValues, useGraphicsSources } from '@/composables/graphicsSources'
import LivePreview from '@/components/player/LivePreview.vue'
import TemplatePreview from '@/components/graphics/TemplatePreview.vue'
import { useChannelGraphics } from '@/composables/channelGraphics'
import { AuthFetchError } from '@/composables/authFetch'
import {
    type GraphicOnAir,
    type GraphicsTemplate,
    fieldList,
    onAirSince,
    useGraphicsApi,
} from '@/composables/graphicsTemplates'
import { useGraphicsDisplayNames } from '@/composables/graphicsDisplayNames'
import { useAuth } from '@/stores/auth'
import { useConfig } from '@/stores/config'
import { useIndex } from '@/stores/index'

/**
 * Grafismo › No ar: the operator's screen. A template is picked and filled
 * in on the preview, then put on air; the program monitor shows the real
 * output, and what is on air is listed to take out.
 */
const { t } = useI18n()
const { templateName } = useGraphicsDisplayNames()
const route = useRoute()
const authStore = useAuth()
const configStore = useConfig()
const indexStore = useIndex()
const graphicsApi = useGraphicsApi()
const { sources: dataSources, unavailable: sourcesUnavailable } = useGraphicsSources()
const recordSelection = reactive<Record<number, Record<string, number>>>({})
const isAdmin = ['global_admin', 'channel_admin'].includes(authStore.role)

const templates = ref<GraphicsTemplate[]>([])
const {
    models: channelModels,
    onAir,
    busy: channelBusy,
    unavailable,
    refresh: refreshStatus,
    control: controlModel,
} = useChannelGraphics()
const loaded = ref(false)
const selectedId = ref<number | null>(null)
const values = reactive<Record<string, Record<string, string>>>({})
const duration = ref('')
const busy = ref(false)
const waiting = ref(false)

const channel = computed(() => configStore.channels[configStore.i])
const selected = computed(() => templates.value.find((template) => template.id === selectedId.value))
const selectedValues = computed(() => (selectedId.value === null ? {} : (values[selectedId.value] ?? {})))
const selectedRecords = computed(() => (selectedId.value === null ? {} : (recordSelection[selectedId.value] ?? {})))
const mapped = computed(() => sourceBindings(selected.value))
const mappedSources = computed(() =>
    dataSources.value.filter((source) =>
        Object.values(mapped.value).some((binding) => binding.source_id === source.id),
    ),
)
const mappedValues = computed(() => sourceValues(selected.value, dataSources.value, selectedRecords.value))
const latest = computed(() => [...onAir.value].reverse().find((graphic) => !graphic.leaving))
const selectedModel = computed(() => channelModels.value.find((model) => model.template.id === selectedId.value))
const permanent = computed(() =>
    channelModels.value.filter((model) => ['logo', 'ticker', 'permanent'].includes(model.kind)),
)
const ordinary = computed(() =>
    templates.value.filter((template) => !channelModels.value.some((model) => model.template.id === template.id)),
)
const programName = computed(
    () =>
        (latest.value ? onAirName(latest.value) : '') ||
        channelModels.value
            .filter((model) => model.kind !== 'scene' && model.on && model.ready)
            .map((model) => templateName(model.template))
            .join(' · ') ||
        t('graphics.onAir.noGraphic'),
)

function onAirName(graphic: GraphicOnAir) {
    const template = templates.value.find((template) => template.id === graphic.template_id)

    return template?.name === graphic.name ? templateName(template) : graphic.name
}

function templateHint(template: GraphicsTemplate) {
    const assigned = channelModels.value.find((model) => model.template.id === template.id)

    return assigned
        ? t(`graphics.assigned.hint.${assigned.kind}`)
        : fieldList(template) || t('graphics.templates.noFields')
}

/** The template list's search text and group filter (`null` = every group). */
const search = ref('')
const groupFilter = ref<string | null>(null)
const groupNames = computed(() => [
    ...new Set(templates.value.map((template) => template.group || t('graphics.templates.noGroup'))),
])
const filtered = computed(() => {
    const needle = search.value.trim().toLocaleLowerCase()

    return templates.value.filter(
        (template) =>
            (groupFilter.value === null || (template.group || t('graphics.templates.noGroup')) === groupFilter.value) &&
            (!needle ||
                `${templateName(template)} ${template.name} ${template.fields.join(' ')}`
                    .toLocaleLowerCase()
                    .includes(needle)),
    )
})

const groups = computed(() => {
    const byGroup = new Map<string, GraphicsTemplate[]>()

    for (const template of filtered.value) {
        const group = template.group || t('graphics.templates.noGroup')
        byGroup.set(group, [...(byGroup.get(group) ?? []), template])
    }

    return [...byGroup.entries()].map(([name, items]) => ({ name, items }))
})

function message(error: unknown, fallback: string): string {
    return error instanceof AuthFetchError && typeof error.data === 'string' && error.data ? error.data : fallback
}

async function loadOnAir() {
    await refreshStatus()
}

function select(template: GraphicsTemplate) {
    values[template.id] ??= {}
    recordSelection[template.id] ??= {}
    selectedId.value = template.id
    duration.value = String(template.duration)
}

async function load() {
    try {
        const [models] = await Promise.all([graphicsApi.templates(), refreshStatus()])
        if (unavailable.value) {
            throw new Error(t('graphics.templates.loadFailed'))
        }

        const scenes = new Set(
            channelModels.value.filter((model) => model.kind === 'scene').map((model) => model.template.id),
        )
        templates.value = models.filter((model) => !scenes.has(model.id))
    } catch (error) {
        indexStore.msgAlert('error', message(error, t('graphics.templates.loadFailed')), 4)
    } finally {
        loaded.value = true
    }

    const wanted = Number(route.query.template)
    const first =
        templates.value.find((template) => template.id === wanted) ??
        ordinary.value[0] ??
        templates.value.find((template) =>
            channelModels.value.some((model) => model.template.id === template.id && model.ready),
        ) ??
        templates.value[0]

    if (first) {
        select(first)
    }

    await loadOnAir()
}

async function take() {
    const template = selected.value

    if (!template || busy.value) {
        return
    }

    if (selectedModel.value) {
        await controlModel(selectedModel.value, true)
        return
    }

    const seconds = Number(duration.value.replace(',', '.'))
    const fields = Object.fromEntries(template.fields.map((field) => [field, selectedValues.value[field] ?? '']))
    busy.value = true

    try {
        const result = await graphicsApi.take(
            template.id,
            fields,
            duration.value.trim() !== '' && Number.isFinite(seconds) ? seconds : undefined,
            selectedRecords.value,
        )

        waiting.value = result.state === 'waiting'

        if (result.state === 'waiting') {
            indexStore.msgAlert('success', t('graphics.templates.waiting', { name: templateName(template) }), 3)
        }

        await loadOnAir()
    } catch (error) {
        indexStore.msgAlert('error', message(error, t('graphics.templates.takeFailed')), 5)
    } finally {
        busy.value = false
    }
}

async function takeOut(takeId?: number) {
    if (takeId === undefined && selectedModel.value) {
        await controlModel(selectedModel.value, false)
        return
    }

    try {
        await graphicsApi.out(takeId ?? latest.value?.take_id)
        if (takeId === undefined && !latest.value) {
            waiting.value = false
        }
        await loadOnAir()
    } catch (error) {
        indexStore.msgAlert('error', message(error, t('graphics.templates.outFailed')), 4)
    }
}

async function takeAllOut() {
    if (busy.value) {
        return
    }

    busy.value = true
    try {
        await graphicsApi.out()
        waiting.value = false
        await loadOnAir()
    } catch (error) {
        indexStore.msgAlert('error', message(error, t('graphics.templates.outFailed')), 4)
    } finally {
        busy.value = false
    }
}

/** The next template of the list goes to the preview. */
function next() {
    const index = ordinary.value.findIndex((template) => template.id === selectedId.value)
    const following = ordinary.value[(index + 1) % Math.max(ordinary.value.length, 1)]

    if (following) {
        select(following)
    }
}

watch(selectedId, () => {
    if (selected.value) {
        duration.value = String(selected.value.duration)
    }
})

onMounted(() => {
    load()
})
</script>
<template>
    <div class="onair-page flex flex-col gap-3 min-w-0">
        <div class="flex flex-wrap items-center justify-between gap-2">
            <h1 class="m-0 text-xl font-semibold" :title="t('graphics.intro.onair')">
                {{ t('graphics.sections.onair') }}
            </h1>
            <div class="flex flex-wrap gap-2">
                <RouterLink
                    v-if="isAdmin"
                    :to="{ name: 'graphics-composer', query: { channel: channel?.id } }"
                    class="btn-main min-h-9"
                    >+ {{ t('graphics.library.new') }}</RouterLink
                ><RouterLink
                    v-if="isAdmin && selected"
                    :to="{ name: 'graphics-composer', query: { channel: channel?.id, template: selected.id } }"
                    class="btn-quiet min-h-9"
                    >{{ t('graphics.composer.edit') }}</RouterLink
                >
                <RouterLink
                    :to="{ name: 'graphics-templates', query: { channel: channel?.id } }"
                    class="btn-quiet min-h-9"
                >
                    {{ t('graphics.onAir.library') }}
                </RouterLink>
            </div>
        </div>

        <div class="onair-grid">
            <section class="onair-main min-w-0" :aria-label="t('graphics.onAir.monitors')">
                <figure class="m-0 flex flex-col gap-1.5 min-w-0">
                    <figcaption class="flex items-center gap-2 text-xs">
                        <span class="font-semibold tracking-[0.08em] text-success">{{
                            t('graphics.onAir.preview')
                        }}</span>
                        <span class="text-muted truncate">{{
                            selected ? templateName(selected) : t('graphics.onAir.selectPreview')
                        }}</span>
                    </figcaption>
                    <TemplatePreview
                        :template="selected"
                        :values="selectedValues"
                        :records="selectedRecords"
                        class="rounded-field outline-2 outline-success/60"
                    >
                        <template #overlay>
                            <div
                                v-if="!selected || (selectedModel && !selectedModel.ready)"
                                class="absolute inset-0 flex items-center justify-center p-5 text-center text-sm text-white bg-black/40"
                            >
                                {{ selected ? t('graphics.assigned.configure') : t('graphics.onAir.selectPreview') }}
                            </div>
                        </template>
                    </TemplatePreview>
                </figure>
                <figure class="m-0 flex flex-col gap-1.5 min-w-0">
                    <figcaption class="flex items-center gap-2 text-xs">
                        <span class="font-semibold tracking-[0.08em] text-error">{{
                            t('graphics.onAir.program')
                        }}</span>
                        <span class="text-muted truncate">{{ programName }}</span>
                    </figcaption>
                    <LivePreview data-testid="graphics-program-monitor" />
                </figure>

                <div class="flex flex-col gap-3 min-w-0">
                    <div v-if="selected && Object.keys(mapped).length" class="text-xs text-warning">
                        <p v-if="sourcesUnavailable" role="status">{{ t('graphics.sources.unavailable') }}</p>
                        <template v-for="source in mappedSources" :key="source.id">
                            <p v-if="source.error" role="status">
                                {{ source.name }}:
                                {{ t(source.last_success ? 'graphics.sources.lastGood' : 'graphics.sources.noData') }}
                                {{ source.error }} · {{ t('graphics.sources.lastSuccess') }}:
                                {{
                                    source.last_success
                                        ? new Date(source.last_success).toLocaleString()
                                        : t('graphics.sources.never')
                                }}
                            </p>
                        </template>
                    </div>

                    <section
                        v-if="selected && !selectedModel"
                        class="card-box p-3 flex flex-col gap-2.5"
                        :aria-label="t('graphics.templates.fillLabel', { name: templateName(selected) })"
                    >
                        <h2 class="m-0 text-sm font-semibold" :title="t('graphics.onAir.fieldsHint')">
                            {{ t('graphics.onAir.fieldsOf', { name: templateName(selected) }) }}
                        </h2>
                        <div class="grid grid-cols-[repeat(auto-fill,minmax(180px,1fr))] gap-2">
                            <p v-if="sourcesUnavailable" role="status" class="text-warning text-sm">
                                {{ t('graphics.sources.unavailable') }}
                            </p>
                            <div v-for="source in mappedSources" :key="source.id" class="flex flex-col gap-1">
                                <label class="field-label"
                                    >{{ t('graphics.sources.sourceRecord', { name: source.name })
                                    }}<select
                                        class="field-input"
                                        :value="
                                            selectedRecords[String(source.id)] ??
                                            Object.values(mapped).find((field) => field.source_id === source.id)?.row ??
                                            0
                                        "
                                        @change="
                                            selectedRecords[String(source.id)] = Number(
                                                ($event.target as HTMLSelectElement).value,
                                            )
                                        "
                                    >
                                        <option v-for="(row, index) in source.rows" :key="index" :value="index">
                                            {{ index + 1 }} · {{ Object.values(row).slice(0, 2).join(' · ') }}
                                        </option>
                                    </select></label
                                >
                                <p v-if="source.error" role="status" class="text-xs text-warning m-0">
                                    {{
                                        t(source.last_success ? 'graphics.sources.lastGood' : 'graphics.sources.noData')
                                    }}
                                    {{ source.error }}
                                </p>
                            </div>
                            <label v-for="field in selected.fields" :key="field" class="field-label">
                                {{ '{' + field + '}' }}
                                <input
                                    v-model="selectedValues[field]"
                                    :placeholder="mappedValues[field]"
                                    class="field-input"
                                    type="text"
                                    @keydown.enter.prevent="take"
                                />
                            </label>
                            <label class="field-label">
                                {{ t('graphics.templates.duration') }}
                                <input
                                    v-model="duration"
                                    class="field-input font-mono"
                                    inputmode="decimal"
                                    :title="t('graphics.templates.durationHint')"
                                />
                            </label>
                        </div>
                        <span class="text-xs text-muted"
                            >{{ t('graphics.templates.durationHint') }} ·
                            {{ t(`graphics.templates.lshape.${selected.during_lshape}`) }}</span
                        >
                    </section>

                    <div class="grid grid-cols-[1.6fr_1.3fr_1fr] gap-2">
                        <button
                            type="button"
                            class="min-h-12 rounded-field font-bold tracking-[0.04em] bg-error text-error-content cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
                            :disabled="
                                !selected ||
                                busy ||
                                channelBusy !== null ||
                                Boolean(selectedModel && (unavailable || !selectedModel.ready || selectedModel.on))
                            "
                            @click="take"
                        >
                            {{
                                selectedModel?.kind === 'lshape'
                                    ? t('graphics.assigned.enter')
                                    : t('graphics.onAir.take')
                            }}
                        </button>
                        <button
                            type="button"
                            class="min-h-12 px-2 rounded-field font-bold tracking-[0.04em] whitespace-nowrap border border-line-strong bg-base-100 cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
                            :disabled="
                                busy ||
                                channelBusy !== null ||
                                (selectedModel ? unavailable || !selectedModel.on : !latest && !waiting)
                            "
                            @click="takeOut()"
                        >
                            {{
                                selectedModel?.kind === 'lshape'
                                    ? t('graphics.assigned.fullscreen')
                                    : t('graphics.onAir.out')
                            }}
                        </button>
                        <button
                            type="button"
                            class="min-h-12 rounded-field font-bold tracking-[0.04em] border border-line-strong bg-base-100 cursor-pointer disabled:opacity-50"
                            :disabled="ordinary.length < 2"
                            @click="next"
                        >
                            {{ t('graphics.onAir.next') }}
                        </button>
                    </div>
                </div>

                <section class="card-box p-3 flex flex-col gap-2.5 min-w-0" :aria-label="t('graphics.onAir.nowOnAir')">
                    <div class="flex flex-wrap items-center justify-between gap-2">
                        <h2 class="m-0 text-sm font-semibold">{{ t('graphics.onAir.nowOnAir') }}</h2>
                        <button type="button" class="btn-quiet min-h-8" :disabled="busy" @click="takeAllOut">
                            {{ t('graphics.templates.allOut') }}
                        </button>
                    </div>
                    <div role="status" aria-live="polite" :aria-label="t('graphics.onAir.temporaryOnAir')">
                        <p v-if="!onAir.length" class="m-0 text-sm text-muted">
                            {{ t('graphics.onAir.noTemporary') }}
                        </p>
                        <ul v-else class="m-0 p-0 list-none flex flex-col gap-1.5">
                            <li
                                v-for="graphic in onAir"
                                :key="graphic.take_id"
                                class="flex flex-wrap items-center gap-2 px-2.5 py-1.5 rounded-field bg-base-100"
                            >
                                <span
                                    class="dot"
                                    :class="graphic.leaving ? 'bg-muted' : 'bg-error'"
                                    aria-hidden="true"
                                />
                                <strong class="font-semibold text-sm flex-auto truncate">{{
                                    onAirName(graphic)
                                }}</strong>
                                <span class="text-xs text-muted font-mono">
                                    {{ graphic.leaving ? t('graphics.templates.leaving') : onAirSince(graphic.since) }}
                                </span>
                                <button
                                    type="button"
                                    class="btn-quiet min-h-8"
                                    :disabled="graphic.leaving"
                                    @click="takeOut(graphic.take_id)"
                                >
                                    {{ t('graphics.templates.out') }}
                                </button>
                            </li>
                        </ul>
                    </div>
                    <section
                        class="flex flex-col gap-1.5 border-t border-line pt-2.5"
                        :aria-label="t('graphics.onAir.permanentTitle')"
                    >
                        <h3
                            class="m-0 text-[11px] text-muted uppercase tracking-[0.08em]"
                            :title="t('graphics.onAir.permanentHint')"
                        >
                            {{ t('graphics.onAir.permanentTitle') }}
                        </h3>
                        <p v-if="unavailable" class="m-0 text-sm text-muted">
                            {{ t('graphics.onAir.statusUnavailable') }}
                        </p>
                        <div class="flex flex-wrap gap-1.5">
                            <button
                                v-for="model in permanent"
                                :key="model.template.id"
                                type="button"
                                class="btn-quiet min-h-9 inline-flex items-center gap-2"
                                :class="model.on ? 'border-success text-success' : ''"
                                :aria-pressed="model.on"
                                :disabled="unavailable || channelBusy !== null || (!model.ready && !model.on)"
                                @click="controlModel(model, !model.on)"
                            >
                                <span
                                    :class="model.on ? 'dot bg-success' : 'inline-block w-2 h-2 rounded-full bg-muted'"
                                    aria-hidden="true"
                                />
                                {{ templateName(model.template) }}
                                <span class="text-xs">{{
                                    t(model.on ? 'graphics.assigned.on' : 'graphics.assigned.off')
                                }}</span>
                            </button>
                        </div>
                        <p class="m-0 text-xs text-muted">{{ t('graphics.onAir.permanentRequested') }}</p>
                    </section>
                </section>
            </section>

            <aside
                class="onair-templates card-box p-3 flex flex-col gap-2 min-w-0"
                :aria-label="t('graphics.onAir.templates')"
            >
                <div class="flex items-center justify-between gap-2">
                    <h2 class="m-0 text-sm font-semibold">{{ t('graphics.onAir.templates') }}</h2>
                    <span class="text-xs text-muted">{{ t('graphics.onAir.count', { count: templates.length }) }}</span>
                </div>
                <input
                    v-model="search"
                    type="search"
                    class="field-input min-h-9"
                    :aria-label="t('graphics.onAir.search')"
                    :placeholder="t('graphics.onAir.search')"
                />
                <div
                    v-if="groupNames.length > 1"
                    class="flex flex-wrap gap-1"
                    role="group"
                    :aria-label="t('graphics.onAir.groups')"
                >
                    <button
                        type="button"
                        class="min-h-7 px-2.5 rounded-full border text-xs cursor-pointer"
                        :class="
                            groupFilter === null
                                ? 'border-accent text-accent bg-accent/10 font-semibold'
                                : 'border-line-strong text-muted'
                        "
                        :aria-pressed="groupFilter === null"
                        @click="groupFilter = null"
                    >
                        {{ t('graphics.onAir.allGroups') }}
                    </button>
                    <button
                        v-for="name in groupNames"
                        :key="name"
                        type="button"
                        class="min-h-7 px-2.5 rounded-full border text-xs cursor-pointer"
                        :class="
                            groupFilter === name
                                ? 'border-accent text-accent bg-accent/10 font-semibold'
                                : 'border-line-strong text-muted'
                        "
                        :aria-pressed="groupFilter === name"
                        @click="groupFilter = groupFilter === name ? null : name"
                    >
                        {{ name }}
                    </button>
                </div>
                <p v-if="loaded && !templates.length" class="m-0 text-sm text-muted">
                    {{ t('graphics.templates.none') }}
                </p>
                <p v-else-if="loaded && !filtered.length" class="m-0 text-sm text-muted">
                    {{ t('graphics.onAir.noMatch') }}
                </p>
                <div class="onair-list flex flex-col gap-2 min-h-0 overflow-y-auto -mx-1 px-1">
                    <div v-for="group in groups" :key="group.name" class="flex flex-col gap-1">
                        <span class="text-[11px] text-muted uppercase tracking-[0.08em]">{{ group.name }}</span>
                        <button
                            v-for="template in group.items"
                            :key="template.id"
                            type="button"
                            class="cursor-pointer min-h-10 px-2.5 py-1.5 flex items-center gap-2 border rounded-field text-left bg-base-100"
                            :class="
                                selectedId === template.id ? 'border-success' : 'border-transparent hover:border-accent'
                            "
                            :aria-pressed="selectedId === template.id"
                            @click="select(template)"
                        >
                            <strong class="font-semibold text-sm truncate min-w-0" :title="templateName(template)">{{
                                templateName(template)
                            }}</strong>
                            <span
                                v-if="
                                    onAir.some((graphic) => graphic.template_id === template.id && !graphic.leaving) ||
                                    channelModels.some((model) => model.template.id === template.id && model.on)
                                "
                                class="text-[10px] font-bold text-error shrink-0"
                            >
                                {{ t('graphics.onAir.badge') }}
                            </span>
                            <span class="ml-auto text-xs text-muted truncate max-w-[45%] shrink-[2]">{{
                                templateHint(template)
                            }}</span>
                        </button>
                    </div>
                </div>
            </aside>
        </div>
    </div>
</template>

<style scoped>
.onair-grid {
    display: grid;
    gap: 0.75rem;
    grid-template-columns: minmax(0, 1fr);
}
.onair-main {
    display: grid;
    gap: 0.75rem;
    grid-template-columns: minmax(0, 1fr);
    align-content: start;
}
@media (min-width: 768px) {
    .onair-main {
        grid-template-columns: repeat(2, minmax(0, 1fr));
    }
}
/* Wide screens: the page fits the window and only the template list scrolls. */
@media (min-width: 1100px) {
    .onair-grid {
        grid-template-columns: minmax(0, 1fr) 300px;
        align-items: start;
    }
    .onair-templates {
        position: sticky;
        top: 4.5rem;
        max-height: calc(100dvh - 6rem);
    }
}
</style>
