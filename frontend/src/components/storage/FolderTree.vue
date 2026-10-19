<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import { listFolders, storageJoin } from '@/composables/storage'

/**
 * One folder of the storage tree and, when open, its subfolders. Folders load
 * when opened; the branch leading to the current folder opens by itself.
 */
const props = withDefaults(
    defineProps<{
        channelId: number
        /** Storage path of this folder; empty for the storage root. */
        path: string
        label: string
        hasSheet?: boolean
        /** The sheet is a film's, not a series'. */
        movie?: boolean
        /** The folder holds a live. */
        live?: boolean
        /** The folder is a commercials folder. */
        commercial?: boolean
        /** Storage path of the folder shown in the page. */
        current: string
        /** Bumped by the page after changes, so open folders reload. */
        version?: number
        depth?: number
    }>(),
    { hasSheet: false, movie: false, live: false, commercial: false, version: 0, depth: 0 },
)

const emit = defineEmits<{
    open: [path: string]
    drop: [path: string, event: DragEvent]
}>()

const { t } = useI18n()

const open = ref(props.depth === 0 || isOnCurrentBranch())
const loaded = ref(false)
const loading = ref(false)
const folders = ref<string[]>([])
const sheetFolders = ref<string[]>([])
const movieFolders = ref<string[]>([])
const liveFolders = ref<string[]>([])
const commercialFolders = ref<string[]>([])
const dragOver = ref(false)
/** The version this folder last loaded at; passed down so subfolders reload
 * after it, and a removed subfolder disappears instead of reloading. */
const loadedVersion = ref(props.version)

const isCurrent = computed(() => props.current === props.path)

function isOnCurrentBranch(): boolean {
    return props.path === '' || props.current === props.path || props.current.startsWith(`${props.path}/`)
}

async function load() {
    loading.value = true

    try {
        const listing = await listFolders(props.channelId, props.path)

        folders.value = listing.folders
        sheetFolders.value = listing.sheetFolders
        movieFolders.value = listing.movieFolders
        liveFolders.value = listing.liveFolders
        commercialFolders.value = listing.commercialFolders
        loaded.value = true
        loadedVersion.value = props.version
    } catch {
        folders.value = []
    } finally {
        loading.value = false
    }
}

function toggle() {
    open.value = !open.value
}

watch(
    open,
    (isOpen) => {
        if (isOpen && !loaded.value) {
            load()
        }
    },
    { immediate: true },
)

watch(
    () => props.current,
    () => {
        if (isOnCurrentBranch()) {
            open.value = true
        }
    },
)

watch(
    () => [props.version, props.channelId],
    () => {
        if (loaded.value) {
            load()
        }
    },
)

function onDragOver(event: DragEvent) {
    const types = event.dataTransfer?.types ?? []

    // Rows moved from the list, or files from the computer to upload here.
    if (types.includes('application/x-noar-files') || types.includes('Files')) {
        event.preventDefault()
        dragOver.value = true
    }
}

function onDrop(event: DragEvent) {
    dragOver.value = false
    emit('drop', props.path, event)
}
</script>
<template>
    <li class="list-none">
        <div
            class="flex items-center rounded-field min-h-9"
            :class="[
                isCurrent ? 'bg-base-300 text-base-content' : 'text-muted hover:bg-raised hover:text-base-content',
                dragOver ? 'outline-2 outline-accent -outline-offset-2' : '',
            ]"
            :style="{ paddingLeft: `${depth * 14}px` }"
            @dragover="onDragOver"
            @dragleave="dragOver = false"
            @drop.prevent="onDrop"
        >
            <button
                type="button"
                class="btn-icon min-w-7 min-h-7 text-subtle"
                :aria-expanded="open"
                :aria-label="open ? t('storage.collapse', { name: label }) : t('storage.expand', { name: label })"
                @click="toggle"
            >
                <span v-if="loading" class="loading loading-spinner loading-xs" />
                <i v-else class="bi text-[11px]" :class="open ? 'bi-chevron-down' : 'bi-chevron-right'" />
            </button>
            <button
                type="button"
                class="flex-1 min-w-0 flex items-center gap-2 text-left py-1.5 pe-2 cursor-pointer bg-transparent border-0 text-inherit"
                :aria-current="isCurrent ? 'page' : undefined"
                @click="emit('open', path)"
            >
                <i class="bi" :class="open ? 'bi-folder2-open' : 'bi-folder'" aria-hidden="true" />
                <span class="truncate flex-1">{{ label }}</span>
                <span
                    v-if="commercial"
                    class="text-[11px] text-after-clip border border-after-clip/50 rounded-md px-1.5 leading-4"
                    :title="t('storage.commercial.badgeHint')"
                >
                    {{ t('storage.commercial.badge') }}
                </span>
                <span
                    v-else-if="live"
                    class="text-[11px] text-live border border-live/50 rounded-md px-1.5 leading-4"
                    :title="t('storage.live.badgeHint')"
                >
                    {{ t('storage.live.badge') }}
                </span>
                <span
                    v-else-if="hasSheet"
                    class="text-[11px] text-accent-text border border-accent/40 rounded-md px-1.5 leading-4"
                    :title="t('storage.hasSheet')"
                >
                    {{ movie ? t('storage.movieBadge') : t('storage.seriesBadge') }}
                </span>
            </button>
        </div>
        <ul v-if="open && folders.length" class="m-0 p-0">
            <FolderTree
                v-for="folder in folders"
                :key="folder"
                :channel-id="channelId"
                :path="storageJoin(path, folder)"
                :label="folder"
                :has-sheet="sheetFolders.includes(folder)"
                :movie="movieFolders.includes(folder)"
                :live="liveFolders.includes(folder)"
                :commercial="commercialFolders.includes(folder)"
                :current="current"
                :version="loadedVersion"
                :depth="depth + 1"
                @open="emit('open', $event)"
                @drop="(target, event) => emit('drop', target, event)"
            />
        </ul>
    </li>
</template>
