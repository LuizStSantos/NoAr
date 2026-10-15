<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useHead } from '@unhead/vue'
import { useRoute } from 'vue-router'
import dayjs from 'dayjs'

import { useAuth } from '@/stores/auth'
import { useConfig } from '@/stores/config'
import { useIndex } from '@/stores/index'
import { useMedia } from '@/stores/media'

import FolderTree from '@/components/storage/FolderTree.vue'
import SheetCard from '@/components/storage/SheetCard.vue'
import SheetEditor from '@/components/storage/SheetEditor.vue'
import GenericModal from '@/components/utils/GenericModal.vue'
import VideoPlayer from '@/components/utils/VideoPlayer.vue'

import { authFetch } from '@/composables/authFetch'
import { createFilePreviewUrl } from '@/composables/fileAccess'
import { stringFormatter } from '@/composables/helper'
import { baseName, readSheet, storageJoin, uploadArtwork } from '@/composables/storage'
import { useFileUpload } from '@/composables/useFileUpload'

const { t, locale } = useI18n()
const authStore = useAuth()
const configStore = useConfig()
const indexStore = useIndex()
const mediaStore = useMedia()
const route = useRoute()
const { mediaType, fileSize } = stringFormatter()
const { uploadTask, currentNumber, currentProgress, uploading, resetUploadState, uploadFiles } = useFileUpload()

const DRAG_TYPE = 'application/x-noar-files'
const IMAGE_TYPES = ['jpg', 'jpeg', 'png', 'webp', 'gif']

interface Row {
    name: string
    folder: boolean
    file?: FileObject
}

const channelId = computed(() => configStore.channels[configStore.i]?.id ?? 0)
const current = computed(() => storageJoin(mediaStore.folderTree.source))
const crumbs = computed(() => {
    const parts = current.value ? current.value.split('/') : []

    return [
        { name: t('storage.root'), path: '' },
        ...parts.map((name, index) => ({ name, path: parts.slice(0, index + 1).join('/') })),
    ]
})

const allFiles = ref(readSetting('storage-all-files') === 'true')
const filter = ref('')
const selected = ref<string[]>([])
const sheet = ref<MediaSheet | null>(null)
const editorPath = ref<string | null>(null)
const treeVersion = ref(0)
const disk = ref<{ total: number; free: number } | null>(null)
const dropActive = ref(false)
const uploadInput = ref<HTMLInputElement>()
const nfoUploading = ref(false)

const renameTarget = ref<Row | null>(null)
const renameValue = ref('')
const showCreate = ref(false)
const newFolderName = ref('')
const deleteNames = ref<string[]>([])
const deleteRecursive = ref(false)
const moveNames = ref<string[]>([])
const moveTarget = ref('')
const preview = ref<{ name: string; url: string; video: boolean; options?: any } | null>(null)

useHead({ title: computed(() => t('storage.title')) })

function readSetting(key: string): string | null {
    try {
        return localStorage.getItem(key)
    } catch {
        return null
    }
}

function writeSetting(key: string, value: string) {
    try {
        localStorage.setItem(key, value)
    } catch {
        // Only a convenience: the choice is not kept.
    }
}

const rows = computed<Row[]>(() => {
    const needle = filter.value.trim().toLowerCase()
    const folders: Row[] = mediaStore.folderTree.folders.map((folder) => ({ name: folder.name, folder: true }))
    const files: Row[] = mediaStore.folderTree.files.map((file) => ({ name: file.name, folder: false, file }))

    return [...folders, ...files].filter((row) => !needle || row.name.toLowerCase().includes(needle))
})

/** Playable video or audio; images can be in the channel's extensions too. */
function isVideo(file?: FileObject): boolean {
    return Boolean(file) && file?.media !== false && !IMAGE_TYPES.includes(extension(file?.name ?? ''))
}

const videos = computed(() => mediaStore.folderTree.files.filter((file) => isVideo(file)))
const summary = computed(() => {
    const seconds = videos.value.reduce((sum, file) => sum + (file.duration || 0), 0)

    return t('storage.summary', { count: videos.value.length, length: clock(seconds) }, videos.value.length)
})
const allSelected = computed(
    () => rows.value.length > 0 && rows.value.every((row) => selected.value.includes(row.name)),
)
const sheetFolders = computed(() => mediaStore.folderTree.sheet_folders ?? [])

function clock(seconds: number): string {
    const total = Math.round(seconds)
    const hours = Math.floor(total / 3600)
    const minutes = Math.floor((total % 3600) / 60)
    const rest = String(total % 60).padStart(2, '0')

    return hours ? `${hours}:${String(minutes).padStart(2, '0')}:${rest}` : `${minutes}:${rest}`
}

function modifiedLabel(file?: FileObject): string {
    if (!file?.modified) {
        return '—'
    }

    return dayjs.unix(file.modified).locale(locale.value.toLowerCase()).format('D MMM YYYY, HH:mm')
}

function extension(name: string): string {
    return name.split('.').pop()?.toLowerCase() ?? ''
}

function fileIcon(row: Row): string {
    if (row.folder) {
        return 'bi-folder'
    }

    const ext = extension(row.name)

    if (ext === 'nfo') {
        return 'bi-file-earmark-text'
    }

    if (IMAGE_TYPES.includes(ext)) {
        return 'bi-file-earmark-image'
    }

    return mediaType(row.name) === 'audio' ? 'bi-music-note-beamed' : 'bi-film'
}

function canPreview(row: Row): boolean {
    return !row.folder && (row.file?.media !== false || IMAGE_TYPES.includes(extension(row.name)))
}

async function load(path: string) {
    selected.value = []
    filter.value = ''
    await mediaStore.getTree(`/${path}`, false, allFiles.value)
}

async function refresh() {
    await mediaStore.getTree(`/${current.value}`, false, allFiles.value)
    selected.value = selected.value.filter((name) => rows.value.some((row) => row.name === name))
    treeVersion.value++
    await loadSheet()
}

async function loadSheet() {
    const path = current.value

    if (!path || !channelId.value) {
        sheet.value = null
        return
    }

    try {
        const next = await readSheet(channelId.value, path)

        if (path === current.value) {
            sheet.value = next
        }
    } catch {
        sheet.value = null
    }
}

async function loadDisk() {
    if (!channelId.value) {
        return
    }

    try {
        const stat = await authFetch<{ storage?: { total: number; free: number } }>(`/api/system/${channelId.value}`)

        disk.value = stat.storage?.total ? stat.storage : null
    } catch {
        disk.value = null
    }
}

onMounted(async () => {
    // Other pages link to a folder with `?path=` (the log, for example).
    const linked = typeof route.query.path === 'string' ? storageJoin(route.query.path) : null

    await load(linked ?? current.value)
    loadDisk()
})

watch(current, loadSheet, { immediate: true })

watch(channelId, async () => {
    await load('')
    loadDisk()
})

watch(allFiles, (value) => {
    writeSetting('storage-all-files', String(value))
    mediaStore.getTree(`/${current.value}`, false, value)
})

function toggle(name: string) {
    selected.value = selected.value.includes(name)
        ? selected.value.filter((item) => item !== name)
        : [...selected.value, name]
}

function toggleAll() {
    selected.value = allSelected.value ? [] : rows.value.map((row) => row.name)
}

function failed(error: any) {
    indexStore.msgAlert('error', t('storage.actionFailed', { error: error?.data ?? error?.message ?? error }), 6)
}

async function move(names: string[], target: string) {
    const from = current.value

    if (target === from) {
        indexStore.msgAlert('warning', t('storage.moveSame'), 3)
        return
    }

    try {
        for (const name of names) {
            if (storageJoin(from, name) === target || target.startsWith(`${storageJoin(from, name)}/`)) {
                continue
            }

            await authFetch(`/api/file/${channelId.value}/rename`, {
                method: 'POST',
                headers: configStore.contentType,
                body: JSON.stringify({
                    source: `/${storageJoin(from, name)}`,
                    target: `/${storageJoin(target, name)}`,
                }),
            })
        }
    } catch (e) {
        failed(e)
    }

    await refresh()
}

// Moving with drag and drop: rows go onto the tree, folder rows or the path.
function dragRow(event: DragEvent, row: Row) {
    const names = selected.value.includes(row.name) ? selected.value : [row.name]

    event.dataTransfer?.setData(DRAG_TYPE, JSON.stringify(names))
    event.dataTransfer?.setData('text/plain', names.join('\n'))

    if (event.dataTransfer) {
        event.dataTransfer.effectAllowed = 'move'
    }
}

function droppedNames(event: DragEvent): string[] {
    try {
        return JSON.parse(event.dataTransfer?.getData(DRAG_TYPE) || '[]')
    } catch {
        return []
    }
}

function dropOnFolder(target: string, event: DragEvent) {
    const names = droppedNames(event)

    if (names.length) {
        move(names, target)
    }
}

function allowFolderDrop(event: DragEvent) {
    if (event.dataTransfer?.types.includes(DRAG_TYPE)) {
        event.preventDefault()
    }
}

// Uploads: the button, or files dragged from the computer onto the list.
function onDragOverFiles(event: DragEvent) {
    if (event.dataTransfer?.types.includes('Files')) {
        event.preventDefault()
        dropActive.value = true
    }
}

function onDropFiles(event: DragEvent) {
    dropActive.value = false

    const files = Array.from(event.dataTransfer?.files ?? [])

    if (files.length) {
        upload(files)
    }
}

function onPickFiles(event: Event) {
    const input = event.target as HTMLInputElement

    upload(Array.from(input.files ?? []))
    input.value = ''
}

async function upload(files: File[]) {
    const existing = new Set(mediaStore.folderTree.files.map((file) => file.name))
    const fresh = files.filter((file) => {
        if (existing.has(file.name)) {
            indexStore.msgAlert('warning', t('storage.fileExists', { name: file.name }), 4)
            return false
        }

        return true
    })

    if (!fresh.length || uploading.value) {
        return
    }

    const folder = current.value

    try {
        await uploadFiles(fresh, {
            buildRequest: (_file, currentIndex, batchCount) => ({
                url: `/api/file/${channelId.value}/upload?path=${encodeURIComponent(folder)}`,
                headers: authStore.authHeader,
                method: 'PUT',
                currentIndex,
                batchCount,
            }),
        })
        indexStore.msgAlert('success', t('storage.uploadDone'), 2)
    } catch (e) {
        failed(e)
    }

    resetUploadState()

    if (folder === current.value) {
        await refresh()
    }
}

async function uploadNfo(file: File) {
    nfoUploading.value = true

    try {
        // Saved as the folder's program sheet, whatever the file was called.
        const named = new File([file], 'tvshow.nfo', { type: 'text/xml' })

        await upload([named])
    } finally {
        nfoUploading.value = false
    }
}

async function cardArtwork(kind: ArtworkKind, file: File) {
    try {
        sheet.value = await uploadArtwork(channelId.value, current.value, kind, file)
        indexStore.msgAlert('success', t('storage.artwork.saved'), 2)
        await mediaStore.getTree(`/${current.value}`, false, allFiles.value)
    } catch (e) {
        failed(e)
    }
}

async function onSheetSaved(next: MediaSheet) {
    if (!next.clip) {
        sheet.value = next
    }

    await mediaStore.getTree(`/${current.value}`, false, allFiles.value)
    treeVersion.value++
}

// Rename, create, delete and move dialogs.
function startRename(row: Row) {
    renameTarget.value = row
    renameValue.value = row.name
}

async function finishRename(confirmed: boolean) {
    const row = renameTarget.value
    const name = renameValue.value.trim()

    renameTarget.value = null

    if (!confirmed || !row || !name || name === row.name) {
        return
    }

    try {
        await authFetch(`/api/file/${channelId.value}/rename`, {
            method: 'POST',
            headers: configStore.contentType,
            body: JSON.stringify({
                source: `/${storageJoin(current.value, row.name)}`,
                target: `/${storageJoin(current.value, name)}`,
            }),
        })
    } catch (e) {
        failed(e)
    }

    await refresh()
}

async function finishCreate(confirmed: boolean) {
    const name = newFolderName.value.trim()

    showCreate.value = false
    newFolderName.value = ''

    if (!confirmed || !name) {
        return
    }

    if (mediaStore.folderTree.folders.some((folder) => folder.name === name)) {
        indexStore.msgAlert('warning', t('storage.folderExists'), 3)
        return
    }

    try {
        await authFetch(`/api/file/${channelId.value}/create-folder`, {
            method: 'POST',
            headers: configStore.contentType,
            body: JSON.stringify({ source: `/${storageJoin(current.value, name)}` }),
        })
        indexStore.msgAlert('success', t('storage.folderCreated'), 2)
    } catch (e) {
        failed(e)
    }

    await refresh()
}

const deleteHasFolder = computed(() =>
    deleteNames.value.some((name) => mediaStore.folderTree.folders.some((folder) => folder.name === name)),
)

async function finishDelete(confirmed: boolean) {
    const names = deleteNames.value
    const recursive = deleteRecursive.value

    deleteNames.value = []
    deleteRecursive.value = false

    if (!confirmed) {
        return
    }

    try {
        for (const name of names) {
            await authFetch(`/api/file/${channelId.value}/remove`, {
                method: 'POST',
                headers: configStore.contentType,
                body: JSON.stringify({ source: `/${storageJoin(current.value, name)}`, recursive }),
            })
        }
    } catch (e) {
        failed(e)
    }

    selected.value = selected.value.filter((name) => !names.includes(name))
    await refresh()
}

function startMove(names: string[]) {
    moveNames.value = names
    moveTarget.value = current.value
}

async function finishMove(confirmed: boolean) {
    const names = moveNames.value

    moveNames.value = []

    if (confirmed && names.length) {
        await move(names, moveTarget.value)
    }
}

async function openPreview(row: Row) {
    const path = `/${storageJoin(current.value, row.name)}`

    try {
        const url = await createFilePreviewUrl(channelId.value, path)
        const video = row.file?.media !== false
        const ext = extension(row.name)

        preview.value = {
            name: row.name,
            url,
            video,
            options: video
                ? {
                      liveui: false,
                      controls: true,
                      suppressNotSupportedError: true,
                      autoplay: false,
                      preload: 'auto',
                      sources: [
                          {
                              type: mediaType(row.name) === 'audio' ? `audio/${ext}` : `video/${ext}`,
                              src: url,
                          },
                      ],
                  }
                : undefined,
        }
    } catch (e) {
        failed(e)
    }
}

const uploadAccept = computed(() =>
    [...configStore.playout.storage.extensions, ...IMAGE_TYPES, 'nfo'].map((ext) => `.${ext}`).join(','),
)
</script>
<template>
    <div class="max-w-[1440px] mx-auto px-4 sm:px-6 py-6 flex flex-col gap-4.5">
        <div class="flex flex-wrap items-center gap-x-4 gap-y-3">
            <div class="flex-[1_1_320px] min-w-0">
                <h1 class="m-0 text-2xl font-semibold">{{ t('storage.title') }}</h1>
                <nav :aria-label="t('storage.path')" class="flex flex-wrap items-center gap-1.5 mt-1 text-muted">
                    <template v-for="(crumb, index) in crumbs" :key="crumb.path">
                        <i v-if="index > 0" class="bi bi-chevron-right text-[10px]" aria-hidden="true" />
                        <span v-if="index === crumbs.length - 1" aria-current="page" class="text-base-content">
                            {{ crumb.name }}
                        </span>
                        <button
                            v-else
                            type="button"
                            class="bg-transparent border-0 p-0 text-inherit hover:text-base-content cursor-pointer"
                            @click="load(crumb.path)"
                            @dragover="allowFolderDrop"
                            @drop.prevent="dropOnFolder(crumb.path, $event)"
                        >
                            {{ crumb.name }}
                        </button>
                    </template>
                </nav>
            </div>
            <label
                class="flex items-center gap-2 bg-base-100 border border-line-strong rounded-field px-3 min-h-10.5 text-muted flex-[0_1_300px] min-w-48"
            >
                <i class="bi bi-search text-xs" aria-hidden="true" />
                <input
                    v-model="filter"
                    type="search"
                    :placeholder="t('storage.filter')"
                    :aria-label="t('storage.filter')"
                    class="bg-transparent border-0 text-base-content flex-1 min-w-0 outline-none"
                />
            </label>
            <button type="button" class="btn-quiet min-h-10.5" @click="showCreate = true">
                <i class="bi bi-folder-plus" aria-hidden="true" />{{ t('storage.newFolder') }}
            </button>
            <button type="button" class="btn-main min-h-10.5" :disabled="uploading" @click="uploadInput?.click()">
                <i class="bi bi-upload" aria-hidden="true" />{{ t('storage.upload') }}
            </button>
            <input ref="uploadInput" type="file" multiple class="hidden" :accept="uploadAccept" @change="onPickFiles" />
        </div>

        <div class="flex flex-wrap gap-5 items-start">
            <aside
                :aria-label="t('storage.folders')"
                class="flex-[1_1_240px] w-full lg:max-w-[280px] flex flex-col gap-3.5"
            >
                <nav class="card-box p-2.5">
                    <div class="text-[11px] text-subtle tracking-[0.08em] uppercase px-2.5 py-1.5">
                        {{ t('storage.folders') }}
                    </div>
                    <ul v-if="channelId" class="m-0 p-0 max-h-[60vh] overflow-y-auto">
                        <FolderTree
                            :key="channelId"
                            :channel-id="channelId"
                            path=""
                            :label="t('storage.root')"
                            :current="current"
                            :version="treeVersion"
                            @open="load"
                            @drop="dropOnFolder"
                        />
                    </ul>
                </nav>
                <section v-if="disk" :aria-label="t('storage.disk')" class="card-box px-4 py-3.5 flex flex-col gap-2">
                    <div class="flex justify-between gap-2 text-[13px]">
                        <span class="text-muted">{{ t('storage.disk') }}</span>
                        <span class="font-mono">
                            {{
                                t('storage.diskFree', { free: fileSize(disk.free, 1), total: fileSize(disk.total, 1) })
                            }}
                        </span>
                    </div>
                    <div class="h-2 rounded bg-base-300 overflow-hidden">
                        <div
                            class="h-full"
                            :class="disk.free / disk.total < 0.1 ? 'bg-on-air' : 'bg-ok'"
                            :style="{ width: `${((disk.total - disk.free) / disk.total) * 100}%` }"
                        />
                    </div>
                </section>
            </aside>

            <main class="flex-[999_1_640px] min-w-0 flex flex-col gap-4">
                <SheetCard
                    v-if="current && sheet"
                    :channel-id="channelId"
                    :folder="current"
                    :sheet="sheet"
                    :videos="videos.length"
                    :videos-with-sheet="videos.filter((file) => file.sheet).length"
                    @edit="editorPath = current"
                    @artwork="cardArtwork"
                    @upload-nfo="uploadNfo"
                    @open-folder="load"
                />

                <section
                    :aria-label="t('storage.files')"
                    class="card-box relative"
                    @dragover="onDragOverFiles"
                    @dragleave.self="dropActive = false"
                    @drop.prevent="onDropFiles"
                >
                    <div class="flex flex-wrap items-center gap-x-3.5 gap-y-2.5 px-4 py-3 border-b border-line">
                        <h2 class="m-0 text-[15px] font-semibold">{{ t('storage.files') }}</h2>
                        <span class="font-mono text-xs bg-raised rounded-md px-2 py-1">{{ summary }}</span>
                        <div class="flex-auto" />
                        <div
                            role="group"
                            :aria-label="t('storage.show')"
                            class="inline-flex bg-base-200 border border-line-strong rounded-field p-[3px]"
                        >
                            <button
                                v-for="option in [false, true]"
                                :key="String(option)"
                                type="button"
                                :aria-pressed="allFiles === option"
                                class="border-0 rounded-md px-3 min-h-8 text-[13px] cursor-pointer"
                                :class="
                                    allFiles === option ? 'bg-base-300 text-base-content' : 'bg-transparent text-muted'
                                "
                                @click="allFiles = option"
                            >
                                {{ option ? t('storage.allFiles') : t('storage.videos') }}
                            </button>
                        </div>
                    </div>

                    <div
                        v-if="selected.length"
                        class="bg-accent-soft border-b border-accent/30 px-4 py-2.5 flex flex-wrap items-center gap-x-3.5 gap-y-2 text-[13px]"
                    >
                        <span class="font-semibold">{{
                            t('storage.selected', { count: selected.length }, selected.length)
                        }}</span>
                        <button type="button" class="btn-quiet min-h-8.5 text-[13px]" @click="startMove([...selected])">
                            {{ t('storage.moveTo') }}
                        </button>
                        <button
                            type="button"
                            class="btn-quiet min-h-8.5 text-[13px] text-error"
                            @click="deleteNames = [...selected]"
                        >
                            {{ t('storage.delete') }}
                        </button>
                        <div class="flex-auto" />
                        <button type="button" class="link-accent text-base-content/80" @click="selected = []">
                            {{ t('storage.clearSelection') }}
                        </button>
                    </div>

                    <div class="relative overflow-x-auto">
                        <div
                            v-if="mediaStore.isLoading"
                            class="absolute inset-0 z-10 flex justify-center pt-10 bg-base-100/60"
                        >
                            <span class="loading loading-spinner loading-lg" />
                        </div>
                        <table class="w-full min-w-[760px] border-collapse text-[13px]">
                            <thead>
                                <tr class="text-left text-muted">
                                    <th class="ps-4 pe-2 py-2.5 w-8 font-medium">
                                        <input
                                            type="checkbox"
                                            class="checkbox checkbox-xs checkbox-accent rounded-[4px]"
                                            :checked="allSelected"
                                            :aria-label="t('storage.selectAll')"
                                            @change="toggleAll"
                                        />
                                    </th>
                                    <th class="px-2 py-2.5 font-medium">{{ t('storage.name') }}</th>
                                    <th class="px-2 py-2.5 font-medium w-20">{{ t('storage.duration') }}</th>
                                    <th class="px-2 py-2.5 font-medium w-24">{{ t('storage.size') }}</th>
                                    <th class="px-2 py-2.5 font-medium w-40">{{ t('storage.modified') }}</th>
                                    <th class="px-2 py-2.5 font-medium w-36">{{ t('storage.sheetColumn') }}</th>
                                    <th class="ps-2 pe-4 py-2.5 w-40">
                                        <span class="sr-only">{{ t('storage.actions') }}</span>
                                    </th>
                                </tr>
                            </thead>
                            <tbody>
                                <tr
                                    v-for="row in rows"
                                    :key="`${row.folder ? 'd' : 'f'}:${row.name}`"
                                    class="group border-t border-raised"
                                    :class="selected.includes(row.name) ? 'bg-accent-soft' : 'hover:bg-raised/60'"
                                    draggable="true"
                                    @dragstart="dragRow($event, row)"
                                    @dragover="row.folder && allowFolderDrop($event)"
                                    @drop.prevent="row.folder && dropOnFolder(storageJoin(current, row.name), $event)"
                                >
                                    <td class="ps-4 pe-2 py-2">
                                        <input
                                            type="checkbox"
                                            class="checkbox checkbox-xs checkbox-accent rounded-[4px]"
                                            :checked="selected.includes(row.name)"
                                            :aria-label="t('storage.select', { name: row.name })"
                                            @change="toggle(row.name)"
                                        />
                                    </td>
                                    <td class="px-2 py-2 max-w-0 w-full">
                                        <button
                                            v-if="row.folder"
                                            type="button"
                                            class="flex items-center gap-2.5 bg-transparent border-0 p-0 text-inherit cursor-pointer min-h-7 max-w-full"
                                            :aria-label="t('storage.open', { name: row.name })"
                                            @click="load(storageJoin(current, row.name))"
                                        >
                                            <i class="bi text-muted" :class="fileIcon(row)" aria-hidden="true" />
                                            <span class="truncate font-medium">{{ row.name }}</span>
                                            <span
                                                v-if="sheetFolders.includes(row.name)"
                                                class="text-[11px] text-accent-text border border-accent/40 rounded-md px-1.5 leading-4"
                                            >
                                                {{ t('storage.sheetBadge') }}
                                            </span>
                                        </button>
                                        <span v-else class="flex items-center gap-2.5 min-h-7 min-w-0">
                                            <i class="bi text-muted" :class="fileIcon(row)" aria-hidden="true" />
                                            <span
                                                class="truncate"
                                                :class="row.file?.media === false ? 'text-muted' : ''"
                                            >
                                                {{ row.name }}
                                            </span>
                                        </span>
                                    </td>
                                    <td class="px-2 py-2 font-mono">
                                        {{ isVideo(row.file) ? clock(row.file?.duration ?? 0) : '' }}
                                    </td>
                                    <td class="px-2 py-2 font-mono text-muted">
                                        {{ row.file ? fileSize(row.file.size, 1) || '0 B' : '' }}
                                    </td>
                                    <td class="px-2 py-2 text-muted whitespace-nowrap">
                                        {{ row.file ? modifiedLabel(row.file) : '' }}
                                    </td>
                                    <td class="px-2 py-2">
                                        <template v-if="row.file && isVideo(row.file)">
                                            <button
                                                v-if="row.file.sheet"
                                                type="button"
                                                class="link-accent text-ok inline-flex items-center gap-1.5 whitespace-nowrap"
                                                @click="editorPath = storageJoin(current, row.name)"
                                            >
                                                <i class="bi bi-check-lg" aria-hidden="true" />{{
                                                    t('storage.editSheet')
                                                }}
                                            </button>
                                            <button
                                                v-else
                                                type="button"
                                                class="link-accent whitespace-nowrap"
                                                @click="editorPath = storageJoin(current, row.name)"
                                            >
                                                + {{ t('storage.createSheet') }}
                                            </button>
                                        </template>
                                    </td>
                                    <td class="ps-2 pe-4 py-1.5">
                                        <div
                                            class="flex justify-end gap-0.5 opacity-100 sm:opacity-0 sm:group-hover:opacity-100 sm:group-focus-within:opacity-100"
                                        >
                                            <button
                                                v-if="canPreview(row)"
                                                type="button"
                                                class="btn-icon text-muted"
                                                :aria-label="t('storage.preview', { name: row.name })"
                                                @click="openPreview(row)"
                                            >
                                                <i class="bi bi-play-fill" aria-hidden="true" />
                                            </button>
                                            <button
                                                type="button"
                                                class="btn-icon text-muted"
                                                :aria-label="t('storage.rename', { name: row.name })"
                                                @click="startRename(row)"
                                            >
                                                <i class="bi bi-pencil" aria-hidden="true" />
                                            </button>
                                            <button
                                                type="button"
                                                class="btn-icon text-muted"
                                                :aria-label="t('storage.move', { name: row.name })"
                                                @click="startMove([row.name])"
                                            >
                                                <i class="bi bi-arrow-right" aria-hidden="true" />
                                            </button>
                                            <button
                                                type="button"
                                                class="btn-icon text-error"
                                                :aria-label="t('storage.remove', { name: row.name })"
                                                @click="deleteNames = [row.name]"
                                            >
                                                <i class="bi bi-trash" aria-hidden="true" />
                                            </button>
                                        </div>
                                    </td>
                                </tr>
                            </tbody>
                        </table>
                        <p v-if="!rows.length && filter" class="m-0 px-4 py-6 text-muted">
                            {{ t('storage.noMatch', { filter }) }}
                        </p>
                        <p v-else-if="!rows.length && !mediaStore.isLoading" class="m-0 px-4 pt-6 pb-2 text-muted">
                            {{ t('storage.empty') }} {{ t('storage.emptyHint') }}
                        </p>
                    </div>

                    <button
                        type="button"
                        class="mx-4 mt-3 mb-4 w-[calc(100%-2rem)] border-[1.5px] border-dashed rounded-[10px] p-4.5 flex flex-wrap items-center justify-center gap-2.5 bg-transparent cursor-pointer"
                        :class="
                            dropActive
                                ? 'border-accent text-accent-text bg-accent-soft'
                                : 'border-line-strong text-muted'
                        "
                        @click="uploadInput?.click()"
                    >
                        <i class="bi bi-upload" aria-hidden="true" />
                        {{
                            dropActive
                                ? t('storage.dropRelease', { folder: crumbs[crumbs.length - 1]?.name })
                                : t('storage.dropHere')
                        }}
                    </button>
                </section>

                <section
                    v-if="uploading || nfoUploading"
                    aria-live="polite"
                    class="bg-raised border border-line-strong rounded-box px-4 py-3 flex flex-col gap-2"
                >
                    <div class="flex flex-wrap items-center gap-x-3.5 gap-y-1 text-[13px]">
                        <span class="font-semibold">
                            {{ t('storage.uploading', { current: currentNumber, total: Math.max(currentNumber, 1) }) }}
                        </span>
                        <span class="font-mono text-muted truncate">{{ uploadTask }}</span>
                        <div class="flex-auto" />
                        <span class="font-mono">{{ currentProgress }}%</span>
                    </div>
                    <div class="h-1.5 rounded bg-base-200 overflow-hidden">
                        <div class="h-full bg-accent transition-[width]" :style="{ width: `${currentProgress}%` }" />
                    </div>
                </section>
            </main>
        </div>

        <SheetEditor
            v-if="editorPath !== null"
            :key="editorPath"
            :channel-id="channelId"
            :path="editorPath"
            @close="editorPath = null"
            @saved="onSheetSaved"
        />

        <GenericModal :show="renameTarget !== null" :title="t('storage.renameTitle')" :modal-action="finishRename">
            <label class="field-label w-[min(420px,80vw)]">
                {{ t('storage.newName') }}
                <input v-model="renameValue" class="field-input" @keydown.enter="finishRename(true)" />
            </label>
        </GenericModal>

        <GenericModal :show="showCreate" :title="t('storage.createTitle')" :modal-action="finishCreate">
            <label class="field-label w-[min(420px,80vw)]">
                {{ t('storage.folderName') }}
                <input v-model="newFolderName" class="field-input" @keydown.enter="finishCreate(true)" />
            </label>
        </GenericModal>

        <GenericModal
            :show="deleteNames.length > 0"
            :title="t('storage.deleteTitle')"
            :ok-label="t('storage.delete')"
            danger
            :modal-action="finishDelete"
        >
            <div class="flex flex-col gap-3 w-[min(460px,80vw)]">
                <p class="m-0">{{ t('storage.deleteQuestion', { count: deleteNames.length }, deleteNames.length) }}</p>
                <ul class="m-0 ps-5 max-h-40 overflow-y-auto font-mono text-[13px] text-muted">
                    <li v-for="name in deleteNames" :key="name">{{ name }}</li>
                </ul>
                <label v-if="deleteHasFolder" class="flex items-center gap-2.5 cursor-pointer">
                    <input v-model="deleteRecursive" type="checkbox" class="checkbox checkbox-sm checkbox-accent" />
                    {{ t('storage.deleteRecursive') }}
                </label>
            </div>
        </GenericModal>

        <GenericModal
            :show="moveNames.length > 0"
            :title="t('storage.moveTitle', { count: moveNames.length }, moveNames.length)"
            :ok-label="t('storage.moveHere')"
            :ok-disabled="moveTarget === current"
            :modal-action="finishMove"
        >
            <div class="flex flex-col gap-2 w-[min(460px,80vw)]">
                <ul class="m-0 p-1 max-h-[50vh] overflow-y-auto border border-line rounded-field">
                    <FolderTree
                        :channel-id="channelId"
                        path=""
                        :label="t('storage.root')"
                        :current="moveTarget"
                        @open="moveTarget = $event"
                    />
                </ul>
                <p class="m-0 text-[13px] text-muted">
                    {{ t('storage.moveTarget', { folder: moveTarget ? baseName(moveTarget) : t('storage.root') }) }}
                </p>
            </div>
        </GenericModal>

        <GenericModal
            :show="preview !== null"
            :title="preview?.name ?? ''"
            :modal-action="() => (preview = null)"
            :hide-buttons="true"
        >
            <div v-if="preview" class="w-5xl max-w-full aspect-video flex items-center justify-center">
                <VideoPlayer v-if="preview.video" reference="previewPlayer" :options="preview.options" />
                <img v-else :src="preview.url" class="max-w-full max-h-[70vh] object-contain" :alt="preview.name" />
            </div>
        </GenericModal>
    </div>
</template>
