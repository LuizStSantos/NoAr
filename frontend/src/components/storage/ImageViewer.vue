<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useEventListener } from '@vueuse/core'

import { useDialogFocus } from '@/composables/dialogFocus'
import { stringFormatter } from '@/composables/helper'

/** One image of the open folder, with the address it can be shown from. */
export interface ViewerImage {
    key: string
    name: string
    url: string | null
    size: number
    /** Width × height once the image has loaded. */
    dims: string | null
}

/** The open folder's images one at a time, in a full-screen viewer. */
const props = defineProps<{
    images: ViewerImage[]
    folder: string
    /** Name of the image that is the folder's poster or wide image, if any. */
    poster: string | null
    backdrop: string | null
    /** Whose artwork the folder holds: a series, a film or just a folder. */
    owner: 'series' | 'movie' | 'folder'
    /** Using an image as artwork is offered (not in the storage root). */
    canUse: boolean
    busy: boolean
}>()

const index = defineModel<number>({ required: true })

const emit = defineEmits<{
    close: []
    use: [kind: ArtworkKind, name: string]
    loaded: [key: string, dims: string]
}>()

const { t } = useI18n()
const { fileSize } = stringFormatter()

const dialog = ref<HTMLElement>()
const strip = ref<HTMLElement>()
const image = computed(() => props.images[index.value])

const details = computed(() => {
    const current = image.value

    if (!current) {
        return ''
    }

    return [
        current.dims,
        fileSize(current.size, 1) || '0 B',
        props.folder,
        t('storage.images.position', { index: index.value + 1, total: props.images.length }),
    ]
        .filter(Boolean)
        .join(' · ')
})

function go(step: number) {
    const total = props.images.length

    if (total) {
        index.value = (index.value + step + total) % total
    }
}

function loaded(event: Event, item: ViewerImage) {
    const element = event.target as HTMLImageElement

    if (!item.dims && element.naturalWidth) {
        emit('loaded', item.key, `${element.naturalWidth}×${element.naturalHeight}`)
    }
}

useDialogFocus(dialog, { onEscape: () => emit('close') })

useEventListener(window, 'keydown', (event: KeyboardEvent) => {
    if (event.key === 'ArrowLeft') {
        go(-1)
    } else if (event.key === 'ArrowRight') {
        go(1)
    }
})

// Keep the chosen thumbnail in sight when moving through a long folder.
watch(index, async () => {
    await nextTick()
    strip.value?.querySelector('[aria-current="true"]')?.scrollIntoView({ block: 'nearest', inline: 'nearest' })
})

onMounted(() => {
    document.body.style.overflow = 'hidden'
})

onBeforeUnmount(() => {
    document.body.style.overflow = ''
})
</script>
<template>
    <div
        ref="dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="image-viewer-title"
        tabindex="-1"
        class="fixed inset-0 z-50 bg-[#0b0c0d] text-white flex flex-col outline-none"
    >
        <div class="flex flex-wrap items-center gap-x-3 gap-y-2 px-4 sm:px-6 py-3 border-b border-white/10">
            <div class="flex-[1_1_280px] min-w-0">
                <h2 id="image-viewer-title" class="m-0 text-base font-semibold truncate">{{ image?.name }}</h2>
                <div class="font-mono text-xs text-white/60 truncate">{{ details }}</div>
            </div>
            <template v-if="image && canUse">
                <span v-if="image.name === poster" class="inline-flex items-center gap-1.5 text-[13px] text-ok">
                    <i class="bi bi-check2" aria-hidden="true" />{{ t(`storage.images.isPoster.${owner}`) }}
                </span>
                <button
                    v-else
                    type="button"
                    class="btn-quiet min-h-10 text-white border-white/25"
                    :disabled="busy"
                    @click="emit('use', 'poster', image.name)"
                >
                    {{ t('storage.images.usePoster') }}
                </button>
                <span v-if="image.name === backdrop" class="inline-flex items-center gap-1.5 text-[13px] text-ok">
                    <i class="bi bi-check2" aria-hidden="true" />{{ t(`storage.images.isBackdrop.${owner}`) }}
                </span>
                <button
                    v-else
                    type="button"
                    class="btn-quiet min-h-10 text-white border-white/25"
                    :disabled="busy"
                    @click="emit('use', 'backdrop', image.name)"
                >
                    {{ t('storage.images.useBackdrop') }}
                </button>
            </template>
            <a
                v-if="image?.url"
                :href="image.url"
                :download="image.name"
                class="btn-quiet min-h-10 text-white border-white/25 no-underline"
            >
                {{ t('storage.images.download') }}
            </a>
            <button
                type="button"
                class="btn-icon text-white/80"
                :aria-label="t('storage.images.close')"
                @click="emit('close')"
            >
                <i class="bi bi-x-lg" aria-hidden="true" />
            </button>
        </div>

        <div class="flex-1 min-h-0 flex items-center gap-3 sm:gap-4 px-3 sm:px-6 py-4">
            <button
                type="button"
                class="size-12 shrink-0 rounded-full border border-white/20 bg-white/5 text-white flex items-center justify-center cursor-pointer disabled:opacity-30"
                :aria-label="t('storage.images.previous')"
                :disabled="images.length < 2"
                @click="go(-1)"
            >
                <i class="bi bi-chevron-left" aria-hidden="true" />
            </button>
            <div class="flex-1 h-full min-w-0 flex items-center justify-center">
                <img
                    v-if="image?.url"
                    :key="image.key"
                    :src="image.url"
                    :alt="image.name"
                    class="max-w-full max-h-full object-contain"
                    @load="loaded($event, image)"
                />
                <span v-else class="loading loading-spinner loading-lg text-white/60" />
            </div>
            <button
                type="button"
                class="size-12 shrink-0 rounded-full border border-white/20 bg-white/5 text-white flex items-center justify-center cursor-pointer disabled:opacity-30"
                :aria-label="t('storage.images.next')"
                :disabled="images.length < 2"
                @click="go(1)"
            >
                <i class="bi bi-chevron-right" aria-hidden="true" />
            </button>
        </div>

        <div class="flex items-center gap-3 px-4 sm:px-6 pt-3 pb-4 border-t border-white/10">
            <ul
                ref="strip"
                :aria-label="t('storage.images.strip')"
                class="m-0 p-0.5 list-none flex gap-2 flex-1 min-w-0 overflow-x-auto"
            >
                <li v-for="(item, position) in images" :key="item.key" class="shrink-0">
                    <button
                        type="button"
                        class="h-16 min-w-12 max-w-28 rounded-md overflow-hidden border-2 p-0 bg-white/5 cursor-pointer flex items-center justify-center"
                        :class="position === index ? 'border-accent' : 'border-transparent'"
                        :aria-label="item.name"
                        :aria-current="position === index"
                        @click="index = position"
                    >
                        <img v-if="item.url" :src="item.url" alt="" class="h-full w-auto object-cover" loading="lazy" />
                        <i v-else class="bi bi-image text-white/40" aria-hidden="true" />
                    </button>
                </li>
            </ul>
            <span class="hidden sm:inline text-[13px] text-white/60 whitespace-nowrap">{{
                t('storage.images.keys')
            }}</span>
        </div>
    </div>
</template>
