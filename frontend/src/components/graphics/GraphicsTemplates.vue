<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, reactive, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import SectionCard from '@/components/settings/SectionCard.vue'
import { AuthFetchError, authFetch } from '@/composables/authFetch'
import { useAuth } from '@/stores/auth'
import { useConfig } from '@/stores/config'
import { useIndex } from '@/stores/index'

/**
 * Grafismo › Live now: the channel's templates, their fields, and the
 * graphics on air, taken and taken out with one click.
 */
interface Template {
    id: number
    name: string
    group: string
    duration: number
    during_lshape: 'follow' | 'wait' | 'skip'
    fields: string[]
}

interface OnAir {
    take_id: number
    template_id: number
    name: string
    since: number
    leaving: boolean
}

const { t } = useI18n()
const authStore = useAuth()
const configStore = useConfig()
const indexStore = useIndex()

const templates = ref<Template[]>([])
const onAir = ref<OnAir[]>([])
const loaded = ref(false)
const selectedId = ref<number | null>(null)
const values = reactive<Record<string, string>>({})
const duration = ref('')
const busy = ref(false)
let timer: ReturnType<typeof setInterval> | undefined

const channelId = computed(() => configStore.channels[configStore.i]?.id)
const selected = computed(() => templates.value.find((template) => template.id === selectedId.value))
const groups = computed(() => {
    const byGroup = new Map<string, Template[]>()

    for (const template of templates.value) {
        const group = template.group || t('graphics.templates.noGroup')
        byGroup.set(group, [...(byGroup.get(group) ?? []), template])
    }

    return [...byGroup.entries()].map(([name, items]) => ({ name, items }))
})

function headers() {
    return { ...configStore.contentType, ...authStore.authHeader }
}

function message(error: unknown, fallback: string): string {
    return error instanceof AuthFetchError && typeof error.data === 'string' && error.data ? error.data : fallback
}

async function loadOnAir() {
    try {
        onAir.value = await authFetch<OnAir[]>(`/api/graphics/${channelId.value}/on-air`, { headers: headers() })
    } catch {
        // The next poll tries again; the list keeps what it showed.
    }
}

async function load() {
    try {
        templates.value = await authFetch<Template[]>(`/api/graphics/${channelId.value}/templates`, {
            headers: headers(),
        })
    } catch (error) {
        indexStore.msgAlert('error', message(error, t('graphics.templates.loadFailed')), 4)
    } finally {
        loaded.value = true
    }

    await loadOnAir()
}

function select(template: Template) {
    selectedId.value = template.id
    duration.value = String(template.duration)

    for (const key of Object.keys(values)) {
        if (!template.fields.includes(key)) {
            delete values[key]
        }
    }
}

async function take() {
    const template = selected.value

    if (!template) {
        return
    }

    const seconds = Number(duration.value.replace(',', '.'))
    busy.value = true

    try {
        const result = await authFetch<{ state: 'on_air' | 'waiting' }>(`/api/graphics/${channelId.value}/take`, {
            method: 'POST',
            headers: headers(),
            body: JSON.stringify({
                template_id: template.id,
                fields: Object.fromEntries(template.fields.map((field) => [field, values[field] ?? ''])),
                duration: Number.isFinite(seconds) && duration.value.trim() !== '' ? seconds : undefined,
            }),
        })
        indexStore.msgAlert(
            'success',
            t(result.state === 'waiting' ? 'graphics.templates.waiting' : 'graphics.templates.taken', {
                name: template.name,
            }),
            3,
        )
        await loadOnAir()
    } catch (error) {
        indexStore.msgAlert('error', message(error, t('graphics.templates.takeFailed')), 5)
    } finally {
        busy.value = false
    }
}

async function takeOut(takeId?: number) {
    try {
        await authFetch(`/api/graphics/${channelId.value}/out`, {
            method: 'POST',
            headers: headers(),
            body: JSON.stringify(takeId === undefined ? {} : { take_id: takeId }),
        })
        await loadOnAir()
    } catch (error) {
        indexStore.msgAlert('error', message(error, t('graphics.templates.outFailed')), 4)
    }
}

/** The template's fields as `{name} {role}`, or a word for none. */
function fieldList(template: Template): string {
    return template.fields.length
        ? template.fields.map((field) => '{' + field + '}').join(' ')
        : t('graphics.templates.noFields')
}

function since(seconds: number): string {
    const whole = Math.floor(seconds)

    return whole < 60 ? `${whole} s` : `${Math.floor(whole / 60)} min ${String(whole % 60).padStart(2, '0')} s`
}

onMounted(() => {
    load()
    timer = setInterval(loadOnAir, 2000)
})

onBeforeUnmount(() => clearInterval(timer))
</script>
<template>
    <SectionCard :title="t('graphics.templates.title')" :description="t('graphics.templates.intro')">
        <template #aside>
            <button
                type="button"
                class="btn-quiet min-h-11 px-4"
                :disabled="!onAir.some((graphic) => !graphic.leaving)"
                @click="takeOut()"
            >
                {{ t('graphics.templates.allOut') }}
            </button>
        </template>

        <div class="flex flex-col gap-2" role="status" aria-live="polite">
            <span class="text-xs text-muted uppercase tracking-[0.08em]">{{ t('graphics.templates.onAir') }}</span>
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
                        {{ graphic.leaving ? t('graphics.templates.leaving') : since(graphic.since) }}
                    </span>
                    <button
                        type="button"
                        class="btn-quiet min-h-9 px-3 text-sm"
                        :disabled="graphic.leaving"
                        @click="takeOut(graphic.take_id)"
                    >
                        {{ t('graphics.templates.out') }}
                    </button>
                </li>
            </ul>
        </div>

        <p v-if="loaded && !templates.length" class="m-0 text-sm text-muted">
            {{ t('graphics.templates.none') }}
        </p>

        <div v-for="group in groups" :key="group.name" class="flex flex-col gap-2">
            <span class="text-xs text-muted uppercase tracking-[0.08em]">{{ group.name }}</span>
            <div class="grid grid-cols-[repeat(auto-fill,minmax(180px,1fr))] gap-2">
                <button
                    v-for="template in group.items"
                    :key="template.id"
                    type="button"
                    class="cursor-pointer min-h-16 px-3 py-2.5 flex flex-col gap-0.5 border rounded-field text-left bg-base-100"
                    :class="selectedId === template.id ? 'border-accent' : 'border-line-strong hover:border-accent'"
                    :aria-pressed="selectedId === template.id"
                    @click="select(template)"
                >
                    <strong class="font-semibold text-sm">{{ template.name }}</strong>
                    <span class="text-xs text-muted">{{ fieldList(template) }}</span>
                </button>
            </div>
        </div>

        <form
            v-if="selected"
            class="flex flex-col gap-3 border-t border-line pt-4"
            :aria-label="t('graphics.templates.fillLabel', { name: selected.name })"
            @submit.prevent="take"
        >
            <div class="flex flex-wrap items-baseline justify-between gap-2">
                <h3 class="m-0 text-sm font-semibold">{{ selected.name }}</h3>
                <span class="text-xs text-muted">{{ t(`graphics.templates.lshape.${selected.during_lshape}`) }}</span>
            </div>
            <div class="grid grid-cols-[repeat(auto-fill,minmax(220px,1fr))] gap-3">
                <label v-for="field in selected.fields" :key="field" class="field-label">
                    {{ field }}
                    <input v-model="values[field]" class="field-input" type="text" />
                </label>
                <label class="field-label">
                    {{ t('graphics.templates.duration') }}
                    <input v-model="duration" class="field-input font-mono" inputmode="decimal" />
                    <span class="text-xs text-muted">{{ t('graphics.templates.durationHint') }}</span>
                </label>
            </div>
            <div>
                <button type="submit" class="btn-main" :disabled="busy">
                    {{ t('graphics.templates.take') }}
                </button>
            </div>
        </form>
    </SectionCard>
</template>
