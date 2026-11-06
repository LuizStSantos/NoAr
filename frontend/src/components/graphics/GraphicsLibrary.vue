<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import TemplatePreview from '@/components/graphics/TemplatePreview.vue'
import { AuthFetchError } from '@/composables/authFetch'
import {
    type ChannelGraphic,
    type GraphicOnAir,
    type GraphicsTemplate,
    fieldList,
    useGraphicsApi,
} from '@/composables/graphicsTemplates'
import { useAuth } from '@/stores/auth'
import { useConfig } from '@/stores/config'
import { useIndex } from '@/stores/index'

const { t } = useI18n()
const configStore = useConfig()
const indexStore = useIndex()
const graphicsApi = useGraphicsApi()
const authStore = useAuth()
const isAdmin = computed(() => ['global_admin', 'channel_admin'].includes(authStore.role))
const channelId = computed(() => configStore.channels[configStore.i]?.id)
const templates = ref<GraphicsTemplate[]>([])
const onAir = ref<GraphicOnAir[]>([])
const channelModels = ref<ChannelGraphic[]>([])
const loaded = ref(false)
const group = ref<string | null>(null)
const busy = ref<number | null>(null)
let timer: ReturnType<typeof setInterval> | undefined
let stateVersion = 0

const assignedIds = computed(() => new Set(channelModels.value.map((view) => view.template.id)))
const ordinary = computed(() => templates.value.filter((template) => !assignedIds.value.has(template.id)))
const groups = computed(() => [
    { key: '@channel', name: t('graphics.assigned.group') },
    { key: '@scenes', name: t('graphics.assigned.scenes') },
    ...[...new Set(ordinary.value.map((template) => template.group))]
        .filter(Boolean)
        .map((name) => ({ key: `custom:${name}`, name })),
])

const sections = computed(() =>
    [
        {
            key: '@channel',
            name: t('graphics.assigned.group'),
            cards: channelModels.value
                .filter((view) => view.kind !== 'scene')
                .map((view) => ({ template: view.template, assigned: view })),
        },
        {
            key: '@scenes',
            name: t('graphics.assigned.scenes'),
            cards: channelModels.value
                .filter((view) => view.kind === 'scene')
                .map((view) => ({ template: view.template, assigned: view })),
        },
        {
            key: '@ordinary',
            name: t('graphics.sections.templates'),
            cards: ordinary.value
                .filter((template) => group.value === null || group.value === `custom:${template.group}`)
                .map((template) => ({ template, assigned: undefined as ChannelGraphic | undefined })),
        },
    ].filter(
        (section) =>
            (section.cards.length || section.key === '@scenes') &&
            (group.value === null ||
                group.value === section.key ||
                (group.value.startsWith('custom:') && section.key === '@ordinary')),
    ),
)

function sample(template: GraphicsTemplate): Record<string, string> {
    return Object.fromEntries(template.fields.map((field) => [field, `[${field}]`]))
}

function title(template: GraphicsTemplate, assigned?: ChannelGraphic): string {
    return assigned
        ? assigned.kind === 'scene'
            ? assigned.scene_name!
            : t(`graphics.assigned.${assigned.kind}`)
        : template.name
}

function editTarget(template: GraphicsTemplate, assigned?: ChannelGraphic) {
    return {
        name: assigned ? 'graphics-channel-editor' : 'graphics-composer',
        params: assigned ? { kind: assigned.kind } : {},
        query: { channel: channelId.value, ...(!assigned ? { template: template.id } : {}) },
    }
}

function reportError(error: unknown) {
    indexStore.msgAlert(
        'error',
        error instanceof AuthFetchError && typeof error.data === 'string'
            ? error.data
            : t('graphics.templates.loadFailed'),
        4,
    )
}

async function loadLibrary() {
    const requestedChannel = channelId.value
    const version = ++stateVersion

    try {
        const [models, aired, assigned] = await Promise.all([
            graphicsApi.templates(),
            graphicsApi.onAir(),
            graphicsApi.channel(),
        ])

        if (requestedChannel === channelId.value && version === stateVersion) {
            templates.value = models
            onAir.value = aired
            channelModels.value = assigned
        }
    } catch (error) {
        reportError(error)
    } finally {
        loaded.value = true
    }
}

function replaceChannelModels(views: ChannelGraphic[]) {
    const previous = assignedIds.value
    const current = new Set(views.map((view) => view.template.id))
    // A deleted scene must disappear, rather than reappearing as an ordinary GC.
    templates.value = templates.value.filter((model) => !previous.has(model.id) || current.has(model.id))
    channelModels.value = views
}

async function refreshState() {
    if (busy.value !== null) {
        return
    }

    const version = stateVersion

    try {
        const assigned = await graphicsApi.channel()

        if (version === stateVersion) {
            replaceChannelModels(assigned)
        }
    } catch {
        // Keep the last confirmed state during a temporary connection loss.
    }
}

async function control(view: ChannelGraphic) {
    busy.value = view.template.id
    stateVersion += 1

    try {
        replaceChannelModels(await graphicsApi.controlChannel(view.template.id, !view.on))

        if (view.kind === 'scene') {
            indexStore.msgAlert('success', t('graphics.scenes.applied', { name: view.scene_name }), 2)
        }
    } catch (error) {
        reportError(error)
    } finally {
        busy.value = null
    }
}

onMounted(() => {
    void loadLibrary()
    timer = setInterval(refreshState, 2000)
})
onUnmounted(() => clearInterval(timer))
watch(channelId, () => {
    templates.value = []
    channelModels.value = []
    onAir.value = []
    loaded.value = false
    group.value = null
    void loadLibrary()
})
</script>
<template>
    <div class="flex flex-wrap items-start justify-between gap-3">
        <div>
            <h1 class="m-0 text-xl font-semibold">{{ t('graphics.sections.templates') }}</h1>
            <p class="mt-1 mb-0 text-muted">{{ t('graphics.intro.templates') }}</p>
        </div>
        <RouterLink v-if="isAdmin" :to="{ name: 'graphics-composer', query: { channel: channelId } }" class="btn-main">
            + {{ t('graphics.library.new') }}
        </RouterLink>
    </div>
    <div class="flex flex-wrap items-center gap-2" role="group" :aria-label="t('graphics.library.filter')">
        <button type="button" class="btn-quiet rounded-full" :aria-pressed="group === null" @click="group = null">
            {{ t('graphics.library.all') }}
        </button>
        <button
            v-for="item in groups"
            :key="item.key"
            type="button"
            class="btn-quiet rounded-full"
            :aria-pressed="group === item.key"
            @click="group = item.key"
        >
            {{ item.name }}
        </button>
    </div>
    <p v-if="loaded && !templates.length" class="m-0 text-muted">{{ t('graphics.templates.none') }}</p>
    <section v-for="section in sections" :key="section.key" :aria-label="section.name" class="flex flex-col gap-3">
        <div class="flex flex-wrap items-center justify-between gap-2">
            <h2 class="m-0 text-base font-semibold">{{ section.name }}</h2>
            <RouterLink
                v-if="section.key === '@scenes' && isAdmin"
                :to="{ name: 'graphics-channel-editor', params: { kind: 'scene' }, query: { channel: channelId } }"
                class="link-accent text-sm"
            >
                {{ t('graphics.assigned.editScenes') }}
            </RouterLink>
        </div>
        <p v-if="section.key === '@scenes' && !section.cards.length" class="m-0 text-sm text-muted">
            {{ t('graphics.scenes.empty') }}
        </p>
        <div class="grid grid-cols-[repeat(auto-fill,minmax(min(260px,100%),1fr))] gap-3">
            <article
                v-for="card in section.cards"
                :key="card.template.id"
                :data-kind="card.assigned?.kind ?? 'template'"
                class="card-box overflow-hidden flex flex-col"
            >
                <TemplatePreview
                    :template="card.template"
                    :values="sample(card.template)"
                    class="bg-[linear-gradient(180deg,#5b7da6_0%,#3f5f86_55%,#3f6b2f_55%,#2f5524_100%)]"
                >
                    <template #overlay>
                        <span
                            v-if="
                                !card.assigned &&
                                onAir.some((graphic) => graphic.template_id === card.template.id && !graphic.leaving)
                            "
                            class="absolute right-2 top-2 z-10 px-2 py-0.5 rounded-full bg-error text-error-content text-[11px] font-bold"
                        >
                            {{ t('graphics.onAir.badge') }}
                        </span>
                        <span
                            v-if="card.assigned && !card.assigned.ready"
                            class="absolute inset-0 flex items-center justify-center text-sm text-white p-4 text-center bg-black/40"
                        >
                            {{ t('graphics.assigned.configure') }}
                        </span>
                    </template>
                </TemplatePreview>
                <div class="px-4 py-3 flex flex-col gap-1.5 flex-auto">
                    <h3 class="m-0 text-sm font-semibold">{{ title(card.template, card.assigned) }}</h3>
                    <template v-if="!card.assigned">
                        <span class="text-xs text-muted">{{
                            card.template.group || t('graphics.templates.noGroup')
                        }}</span>
                        <span class="text-xs text-muted">
                            {{ fieldList(card.template) || t('graphics.templates.noFields') }} ·
                            {{
                                card.template.duration
                                    ? t('graphics.library.seconds', { seconds: card.template.duration })
                                    : t('graphics.library.untilOut')
                            }}
                        </span>
                        <span class="text-xs text-muted">{{
                            t(`graphics.templates.lshape.${card.template.during_lshape}`)
                        }}</span>
                    </template>
                    <span v-else class="text-xs text-muted">{{
                        t(`graphics.assigned.hint.${card.assigned.kind}`)
                    }}</span>
                    <div class="flex flex-wrap gap-2 mt-auto pt-2">
                        <RouterLink
                            v-if="isAdmin"
                            :to="editTarget(card.template, card.assigned)"
                            class="btn-quiet min-h-9"
                        >
                            {{ t('graphics.composer.edit') }}
                        </RouterLink>
                        <label
                            v-if="card.assigned && ['logo', 'ticker'].includes(card.assigned.kind)"
                            class="inline-flex items-center gap-2 min-h-9 px-2 text-sm cursor-pointer"
                        >
                            <input
                                type="checkbox"
                                role="switch"
                                class="toggle toggle-accent"
                                :aria-label="title(card.template, card.assigned)"
                                :checked="card.assigned.on"
                                :disabled="busy !== null || (!card.assigned.ready && !card.assigned.on)"
                                @click.prevent="control(card.assigned)"
                            />
                            {{ card.assigned.on ? t('graphics.assigned.on') : t('graphics.assigned.off') }}
                        </label>
                        <button
                            v-else-if="card.assigned"
                            type="button"
                            class="btn-main min-h-9"
                            :disabled="busy !== null || (!card.assigned.ready && !card.assigned.on)"
                            @click="control(card.assigned)"
                        >
                            {{
                                card.assigned.kind === 'scene'
                                    ? t('graphics.assigned.apply')
                                    : card.assigned.on
                                      ? t('graphics.assigned.fullscreen')
                                      : t('graphics.assigned.enter')
                            }}
                        </button>
                        <RouterLink
                            v-else
                            :to="{ name: 'graphics-onair', query: { channel: channelId, template: card.template.id } }"
                            class="btn-main min-h-9"
                        >
                            {{ t('graphics.library.toAir') }}
                        </RouterLink>
                    </div>
                </div>
            </article>
        </div>
    </section>
</template>
