<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import TemplatePreview from '@/components/graphics/TemplatePreview.vue'
import { AuthFetchError } from '@/composables/authFetch'
import { type GraphicOnAir, type GraphicsTemplate, fieldList, useGraphicsApi } from '@/composables/graphicsTemplates'
import { useConfig } from '@/stores/config'
import { useIndex } from '@/stores/index'

/** Grafismo › Modelos: the channel's templates as cards, filtered by group. */
const { t } = useI18n()
const configStore = useConfig()
const indexStore = useIndex()
const graphicsApi = useGraphicsApi()

const templates = ref<GraphicsTemplate[]>([])
const onAir = ref<GraphicOnAir[]>([])
const loaded = ref(false)
const group = ref<string | null>(null)

const channelId = computed(() => configStore.channels[configStore.i]?.id)
const groups = computed(() => [...new Set(templates.value.map((template) => template.group))].filter(Boolean))
const shown = computed(() =>
    group.value === null ? templates.value : templates.value.filter((template) => template.group === group.value),
)

/** Sample values so a card shows what the template looks like. */
function sample(template: GraphicsTemplate): Record<string, string> {
    return Object.fromEntries(template.fields.map((field) => [field, `[${field}]`]))
}

onMounted(async () => {
    try {
        ;[templates.value, onAir.value] = await Promise.all([graphicsApi.templates(), graphicsApi.onAir()])
    } catch (error) {
        indexStore.msgAlert(
            'error',
            error instanceof AuthFetchError && typeof error.data === 'string'
                ? error.data
                : t('graphics.templates.loadFailed'),
            4,
        )
    } finally {
        loaded.value = true
    }
})
</script>
<template>
    <div class="flex flex-wrap items-start justify-between gap-3">
        <div>
            <h1 class="m-0 text-xl font-semibold">{{ t('graphics.sections.templates') }}</h1>
            <p class="mt-1 mb-0 text-muted">{{ t('graphics.intro.templates') }}</p>
        </div>
        <button type="button" class="btn-main" disabled :title="t('graphics.library.composerSoon')">
            + {{ t('graphics.library.new') }}
        </button>
    </div>

    <div class="flex flex-wrap items-center gap-2" role="group" :aria-label="t('graphics.library.filter')">
        <button
            type="button"
            class="min-h-9 px-3 rounded-full border text-sm cursor-pointer"
            :class="group === null ? 'border-accent bg-base-300' : 'border-line-strong'"
            :aria-pressed="group === null"
            @click="group = null"
        >
            {{ t('graphics.library.all') }}
        </button>
        <button
            v-for="name in groups"
            :key="name"
            type="button"
            class="min-h-9 px-3 rounded-full border text-sm cursor-pointer"
            :class="group === name ? 'border-accent bg-base-300' : 'border-line-strong'"
            :aria-pressed="group === name"
            @click="group = name"
        >
            {{ name }}
        </button>
        <span class="ml-auto text-sm text-muted">{{ t('graphics.onAir.count', { count: shown.length }) }}</span>
    </div>

    <p v-if="loaded && !templates.length" class="m-0 text-muted">{{ t('graphics.templates.none') }}</p>

    <div class="grid grid-cols-[repeat(auto-fill,minmax(260px,1fr))] gap-3">
        <article v-for="template in shown" :key="template.id" class="card-box overflow-hidden flex flex-col">
            <TemplatePreview
                :template="template"
                :values="sample(template)"
                class="bg-[linear-gradient(180deg,#5b7da6_0%,#3f5f86_55%,#3f6b2f_55%,#2f5524_100%)]"
            >
                <span
                    v-if="onAir.some((graphic) => graphic.template_id === template.id && !graphic.leaving)"
                    class="absolute right-2 top-2 z-10 px-2 py-0.5 rounded-full bg-error text-error-content text-[11px] font-bold"
                >
                    {{ t('graphics.onAir.badge') }}
                </span>
            </TemplatePreview>
            <div class="px-4 py-3 flex flex-col gap-1.5 flex-auto">
                <div class="flex items-baseline justify-between gap-2">
                    <h2 class="m-0 text-sm font-semibold">{{ template.name }}</h2>
                    <span class="text-xs text-muted">{{ template.group || t('graphics.templates.noGroup') }}</span>
                </div>
                <span class="text-xs text-muted">
                    {{ fieldList(template) || t('graphics.templates.noFields') }} ·
                    {{
                        template.duration
                            ? t('graphics.library.seconds', { seconds: template.duration })
                            : t('graphics.library.untilOut')
                    }}
                </span>
                <span class="text-xs text-muted">{{ t(`graphics.templates.lshape.${template.during_lshape}`) }}</span>
                <div class="flex gap-2 mt-auto pt-2">
                    <RouterLink
                        :to="{ name: 'graphics-onair', query: { channel: channelId, template: template.id } }"
                        class="btn-main min-h-9"
                    >
                        {{ t('graphics.library.toAir') }}
                    </RouterLink>
                </div>
            </div>
        </article>
    </div>
</template>
