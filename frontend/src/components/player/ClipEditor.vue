<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { cloneDeep } from 'es-toolkit/object'

import LogoMarksEditor from '@/components/storage/LogoMarksEditor.vue'
import { isRemoteSource, parseLength, preciseLength, storageLabel } from '@/composables/player'
import { useConfig } from '@/stores/config'

const props = defineProps<{
    item: PlaylistItem
    storage: string
    /** A new remote entry, which needs an address and a length. */
    remote?: boolean
    /** From a commercials folder: always a commercial. */
    commercial?: boolean
}>()

const emit = defineEmits<{
    apply: [item: PlaylistItem, parts: number]
    cancel: []
    pick: [target: 'source' | 'audio']
}>()

const { t } = useI18n()

const draft = ref<PlaylistItem>(cloneDeep(props.item))
const inText = ref(preciseLength(props.item.in))
const outText = ref(preciseLength(props.item.out))
const durationText = ref(preciseLength(props.item.duration))
const showAdvanced = ref(Boolean(props.item.custom_filter))
const parts = ref(1)
const configStore = useConfig()
/** The stretches without logo of the video are open in their editor. */
const editingLogo = ref(false)

/** The parent swaps the file or the audio after a pick. */
watch(
    () => [props.item.source, props.item.duration, props.item.audio],
    () => {
        draft.value.source = props.item.source
        draft.value.audio = props.item.audio

        if (draft.value.duration !== props.item.duration) {
            draft.value.duration = props.item.duration
            draft.value.in = 0
            draft.value.out = props.item.duration
            inText.value = preciseLength(0)
            outText.value = preciseLength(props.item.duration)
            durationText.value = preciseLength(props.item.duration)
        }
    },
)

const isRemote = computed(() => props.remote || isRemoteSource(draft.value.source))
/** The video's path in the storage, for its stretches without logo. */
const storagePath = computed(() => {
    const root = props.storage.replace(/\/+$/, '')

    return root && draft.value.source.startsWith(`${root}/`) ? draft.value.source.slice(root.length + 1) : ''
})
const parsedIn = computed(() => parseLength(inText.value))
const parsedOut = computed(() => parseLength(outText.value))
const parsedDuration = computed(() => (isRemote.value ? parseLength(durationText.value) : draft.value.duration))

const error = computed(() => {
    if (isRemote.value && !isRemoteSource(draft.value.source)) {
        return t('onair.errorAddress')
    }

    if (parsedIn.value === null || parsedOut.value === null || parsedDuration.value === null) {
        return t('onair.errorTime')
    }

    if (parsedOut.value <= parsedIn.value) {
        return t('onair.errorOrder')
    }

    if (parsedDuration.value > 0 && parsedOut.value > parsedDuration.value + 0.05) {
        return t('onair.errorLength', { length: preciseLength(parsedDuration.value) })
    }

    return ''
})

const bar = computed(() => {
    const length = parsedDuration.value || 0
    const start = parsedIn.value ?? 0
    const end = parsedOut.value ?? length

    if (length <= 0) {
        return { left: 0, width: 100, onAir: Math.max(0, end - start), length: Math.max(0, end - start) }
    }

    return {
        left: Math.min(100, (start / length) * 100),
        width: Math.max(0, Math.min(100, ((end - start) / length) * 100)),
        onAir: Math.max(0, end - start),
        length,
    }
})

function apply() {
    if (error.value) {
        return
    }

    const item = cloneDeep(draft.value)
    item.in = parsedIn.value ?? 0
    item.out = parsedOut.value ?? item.duration

    if (isRemote.value) {
        item.duration = parsedDuration.value ?? item.out
    }

    item.title = item.title?.trim() || null
    item.custom_filter = item.custom_filter?.trim() || undefined
    item.audio = item.audio?.trim() || undefined

    emit('apply', item, Math.max(1, Math.min(50, Math.round(parts.value) || 1)))
}
</script>
<template>
    <div class="flex flex-col gap-3.5" @keydown.esc.stop="emit('cancel')">
        <div class="flex flex-wrap items-center gap-x-2.5 gap-y-1">
            <span class="font-semibold">{{ remote ? t('onair.newRemote') : t('onair.editingClip') }}</span>
            <span v-if="!isRemote" class="text-muted truncate max-w-full">{{
                storageLabel(draft.source, storage)
            }}</span>
            <button v-if="!isRemote" class="link-accent min-h-8" @click="emit('pick', 'source')">
                {{ t('onair.replaceFile') }}
            </button>
        </div>

        <div class="grid grid-cols-2 md:grid-cols-4 gap-3">
            <label v-if="isRemote" class="field-label col-span-2 md:col-span-4">
                {{ t('onair.address') }}
                <input
                    v-model="draft.source"
                    class="field-input font-mono text-[13px]"
                    placeholder="https://… / srt://… / rtmp://…"
                    spellcheck="false"
                />
            </label>
            <label class="field-label col-span-2">
                {{ t('onair.guideTitle') }}
                <input v-model="draft.title" class="field-input" :placeholder="t('onair.guideTitlePlaceholder')" />
            </label>
            <label class="field-label">
                {{ t('onair.in') }}
                <input v-model="inText" class="field-input font-mono" inputmode="decimal" />
            </label>
            <label class="field-label">
                {{ t('onair.out') }}
                <input v-model="outText" class="field-input font-mono" inputmode="decimal" />
            </label>
            <label v-if="isRemote" class="field-label">
                {{ t('onair.length') }}
                <input v-model="durationText" class="field-input font-mono" inputmode="decimal" />
            </label>
        </div>

        <div class="flex flex-col gap-1.5">
            <div class="relative h-8.5 bg-field border border-line-strong rounded-field overflow-hidden">
                <div
                    class="absolute inset-y-0 bg-accent-soft border-x-4 border-accent"
                    :style="{ left: `${bar.left}%`, width: `${bar.width}%` }"
                />
                <span class="absolute left-3 top-1.5 font-mono text-xs text-accent-text">
                    {{ t('onair.goesOnAir', { onAir: preciseLength(bar.onAir), length: preciseLength(bar.length) }) }}
                </span>
            </div>
            <div class="flex justify-between font-mono text-[11px] text-muted">
                <span>{{ preciseLength(0) }}</span>
                <span>{{ preciseLength(bar.length / 2) }}</span>
                <span>{{ preciseLength(bar.length) }}</span>
            </div>
        </div>

        <p v-if="error" class="m-0 text-sm text-error" role="alert">{{ error }}</p>
        <p v-if="commercial" id="clip-commercial-hint" class="m-0 text-[13px] text-muted">
            {{ t('onair.commercialFromFolder') }}
        </p>

        <div class="flex flex-wrap items-center gap-x-4.5 gap-y-2">
            <label v-if="commercial" class="flex items-center gap-2.5 min-h-10 text-muted">
                <input
                    type="checkbox"
                    role="switch"
                    class="toggle toggle-sm toggle-accent"
                    checked
                    disabled
                    aria-describedby="clip-commercial-hint"
                />
                {{ t('onair.commercial') }}
            </label>
            <label v-else class="flex items-center gap-2.5 min-h-10 cursor-pointer">
                <input v-model="draft.ad" type="checkbox" role="switch" class="toggle toggle-sm toggle-accent" />
                {{ t('onair.commercial') }}
            </label>
            <div class="flex items-center gap-2.5 min-h-10 text-muted">
                {{ t('onair.separateAudio') }}
                <template v-if="draft.audio">
                    <span class="text-base-content truncate max-w-48" :title="draft.audio">{{
                        storageLabel(draft.audio, storage)
                    }}</span>
                    <button
                        class="btn-icon min-w-8 min-h-8"
                        :aria-label="t('onair.removeAudio')"
                        @click="draft.audio = ''"
                    >
                        <i class="bi bi-x-lg" aria-hidden="true" />
                    </button>
                </template>
                <button v-else class="btn-quiet min-h-9" @click="emit('pick', 'audio')">
                    {{ t('onair.chooseFile') }}
                </button>
            </div>
            <button
                v-if="storagePath && !isRemote && !commercial && !draft.ad"
                class="btn-quiet min-h-9"
                :title="t('storage.logoMarks.intro')"
                @click="editingLogo = true"
            >
                <i class="bi bi-badge-tm" aria-hidden="true" />{{ t('storage.logoMarks.title') }}
            </button>
            <button
                class="flex items-center gap-1.5 min-h-10 text-muted bg-transparent border-0 cursor-pointer"
                :aria-expanded="showAdvanced"
                @click="showAdvanced = !showAdvanced"
            >
                <i
                    class="bi text-xs"
                    :class="showAdvanced ? 'bi-chevron-down' : 'bi-chevron-right'"
                    aria-hidden="true"
                />
                {{ t('onair.advanced') }}
            </button>
            <div class="flex-auto" />
            <button class="btn-quiet" @click="emit('cancel')">{{ t('cancel') }}</button>
            <button class="btn-main bg-base-content! text-base-100!" :disabled="!!error" @click="apply">
                {{ t('onair.apply') }}
            </button>
        </div>

        <div v-if="showAdvanced" class="grid grid-cols-1 md:grid-cols-[minmax(0,3fr)_minmax(0,1fr)] gap-3">
            <label class="field-label">
                {{ t('onair.customFilter') }}
                <input
                    v-model="draft.custom_filter"
                    class="field-input font-mono text-[13px]"
                    placeholder="[c_v_in]eq=contrast=1.1[c_v_out]"
                    spellcheck="false"
                />
                <span class="field-hint">{{ t('onair.customFilterHint') }}</span>
            </label>
            <label class="field-label">
                {{ t('onair.splitParts') }}
                <input v-model.number="parts" type="number" min="1" max="50" class="field-input font-mono" />
                <span class="field-hint">{{ t('onair.splitHint') }}</span>
            </label>
        </div>

        <LogoMarksEditor
            v-if="editingLogo && configStore.channels[configStore.i]?.id"
            :channel-id="configStore.channels[configStore.i]!.id"
            :path="storagePath"
            :length="draft.duration"
            @close="editingLogo = false"
        />
    </div>
</template>
