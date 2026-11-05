<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, reactive, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute } from 'vue-router'

import LiveVideo from '@/components/player/LiveVideo.vue'
import SceneButtons from '@/components/graphics/SceneButtons.vue'
import LogoRemote from '@/components/settings/LogoRemote.vue'
import SectionCard from '@/components/settings/SectionCard.vue'
import TemplatePreview from '@/components/graphics/TemplatePreview.vue'
import { AuthFetchError } from '@/composables/authFetch'
import {
    type GraphicOnAir,
    type GraphicsTemplate,
    fieldList,
    onAirSince,
    useGraphicsApi,
} from '@/composables/graphicsTemplates'
import { useAuth } from '@/stores/auth'
import { useConfig } from '@/stores/config'
import { useIndex } from '@/stores/index'

/**
 * Grafismo › No ar: the operator's screen. A template is picked and filled
 * in on the preview, then put on air; the program monitor shows the real
 * output, and what is on air is listed to take out.
 */
const { t } = useI18n()
const route = useRoute()
const authStore = useAuth()
const configStore = useConfig()
const indexStore = useIndex()
const graphicsApi = useGraphicsApi()
const isAdmin = ['global_admin', 'channel_admin'].includes(authStore.role)

const templates = ref<GraphicsTemplate[]>([])
const onAir = ref<GraphicOnAir[]>([])
const loaded = ref(false)
const selectedId = ref<number | null>(null)
const values = reactive<Record<string, Record<string, string>>>({})
const duration = ref('')
const busy = ref(false)
const remote = ref<InstanceType<typeof LogoRemote> | null>(null)
const muted = ref(true)
let timer: ReturnType<typeof setInterval> | undefined

const channel = computed(() => configStore.channels[configStore.i])
const selected = computed(() => templates.value.find((template) => template.id === selectedId.value))
const selectedValues = computed(() => (selectedId.value === null ? {} : (values[selectedId.value] ?? {})))
const programUrl = computed(() => (channel.value ? `/api/preview/${channel.value.id}?quality=light` : ''))
const latest = computed(() => [...onAir.value].reverse().find((graphic) => !graphic.leaving))
const groups = computed(() => {
    const byGroup = new Map<string, GraphicsTemplate[]>()

    for (const template of templates.value) {
        const group = template.group || t('graphics.templates.noGroup')
        byGroup.set(group, [...(byGroup.get(group) ?? []), template])
    }

    return [...byGroup.entries()].map(([name, items]) => ({ name, items }))
})

function message(error: unknown, fallback: string): string {
    return error instanceof AuthFetchError && typeof error.data === 'string' && error.data ? error.data : fallback
}

async function loadOnAir() {
    try {
        onAir.value = await graphicsApi.onAir()
    } catch {
        // The next poll tries again.
    }
}

function select(template: GraphicsTemplate) {
    values[template.id] ??= {}
    selectedId.value = template.id
    duration.value = String(template.duration)
}

async function load() {
    try {
        const [models, channelModels] = await Promise.all([graphicsApi.templates(), graphicsApi.channel()])
        const assigned = new Set(channelModels.map((view) => view.template.id))
        templates.value = models.filter((model) => !assigned.has(model.id))
    } catch (error) {
        indexStore.msgAlert('error', message(error, t('graphics.templates.loadFailed')), 4)
    } finally {
        loaded.value = true
    }

    const wanted = Number(route.query.template)
    const first = templates.value.find((template) => template.id === wanted) ?? templates.value[0]

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

    const seconds = Number(duration.value.replace(',', '.'))
    const fields = Object.fromEntries(template.fields.map((field) => [field, selectedValues.value[field] ?? '']))
    busy.value = true

    try {
        const result = await graphicsApi.take(
            template.id,
            fields,
            duration.value.trim() !== '' && Number.isFinite(seconds) ? seconds : undefined,
        )

        if (result.state === 'waiting') {
            indexStore.msgAlert('success', t('graphics.templates.waiting', { name: template.name }), 3)
        }

        await loadOnAir()
    } catch (error) {
        indexStore.msgAlert('error', message(error, t('graphics.templates.takeFailed')), 5)
    } finally {
        busy.value = false
    }
}

async function takeOut(takeId?: number) {
    try {
        await graphicsApi.out(takeId ?? latest.value?.take_id)
        await loadOnAir()
    } catch (error) {
        indexStore.msgAlert('error', message(error, t('graphics.templates.outFailed')), 4)
    }
}

async function takeAllOut() {
    try {
        await graphicsApi.out()
        await loadOnAir()
    } catch (error) {
        indexStore.msgAlert('error', message(error, t('graphics.templates.outFailed')), 4)
    }
}

/** The next template of the list goes to the preview. */
function next() {
    const index = templates.value.findIndex((template) => template.id === selectedId.value)
    const following = templates.value[(index + 1) % Math.max(templates.value.length, 1)]

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
    timer = setInterval(loadOnAir, 2000)
})

onBeforeUnmount(() => clearInterval(timer))
</script>
<template>
    <div class="flex flex-wrap items-start justify-between gap-3">
        <div>
            <h1 class="m-0 text-xl font-semibold">{{ t('graphics.sections.onair') }}</h1>
            <p class="mt-1 mb-0 text-muted">{{ t('graphics.intro.onair') }}</p>
        </div>
        <div class="flex flex-wrap gap-2">
            <RouterLink
                v-if="isAdmin"
                :to="{ name: 'graphics-composer', query: { channel: channel?.id } }"
                class="btn-main"
                >+ {{ t('graphics.library.new') }}</RouterLink
            ><RouterLink
                v-if="isAdmin && selected"
                :to="{ name: 'graphics-composer', query: { channel: channel?.id, template: selected.id } }"
                class="btn-quiet"
                >{{ t('graphics.composer.edit') }}</RouterLink
            >
            <RouterLink
                :to="{ name: 'graphics-templates', query: { channel: channel?.id } }"
                class="btn-quiet min-h-11"
            >
                {{ t('graphics.onAir.library') }}
            </RouterLink>
        </div>
    </div>

    <div class="flex flex-wrap gap-4 items-start">
        <section class="flex-[999_1_560px] min-w-0 flex flex-col gap-4" :aria-label="t('graphics.onAir.monitors')">
            <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
                <figure class="m-0 card-box p-2.5 flex flex-col gap-2">
                    <figcaption class="flex items-center justify-between gap-2 text-xs">
                        <span class="font-semibold tracking-[0.08em] text-success">{{
                            t('graphics.onAir.preview')
                        }}</span>
                        <span class="text-muted truncate">{{ selected?.name ?? '' }}</span>
                    </figcaption>
                    <TemplatePreview
                        :template="selected"
                        :values="selectedValues"
                        class="rounded-field border border-success/60 bg-[repeating-conic-gradient(#2b2c30_0_25%,#232428_0_50%)] bg-[length:24px_24px]"
                    />
                </figure>
                <figure class="m-0 card-box p-2.5 flex flex-col gap-2">
                    <figcaption class="flex items-center justify-between gap-2 text-xs">
                        <span class="font-semibold tracking-[0.08em] text-error">{{
                            t('graphics.onAir.program')
                        }}</span>
                        <span class="text-muted truncate">{{ latest?.name ?? t('graphics.onAir.noGraphic') }}</span>
                    </figcaption>
                    <div class="relative aspect-video rounded-field overflow-hidden border border-error/60 bg-black">
                        <LiveVideo
                            v-if="programUrl"
                            :key="programUrl"
                            :src="programUrl"
                            direct
                            :token="authStore.jwtToken"
                            :muted="muted"
                        />
                    </div>
                </figure>
            </div>

            <section
                v-if="selected"
                class="card-box px-5 py-4 flex flex-col gap-3"
                :aria-label="t('graphics.templates.fillLabel', { name: selected.name })"
            >
                <div class="flex flex-wrap items-baseline justify-between gap-2">
                    <h2 class="m-0 text-base font-semibold">
                        {{ t('graphics.onAir.fieldsOf', { name: selected.name }) }}
                    </h2>
                    <span class="text-xs text-muted">{{ t('graphics.onAir.fieldsHint') }}</span>
                </div>
                <div class="grid grid-cols-[repeat(auto-fill,minmax(220px,1fr))] gap-3">
                    <label v-for="field in selected.fields" :key="field" class="field-label">
                        {{ '{' + field + '}' }}
                        <input
                            v-model="selectedValues[field]"
                            class="field-input"
                            type="text"
                            @keydown.enter.prevent="take"
                        />
                    </label>
                    <label class="field-label">
                        {{ t('graphics.templates.duration') }}
                        <input v-model="duration" class="field-input font-mono" inputmode="decimal" />
                        <span class="text-xs text-muted">{{ t('graphics.templates.durationHint') }}</span>
                    </label>
                </div>
                <span class="text-xs text-muted">{{ t(`graphics.templates.lshape.${selected.during_lshape}`) }}</span>
            </section>

            <div class="grid grid-cols-3 gap-2">
                <button
                    type="button"
                    class="min-h-14 rounded-field font-bold tracking-[0.04em] bg-error text-error-content cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
                    :disabled="!selected || busy"
                    @click="take"
                >
                    {{ t('graphics.onAir.take') }}
                </button>
                <button
                    type="button"
                    class="min-h-14 rounded-field font-bold tracking-[0.04em] border border-line-strong bg-base-100 cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
                    :disabled="!latest"
                    @click="takeOut()"
                >
                    {{ t('graphics.onAir.out') }}
                </button>
                <button
                    type="button"
                    class="min-h-14 rounded-field font-bold tracking-[0.04em] border border-line-strong bg-base-100 cursor-pointer disabled:opacity-50"
                    :disabled="templates.length < 2"
                    @click="next"
                >
                    {{ t('graphics.onAir.next') }}
                </button>
            </div>

            <section class="card-box px-5 py-4 flex flex-col gap-3" role="status" aria-live="polite">
                <div class="flex flex-wrap items-center justify-between gap-2">
                    <h2 class="m-0 text-base font-semibold">{{ t('graphics.templates.onAir') }}</h2>
                    <button
                        type="button"
                        class="btn-quiet min-h-9"
                        :disabled="!onAir.some((graphic) => !graphic.leaving)"
                        @click="takeAllOut"
                    >
                        {{ t('graphics.templates.allOut') }}
                    </button>
                </div>
                <p v-if="!onAir.length" class="m-0 text-sm text-muted">{{ t('graphics.templates.nothingOnAir') }}</p>
                <ul v-else class="m-0 p-0 list-none flex flex-col gap-1.5">
                    <li
                        v-for="graphic in onAir"
                        :key="graphic.take_id"
                        class="flex flex-wrap items-center gap-3 px-3 py-2 rounded-field border border-line-strong bg-base-100"
                    >
                        <span class="dot" :class="graphic.leaving ? 'bg-muted' : 'bg-error'" aria-hidden="true" />
                        <strong class="font-semibold text-sm flex-auto">{{ graphic.name }}</strong>
                        <span class="text-xs text-muted font-mono">
                            {{ graphic.leaving ? t('graphics.templates.leaving') : onAirSince(graphic.since) }}
                        </span>
                        <button
                            type="button"
                            class="btn-quiet min-h-9"
                            :disabled="graphic.leaving"
                            @click="takeOut(graphic.take_id)"
                        >
                            {{ t('graphics.templates.out') }}
                        </button>
                    </li>
                </ul>
            </section>

            <template v-if="configStore.playout?.processing">
                <LogoRemote ref="remote" part="live" />

                <SectionCard :title="t('graphics.sections.scenes')" :description="t('graphics.scenes.liveIntro')">
                    <SceneButtons @applied="remote?.load()" />
                    <p
                        v-if="!configStore.playoutSaved?.processing?.graphics_scenes?.length"
                        class="m-0 text-muted text-sm"
                    >
                        {{ t('graphics.scenes.none') }}
                        <RouterLink
                            v-if="isAdmin"
                            :to="{ name: 'graphics-scenes', query: { channel: channel?.id } }"
                            class="link-accent"
                        >
                            {{ t('graphics.scenes.create') }}
                        </RouterLink>
                    </p>
                </SectionCard>
            </template>
        </section>

        <aside
            class="flex-[1_1_300px] min-w-0 card-box px-4 py-4 flex flex-col gap-3"
            :aria-label="t('graphics.onAir.templates')"
        >
            <div class="flex items-center justify-between gap-2">
                <h2 class="m-0 text-base font-semibold">{{ t('graphics.onAir.templates') }}</h2>
                <span class="text-xs text-muted">{{ t('graphics.onAir.count', { count: templates.length }) }}</span>
            </div>
            <p v-if="loaded && !templates.length" class="m-0 text-sm text-muted">{{ t('graphics.templates.none') }}</p>
            <div v-for="group in groups" :key="group.name" class="flex flex-col gap-1.5">
                <span class="text-[11px] text-muted uppercase tracking-[0.08em]">{{ group.name }}</span>
                <button
                    v-for="template in group.items"
                    :key="template.id"
                    type="button"
                    class="cursor-pointer min-h-12 px-3 py-2 flex flex-col gap-0.5 border rounded-field text-left bg-base-100"
                    :class="selectedId === template.id ? 'border-success' : 'border-line-strong hover:border-accent'"
                    :aria-pressed="selectedId === template.id"
                    @click="select(template)"
                >
                    <span class="flex items-center gap-2">
                        <strong class="font-semibold text-sm flex-auto">{{ template.name }}</strong>
                        <span
                            v-if="onAir.some((graphic) => graphic.template_id === template.id && !graphic.leaving)"
                            class="text-[11px] font-bold text-error"
                        >
                            {{ t('graphics.onAir.badge') }}
                        </span>
                    </span>
                    <span class="text-xs text-muted">{{
                        fieldList(template) || t('graphics.templates.noFields')
                    }}</span>
                </button>
            </div>
            <p class="m-0 text-xs text-muted leading-relaxed">{{ t('graphics.onAir.rundownsSoon') }}</p>
        </aside>
    </div>
</template>
