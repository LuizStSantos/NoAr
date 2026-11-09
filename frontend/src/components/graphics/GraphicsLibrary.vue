<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import GenericModal from '@/components/utils/GenericModal.vue'
import TemplatePreview from '@/components/graphics/TemplatePreview.vue'
import { AuthFetchError } from '@/composables/authFetch'
import {
    type ChannelGraphic,
    type GraphicOnAir,
    type GraphicsTemplate,
    type TemplateUsage,
    fieldList,
    useGraphicsApi,
} from '@/composables/graphicsTemplates'
import { useGraphicsDisplayNames } from '@/composables/graphicsDisplayNames'
import { useAuth } from '@/stores/auth'
import { useConfig } from '@/stores/config'
import { useIndex } from '@/stores/index'

const { t } = useI18n()
const { templateName } = useGraphicsDisplayNames()
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

const groups = computed(() =>
    [...new Set(templates.value.map((template) => template.group))].map((name) => ({
        key: name,
        name: name || t('graphics.templates.noGroup'),
    })),
)
const sections = computed(() =>
    groups.value
        .filter((item) => group.value === null || group.value === item.key)
        .map((item) => ({
            ...item,
            cards: templates.value
                .filter((template) => template.group === item.key)
                .map((template) => ({
                    template,
                    assigned: channelModels.value.find((model) => model.template.id === template.id),
                })),
        })),
)
const deleting = ref<GraphicsTemplate | null>(null)
const deleteUsage = ref<TemplateUsage | null>(null)

async function openDeletion(template: GraphicsTemplate) {
    if (busy.value !== null) {
        return
    }

    const requestedChannel = channelId.value
    busy.value = template.id

    try {
        const usage = await graphicsApi.usage(template.id)

        if (requestedChannel === channelId.value) {
            deleteUsage.value = usage
            deleting.value = template
        }
    } catch (error) {
        reportError(error)
    } finally {
        busy.value = null
    }
}

function confirmDeletion(confirmed: boolean) {
    if (busy.value !== null) {
        return
    }

    if (confirmed) {
        void deleteTemplate()
    } else {
        deleting.value = null
    }
}

async function deleteTemplate() {
    if (!deleting.value || busy.value !== null) {
        return
    }

    busy.value = deleting.value.id

    try {
        await graphicsApi.remove(deleting.value.id)
        deleting.value = null
        await loadLibrary()
    } catch (error) {
        reportError(error)
    } finally {
        busy.value = null
    }
}

function sample(template: GraphicsTemplate): Record<string, string> {
    return Object.fromEntries(template.fields.map((field) => [field, `[${field}]`]))
}

function title(template: GraphicsTemplate, assigned?: ChannelGraphic): string {
    return assigned?.kind === 'scene' ? assigned.scene_name! : templateName(template)
}

function editTarget(template: GraphicsTemplate, assigned?: ChannelGraphic) {
    return {
        name: assigned?.kind === 'scene' ? 'graphics-channel-editor' : 'graphics-composer',
        params: assigned?.kind === 'scene' ? { kind: 'scene' } : {},
        query: { channel: channelId.value, ...(assigned?.kind !== 'scene' ? { template: template.id } : {}) },
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
    channelModels.value = views
}

async function refreshState() {
    if (busy.value !== null) {
        return
    }

    const version = stateVersion

    try {
        const [assigned, aired] = await Promise.all([graphicsApi.channel(), graphicsApi.onAir()])

        if (version === stateVersion) {
            replaceChannelModels(assigned)
            onAir.value = aired
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
onUnmounted(() => {
    ++stateVersion
    clearInterval(timer)
})
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
        <button
            type="button"
            class="btn-quiet rounded-full"
            :class="group === null ? 'border-accent! bg-accent/15! text-accent!' : ''"
            :aria-pressed="group === null"
            @click="group = null"
        >
            {{ t('graphics.library.all') }}
        </button>
        <button
            v-for="item in groups"
            :key="item.key"
            type="button"
            class="btn-quiet rounded-full"
            :class="group === item.key ? 'border-accent! bg-accent/15! text-accent!' : ''"
            :aria-pressed="group === item.key"
            @click="group = item.key"
        >
            {{ item.name }}
        </button>
    </div>
    <GenericModal
        :show="Boolean(deleting)"
        :title="t('graphics.library.deleteTitle')"
        :ok-label="t('graphics.library.delete')"
        :danger="true"
        :ok-disabled="busy !== null"
        :modal-action="confirmDeletion"
    >
        <p>{{ t('graphics.library.deleteConfirm', { name: deleting ? templateName(deleting) : '' }) }}</p>
        <p
            v-if="deleteUsage?.assignment || deleteUsage?.on || deleteUsage?.temporary_instances"
            class="text-sm text-muted"
        >
            {{ t('graphics.library.deleteUsed', { count: deleteUsage.temporary_instances }) }}
        </p>
        <p v-if="deleteUsage?.always_on" class="text-sm">
            {{ t('graphics.composer.alwaysOn') }} ·
            {{ t(deleteUsage.on ? 'graphics.assigned.on' : 'graphics.assigned.off') }}
        </p>
        <p v-if="deleteUsage?.events.length" class="text-sm text-muted">
            {{ t('graphics.library.deleteScheduled') }}
        </p>
        <ul v-if="deleteUsage?.events.length" class="text-sm text-muted">
            <li v-for="event in deleteUsage.events" :key="event.id">
                {{ event.title }} · {{ t('graphics.library.eventRetained') }}
            </li>
        </ul>
        <p class="text-xs text-muted">{{ t('graphics.library.deleteAssets') }}</p>
    </GenericModal>
    <p v-if="loaded && !templates.length" class="m-0 text-muted">{{ t('graphics.templates.none') }}</p>
    <section v-for="section in sections" :key="section.key" :aria-label="section.name" class="flex flex-col gap-3">
        <div class="flex flex-wrap items-center justify-between gap-2">
            <h2 class="m-0 text-base font-semibold">{{ section.name }}</h2>
        </div>
        <div class="grid grid-cols-[repeat(auto-fill,minmax(min(260px,100%),1fr))] gap-3">
            <article
                v-for="card in section.cards"
                :key="card.template.id"
                :data-kind="card.assigned?.kind ?? 'template'"
                class="card-box overflow-hidden flex flex-col"
            >
                <TemplatePreview :template="card.template" :values="sample(card.template)">
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
                        <button
                            v-if="isAdmin"
                            class="btn-quiet min-h-9 text-error"
                            @click="openDeletion(card.template)"
                        >
                            {{ t('graphics.library.delete') }}
                        </button>
                        <RouterLink
                            v-if="isAdmin"
                            :to="editTarget(card.template, card.assigned)"
                            class="btn-quiet min-h-9"
                        >
                            {{ t('graphics.composer.edit') }}
                        </RouterLink>
                        <label
                            v-if="card.assigned && ['logo', 'ticker', 'permanent'].includes(card.assigned.kind)"
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
