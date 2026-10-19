<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { storeToRefs } from 'pinia'

import { authFetch } from '@/composables/authFetch'
import { stringFormatter } from '@/composables/helper'
import { FOLDER_DRAG_TYPE, LIBRARY_DRAG_TYPE, isGuideAsset, lengthLabel, type LibraryFile } from '@/composables/player'
import { useAuth } from '@/stores/auth'
import { useConfig } from '@/stores/config'
import { useIndex } from '@/stores/index'

const props = defineProps<{ allowAdd: boolean }>()
const emit = defineEmits<{ preview: [path: string]; add: [file: LibraryFile]; addProgram: [folder: string] }>()

const { t } = useI18n()
const { mediaType } = stringFormatter()
const authStore = useAuth()
const configStore = useConfig()
const indexStore = useIndex()
const { i } = storeToRefs(configStore)

interface BrowseResult {
    source: string
    parent: string
    folders: string[]
    sheet_folders?: string[]
    files: { name: string; duration: number }[]
}

const source = ref('')
const folders = ref<string[]>([])
const sheetFolders = ref<string[]>([])
const creating = ref(false)
const newFolder = ref('')
const files = ref<{ name: string; duration: number }[]>([])
const search = ref('')
const loading = ref(false)
const error = ref('')

const crumbs = computed(() => source.value.split('/').filter(Boolean))
const needle = computed(() => search.value.trim().toLowerCase())
const shownFolders = computed(() => folders.value.filter((folder) => folder.toLowerCase().includes(needle.value)))
const shownFiles = computed(() =>
    files.value
        .filter((file) => file.name.toLowerCase().includes(needle.value))
        .map((file) => ({
            ...file,
            guide: isGuideAsset(file.name),
            kind: mediaType(file.name) ?? 'other',
        })),
)

function joinPath(...parts: string[]): string {
    return parts.join('/').split('/').filter(Boolean).join('/')
}

function absolute(name: string): string {
    const storage = (configStore.channels[configStore.i]?.storage ?? '').replace(/\/+$/, '')

    return `${storage}/${joinPath(source.value, name)}`
}

async function open(path: string) {
    const id = configStore.channels[configStore.i]?.id

    if (!id) {
        return
    }

    loading.value = true
    error.value = ''

    try {
        const data = await authFetch<BrowseResult>(`/api/file/${id}/browse`, {
            method: 'POST',
            headers: { ...configStore.contentType, ...authStore.authHeader },
            body: JSON.stringify({ source: `/${path}`, folders_only: false }),
        })

        source.value = joinPath(data.source ?? path)
        folders.value = data.folders ?? []
        sheetFolders.value = data.sheet_folders ?? []
        files.value = data.files ?? []
        search.value = ''
    } catch (err) {
        error.value = err instanceof Error ? err.message : String(err)
    } finally {
        loading.value = false
    }
}

function file(name: string, duration: number): LibraryFile {
    return { source: absolute(name), name, duration: duration || 10 }
}

function dragStart(event: DragEvent, name: string, duration: number) {
    event.dataTransfer?.setData(LIBRARY_DRAG_TYPE, JSON.stringify(file(name, duration)))
    event.dataTransfer?.setData('text/plain', absolute(name))

    if (event.dataTransfer) {
        event.dataTransfer.effectAllowed = 'copy'
    }
}

function dragFolder(event: DragEvent, folder: string) {
    event.dataTransfer?.setData(FOLDER_DRAG_TYPE, joinPath(source.value, folder))

    if (event.dataTransfer) {
        event.dataTransfer.effectAllowed = 'copy'
    }
}

/** Create a folder inside the one on screen, without leaving the Player. */
async function createFolder() {
    const name = newFolder.value.trim()
    const id = configStore.channels[configStore.i]?.id

    if (!name || !id) {
        creating.value = false
        return
    }

    if (folders.value.some((folder) => folder.toLowerCase() === name.toLowerCase())) {
        indexStore.msgAlert('warning', t('storage.folderExists'), 3)
        return
    }

    try {
        await authFetch(`/api/file/${id}/create-folder`, {
            method: 'POST',
            headers: configStore.contentType,
            body: JSON.stringify({ source: `/${joinPath(source.value, name)}` }),
        })
        creating.value = false
        newFolder.value = ''
        indexStore.msgAlert('success', t('onair.folderCreated', { name }), 2)
        await open(source.value)
    } catch (e: any) {
        indexStore.msgAlert('error', e?.data ?? e?.message ?? String(e), 5)
    }
}

function iconOf(kind: string) {
    return kind === 'audio' ? 'bi-music-note-beamed' : kind === 'image' ? 'bi-image' : 'bi-film'
}

onMounted(() => open(''))
watch(i, () => open(''))
</script>
<template>
    <aside :aria-label="t('onair.library')" class="card-box p-3.5 flex flex-col gap-2.5 min-h-0">
        <div class="flex items-center gap-2">
            <h2 class="m-0 text-[15px] font-semibold flex-1">{{ t('onair.library') }}</h2>
            <button
                type="button"
                class="btn-quiet min-h-9 px-2.5 text-[13px]"
                :aria-expanded="creating"
                @click="creating = !creating"
            >
                <i class="bi bi-folder-plus" aria-hidden="true" />{{ t('storage.newFolder') }}
            </button>
            <RouterLink
                :to="{ path: '/media', query: { channel: configStore.channels[configStore.i]?.id } }"
                class="btn-icon border border-line-strong text-muted"
                :aria-label="t('onair.uploadFiles')"
                :title="t('onair.uploadFiles')"
            >
                <i class="bi bi-upload" aria-hidden="true" />
            </RouterLink>
        </div>
        <form v-if="creating" class="flex gap-2" @submit.prevent="createFolder">
            <input
                v-model="newFolder"
                :aria-label="t('storage.folderName')"
                :placeholder="t('storage.folderName')"
                class="field-input flex-1 min-w-0"
                @keydown.esc="creating = false"
            />
            <button type="submit" class="btn-main px-3" :disabled="!newFolder.trim()">{{ t('onair.create') }}</button>
        </form>
        <label
            class="flex items-center gap-2 bg-field border border-line-strong rounded-field px-2.5 min-h-9.5 text-muted"
        >
            <i class="bi bi-search text-xs" aria-hidden="true" />
            <input
                v-model="search"
                type="search"
                :placeholder="t('onair.searchLibrary')"
                class="bg-transparent border-0 text-base-content flex-1 min-w-0 outline-none"
            />
        </label>
        <nav class="text-xs text-muted flex flex-wrap items-center gap-1" :aria-label="t('onair.folder')">
            <button class="hover:text-accent-text cursor-pointer" @click="open('')">{{ t('onair.storage') }}</button>
            <template v-for="(crumb, index) in crumbs" :key="index">
                <span aria-hidden="true">/</span>
                <button
                    class="hover:text-accent-text cursor-pointer"
                    @click="open(crumbs.slice(0, index + 1).join('/'))"
                >
                    {{ crumb }}
                </button>
            </template>
        </nav>

        <div v-if="loading" class="py-6 flex justify-center">
            <span class="loading loading-spinner loading-sm" />
        </div>
        <p v-else-if="error" class="text-error text-sm">{{ error }}</p>

        <ul v-else class="flex flex-col overflow-auto min-h-0 -mx-1 px-1">
            <li v-if="crumbs.length">
                <button
                    class="w-full flex items-center gap-2.5 px-2 py-2 rounded-field min-h-10 text-left text-muted hover:bg-raised cursor-pointer"
                    @click="open(crumbs.slice(0, -1).join('/'))"
                >
                    <i class="bi bi-chevron-left text-xs" aria-hidden="true" />{{ t('onair.back') }}
                </button>
            </li>
            <li
                v-for="folder in shownFolders"
                :key="`d-${folder}`"
                class="flex items-center rounded-field hover:bg-raised"
                :draggable="props.allowAdd"
                @dragstart="dragFolder($event, folder)"
            >
                <button
                    class="flex-1 min-w-0 flex items-center gap-2.5 px-2 py-2 min-h-10 text-left cursor-pointer"
                    @click="open(joinPath(source, folder))"
                >
                    <i class="bi bi-folder2 text-muted" aria-hidden="true" />
                    <span class="truncate">{{ folder }}</span>
                    <span
                        v-if="sheetFolders.includes(folder)"
                        class="text-[11px] text-accent-text border border-accent/40 rounded-md px-1.5 leading-4"
                    >
                        {{ t('storage.sheetBadge') }}
                    </span>
                </button>
                <button
                    v-if="props.allowAdd"
                    class="btn-icon min-w-8 min-h-8 me-1 text-muted"
                    :aria-label="t('onair.addProgram', { name: folder })"
                    :title="t('onair.addProgram', { name: folder })"
                    @click="emit('addProgram', joinPath(source, folder))"
                >
                    <i class="bi bi-plus-lg" aria-hidden="true" />
                </button>
            </li>
            <li
                v-for="item in shownFiles"
                :key="`f-${item.name}`"
                class="group flex items-center gap-2.5 px-2 py-1.5 rounded-field min-h-10"
                :class="item.guide ? 'text-muted' : 'hover:bg-raised cursor-grab'"
                :draggable="!item.guide && props.allowAdd"
                @dragstart="dragStart($event, item.name, item.duration)"
            >
                <i
                    v-if="item.guide"
                    class="bi text-sm"
                    :class="item.kind === 'image' ? 'bi-image' : 'bi-file-earmark-text'"
                    aria-hidden="true"
                />
                <svg
                    v-else
                    width="12"
                    height="14"
                    viewBox="0 0 12 16"
                    fill="currentColor"
                    class="text-subtle flex-none"
                    aria-hidden="true"
                >
                    <circle cx="3.5" cy="3.5" r="1.2" />
                    <circle cx="8.5" cy="3.5" r="1.2" />
                    <circle cx="3.5" cy="8" r="1.2" />
                    <circle cx="8.5" cy="8" r="1.2" />
                    <circle cx="3.5" cy="12.5" r="1.2" />
                    <circle cx="8.5" cy="12.5" r="1.2" />
                </svg>
                <span class="flex-1 min-w-0 truncate" :title="item.name">
                    <i
                        v-if="!item.guide && item.kind !== 'video'"
                        class="bi me-1 text-muted"
                        :class="iconOf(item.kind)"
                        aria-hidden="true"
                    />{{ item.name }}
                </span>
                <template v-if="item.guide">
                    <span class="text-xs">{{ t('onair.guideAsset') }}</span>
                </template>
                <template v-else>
                    <span class="font-mono text-xs text-muted group-hover:hidden group-focus-within:hidden">
                        {{ lengthLabel(item.duration) }}
                    </span>
                    <span class="hidden group-hover:flex group-focus-within:flex items-center">
                        <button
                            class="btn-icon min-w-8 min-h-8"
                            :aria-label="t('onair.previewFile', { name: item.name })"
                            @click="emit('preview', absolute(item.name))"
                        >
                            <i class="bi bi-play-fill" aria-hidden="true" />
                        </button>
                        <button
                            v-if="props.allowAdd"
                            class="btn-icon min-w-8 min-h-8"
                            :aria-label="t('onair.addFile', { name: item.name })"
                            @click="emit('add', file(item.name, item.duration))"
                        >
                            <i class="bi bi-plus-lg" aria-hidden="true" />
                        </button>
                    </span>
                </template>
            </li>
            <li v-if="!shownFolders.length && !shownFiles.length" class="text-sm text-muted px-2 py-3">
                {{ t('onair.emptyFolder') }}
            </li>
        </ul>
        <p class="m-0 text-xs text-muted">{{ props.allowAdd ? t('onair.dragHint') : t('onair.folderModeHint') }}</p>
    </aside>
</template>
