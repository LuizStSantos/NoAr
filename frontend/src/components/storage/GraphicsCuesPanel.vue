<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import { type GraphicsTemplate, useGraphicsApi } from '@/composables/graphicsTemplates'
import type { GraphicsCue } from '@/composables/storage'
import { useGraphicsDisplayNames } from '@/composables/graphicsDisplayNames'

/**
 * The graphics cues of a video: at a time of the video, put a template on
 * air (for a time, or until taken off) or take one off.
 */
const props = defineProps<{ position: number; duration: number }>()
const cues = defineModel<GraphicsCue[]>('cues', { required: true })
const emit = defineEmits<{ seek: [seconds: number] }>()

const { t } = useI18n()
const { templateName } = useGraphicsDisplayNames()
const api = useGraphicsApi()
const templates = ref<GraphicsTemplate[]>([])
const loadFailed = ref(false)
const drafts = ref<Record<number, string>>({})

const sorted = computed(() => cues.value.map((cue, index) => ({ cue, index })).sort((a, b) => a.cue.at - b.cue.at))

function formatTime(seconds: number): string {
    const whole = Math.max(0, seconds)
    const minutes = Math.floor(whole / 60)
    const rest = whole - minutes * 60

    return `${minutes}:${rest.toFixed(1).padStart(4, '0')}`
}

/** `m:ss.s`, `h:mm:ss` or seconds. */
function parseTime(text: string): number | null {
    const parts = text.trim().replace(',', '.').split(':')

    if (!parts.length || parts.some((part) => part === '' || !Number.isFinite(Number(part)))) {
        return null
    }

    const seconds = parts.reduce((total, part) => total * 60 + Number(part), 0)

    return seconds >= 0 ? seconds : null
}

function add(action: GraphicsCue['action']) {
    const first = templates.value[0]

    if (!first) {
        return
    }

    cues.value = [
        ...cues.value,
        {
            at: Math.round(props.position * 10) / 10,
            action,
            template_id: first.id,
            duration: action === 'take' ? 0 : null,
        },
    ]
}

function remove(index: number) {
    cues.value = cues.value.filter((_, at) => at !== index)
}

function update(index: number, patch: Partial<GraphicsCue>) {
    cues.value = cues.value.map((cue, at) => (at === index ? { ...cue, ...patch } : cue))
}

function setTime(index: number, text: string) {
    drafts.value[index] = text
    const seconds = parseTime(text)

    if (seconds !== null) {
        update(index, { at: seconds })
    }
}

function settleTime(index: number) {
    delete drafts.value[index]
}

function setDuration(index: number, text: string) {
    const seconds = Number(text.replace(',', '.'))

    update(index, { duration: Number.isFinite(seconds) && seconds > 0 ? seconds : 0 })
}

function name(id: number): string {
    const template = templates.value.find((item) => item.id === id)

    return template ? templateName(template) : t('storage.graphicsCues.missing', { id })
}

onMounted(async () => {
    try {
        templates.value = await api.templates()
    } catch {
        loadFailed.value = true
    }
})

defineExpose({ name })
</script>
<template>
    <section class="flex flex-col gap-3" :aria-label="t('storage.graphicsCues.title')">
        <p class="m-0 text-sm text-muted">{{ t('storage.graphicsCues.intro') }}</p>
        <p v-if="loadFailed" role="alert" class="m-0 text-sm text-error">{{ t('graphics.templates.loadFailed') }}</p>
        <p v-else-if="!templates.length" class="m-0 text-sm text-muted">{{ t('graphics.templates.none') }}</p>

        <div class="flex flex-wrap gap-2">
            <button type="button" class="btn-main" :disabled="!templates.length" @click="add('take')">
                + {{ t('storage.graphicsCues.addTake', { time: formatTime(position) }) }}
            </button>
            <button type="button" class="btn-quiet" :disabled="!templates.length" @click="add('out')">
                + {{ t('storage.graphicsCues.addOut', { time: formatTime(position) }) }}
            </button>
        </div>

        <p v-if="!cues.length" class="m-0 text-sm text-muted">{{ t('storage.graphicsCues.none') }}</p>
        <ul v-else class="m-0 p-0 list-none flex flex-col gap-2">
            <li
                v-for="{ cue, index } in sorted"
                :key="index"
                class="grid grid-cols-[repeat(auto-fit,minmax(120px,1fr))] items-end gap-2 rounded-field border border-line-strong bg-base-100 p-2.5"
                :class="cue.at > duration + 0.05 && duration > 0 ? 'border-error' : ''"
            >
                <label class="field-label">
                    {{ t('storage.graphicsCues.at') }}
                    <input
                        class="field-input font-mono"
                        :value="drafts[index] ?? formatTime(cue.at)"
                        inputmode="decimal"
                        @input="setTime(index, ($event.target as HTMLInputElement).value)"
                        @blur="settleTime(index)"
                    />
                </label>
                <label class="field-label">
                    {{ t('storage.graphicsCues.action') }}
                    <select
                        class="field-input"
                        :value="cue.action"
                        @change="
                            update(index, {
                                action: ($event.target as HTMLSelectElement).value as GraphicsCue['action'],
                            })
                        "
                    >
                        <option value="take">{{ t('storage.graphicsCues.take') }}</option>
                        <option value="out">{{ t('storage.graphicsCues.out') }}</option>
                    </select>
                </label>
                <label class="field-label">
                    {{ t('storage.graphicsCues.template') }}
                    <select
                        class="field-input"
                        :value="cue.template_id"
                        @change="update(index, { template_id: Number(($event.target as HTMLSelectElement).value) })"
                    >
                        <option v-if="!templates.some((item) => item.id === cue.template_id)" :value="cue.template_id">
                            {{ name(cue.template_id) }}
                        </option>
                        <option v-for="template in templates" :key="template.id" :value="template.id">
                            {{ templateName(template) }}
                        </option>
                    </select>
                </label>
                <label v-if="cue.action === 'take'" class="field-label">
                    {{ t('storage.graphicsCues.duration') }}
                    <input
                        class="field-input font-mono"
                        :value="cue.duration || 0"
                        inputmode="decimal"
                        @change="setDuration(index, ($event.target as HTMLInputElement).value)"
                    />
                </label>
                <div class="flex gap-1">
                    <button type="button" class="btn-quiet min-h-9" @click="emit('seek', cue.at)">
                        {{ t('storage.graphicsCues.go') }}
                    </button>
                    <button
                        type="button"
                        class="btn-icon min-h-9"
                        :aria-label="t('storage.graphicsCues.remove')"
                        @click="remove(index)"
                    >
                        <i class="bi bi-x-lg" aria-hidden="true" />
                    </button>
                </div>
            </li>
        </ul>
        <p class="m-0 text-xs text-muted">{{ t('storage.graphicsCues.hint') }}</p>
    </section>
</template>
