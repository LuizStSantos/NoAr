<script setup lang="ts">
import { useI18n } from 'vue-i18n'

import { stringFormatter } from '@/composables/helper'
import { storageImageKey } from '@/composables/storageImages'

/** Presentation only: folder state, access tickets and commands belong to the page. */
defineProps<{
    images: FileObject[]
    imageUrls: ReadonlyMap<string, string>
    imageDims: ReadonlyMap<string, string>
    loading: boolean
    downloading: boolean
    filter: string
    posterHere: string | null
    backdropHere: string | null
    owner: 'series' | 'movie' | 'folder'
}>()

const emit = defineEmits<{
    drag: [event: DragEvent, file: FileObject]
    open: [name: string]
    loaded: [event: Event, file: FileObject]
    failed: [file: FileObject]
    download: [file: FileObject]
    rename: [file: FileObject]
    remove: [name: string]
}>()

const { t } = useI18n()
const { fileSize } = stringFormatter()
</script>
<template>
    <div class="relative">
        <div v-if="loading" class="absolute inset-0 z-10 flex justify-center pt-10 bg-base-100/60">
            <span class="loading loading-spinner loading-lg" />
        </div>
        <ul v-if="images.length" class="m-0 p-4 list-none grid grid-cols-[repeat(auto-fill,minmax(180px,1fr))] gap-3.5">
            <li
                v-for="file in images"
                :key="file.name"
                class="group relative bg-raised border border-line rounded-xl overflow-hidden hover:border-line-strong focus-within:border-accent"
                draggable="true"
                @dragstart="emit('drag', $event, file)"
            >
                <button
                    type="button"
                    class="w-full h-[150px] bg-base-300 border-0 p-0 cursor-pointer flex items-center justify-center"
                    :aria-label="t('storage.images.view', { name: file.name })"
                    @click="emit('open', file.name)"
                >
                    <img
                        v-if="imageUrls.get(storageImageKey(file))"
                        :src="imageUrls.get(storageImageKey(file))"
                        alt=""
                        class="max-w-full max-h-full object-contain"
                        loading="lazy"
                        @load="emit('loaded', $event, file)"
                        @error="emit('failed', file)"
                    />
                    <i v-else class="bi bi-image text-2xl text-muted" aria-hidden="true" />
                </button>
                <div
                    class="absolute top-2 right-2 flex gap-1 opacity-100 sm:opacity-0 sm:group-hover:opacity-100 sm:group-focus-within:opacity-100"
                >
                    <button
                        type="button"
                        class="btn-icon bg-base-100/90 text-base-content"
                        :aria-label="t('storage.images.view', { name: file.name })"
                        @click="emit('open', file.name)"
                    >
                        <i class="bi bi-eye" aria-hidden="true" />
                    </button>
                    <button
                        type="button"
                        class="btn-icon bg-base-100/90 text-base-content"
                        :aria-label="t('storage.download', { name: file.name })"
                        :title="t('storage.download', { name: file.name })"
                        :disabled="downloading"
                        @click="emit('download', file)"
                    >
                        <i class="bi bi-download" aria-hidden="true" />
                    </button>
                    <button
                        type="button"
                        class="btn-icon bg-base-100/90 text-base-content"
                        :aria-label="t('storage.rename', { name: file.name })"
                        @click="emit('rename', file)"
                    >
                        <i class="bi bi-pencil" aria-hidden="true" />
                    </button>
                    <button
                        type="button"
                        class="btn-icon bg-base-100/90 text-error"
                        :aria-label="t('storage.remove', { name: file.name })"
                        @click="emit('remove', file.name)"
                    >
                        <i class="bi bi-trash" aria-hidden="true" />
                    </button>
                </div>
                <div class="px-3 py-2.5 flex flex-col gap-1 text-[13px]">
                    <span class="truncate" :title="file.name">{{ file.name }}</span>
                    <span class="font-mono text-xs text-muted">{{
                        [imageDims.get(storageImageKey(file)), fileSize(file.size ?? 0, 1) || '0 B']
                            .filter(Boolean)
                            .join(' · ')
                    }}</span>
                    <span
                        v-if="file.name === posterHere"
                        class="self-start text-[11px] text-ok border border-ok/40 rounded-md px-1.5 leading-4"
                    >
                        {{ t(`storage.images.posterBadge.${owner}`) }}
                    </span>
                    <span
                        v-if="file.name === backdropHere"
                        class="self-start text-[11px] text-ok border border-ok/40 rounded-md px-1.5 leading-4"
                    >
                        {{ t('storage.images.backdropBadge') }}
                    </span>
                </div>
            </li>
        </ul>
        <p v-else-if="filter" class="m-0 px-4 py-6 text-muted">
            {{ t('storage.noMatch', { filter }) }}
        </p>
        <p v-else-if="!loading" class="m-0 px-4 pt-6 pb-2 text-muted">
            {{ t('storage.images.empty') }} {{ t('storage.images.emptyHint') }}
        </p>
    </div>
</template>
