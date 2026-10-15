<script setup lang="ts">
import { ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import { baseName, imageUrl } from '@/composables/storage'

/** One image of a sheet (poster, background or clip image): shows it and takes a new one. */
const props = withDefaults(
    defineProps<{
        channelId: number
        /** Storage path of the current image. */
        image: string | null
        label: string
        /** CSS aspect ratio of the frame. */
        ratio: string
        /** File name a new image gets, shown as a hint. */
        target: string
        busy?: boolean
        compact?: boolean
    }>(),
    { busy: false, compact: false },
)

const emit = defineEmits<{ pick: [file: File]; remove: [] }>()

const { t } = useI18n()

const url = ref('')
const input = ref<HTMLInputElement>()
const dragOver = ref(false)

watch(
    () => [props.image, props.channelId] as const,
    async ([image]) => {
        url.value = ''

        if (image) {
            try {
                url.value = await imageUrl(props.channelId, image)
            } catch {
                url.value = ''
            }
        }
    },
    { immediate: true },
)

function choose() {
    input.value?.click()
}

function onChange(event: Event) {
    const file = (event.target as HTMLInputElement).files?.[0]

    if (file) {
        emit('pick', file)
    }

    ;(event.target as HTMLInputElement).value = ''
}

function onDrop(event: DragEvent) {
    dragOver.value = false

    const file = event.dataTransfer?.files?.[0]

    if (file) {
        emit('pick', file)
    }
}
</script>
<template>
    <div class="flex flex-col gap-1.5 text-xs text-muted min-w-0">
        <span>{{ label }}</span>
        <div
            class="relative rounded-[10px] overflow-hidden flex items-center justify-center text-center"
            :class="[
                image ? 'bg-base-300 border border-line-strong' : 'border-[1.5px] border-dashed border-line-strong',
                dragOver ? 'outline-2 outline-accent' : '',
            ]"
            :style="{ aspectRatio: ratio }"
            @dragover.prevent="dragOver = true"
            @dragleave="dragOver = false"
            @drop.prevent="onDrop"
        >
            <img v-if="image && url" :src="url" :alt="label" class="absolute inset-0 w-full h-full object-cover" />
            <span v-else-if="image" class="text-subtle p-2 break-all">{{ baseName(image) }}</span>
            <div v-else class="flex flex-col items-center gap-1.5 p-3">
                <i class="bi bi-upload text-lg" aria-hidden="true" />
                <span v-if="!compact" class="text-sm text-base-content/80">
                    {{ t('storage.artwork.drop') }}
                    <button type="button" class="link-accent" @click="choose">{{ t('storage.artwork.choose') }}</button>
                </span>
                <button v-else type="button" class="link-accent" @click="choose">
                    {{ t('storage.artwork.choose') }}
                </button>
                <span class="text-subtle">{{ t('storage.artwork.savedAs', { name: target }) }}</span>
            </div>
            <div v-if="busy" class="absolute inset-0 bg-base-100/70 flex items-center justify-center">
                <span class="loading loading-spinner" />
            </div>
        </div>
        <div v-if="image" class="flex gap-1.5">
            <button type="button" class="btn-quiet min-h-8.5 flex-1 text-[13px]" :disabled="busy" @click="choose">
                {{ t('storage.artwork.replace') }}
            </button>
            <button
                type="button"
                class="btn-quiet min-h-8.5 min-w-8.5 px-0 text-error"
                :disabled="busy"
                :aria-label="t('storage.artwork.remove', { name: label })"
                @click="emit('remove')"
            >
                <i class="bi bi-trash" aria-hidden="true" />
            </button>
        </div>
        <input
            ref="input"
            type="file"
            class="hidden"
            accept="image/jpeg,image/png,image/webp,image/gif"
            @change="onChange"
        />
    </div>
</template>
