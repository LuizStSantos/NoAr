<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useHead } from '@unhead/vue'
import { useRoute, useRouter } from 'vue-router'
import dayjs from 'dayjs'

import { useConfig } from '@/stores/config'
import { useIndex } from '@/stores/index'
import { useMedia } from '@/stores/media'
import { useSchedule } from '@/stores/schedule'
import { useUploads } from '@/stores/uploads'

import FolderTree from '@/components/storage/FolderTree.vue'
import ImageViewer, { type ViewerImage } from '@/components/storage/ImageViewer.vue'
import LiveCard from '@/components/storage/LiveCard.vue'
import LiveEditor from '@/components/storage/LiveEditor.vue'
import SheetCard from '@/components/storage/SheetCard.vue'
import SheetEditor from '@/components/storage/SheetEditor.vue'
import GenericModal from '@/components/utils/GenericModal.vue'
import VideoPlayer from '@/components/utils/VideoPlayer.vue'

import { authFetch } from '@/composables/authFetch'
import { createFilePreviewUrl } from '@/composables/fileAccess'
import { stringFormatter } from '@/composables/helper'
import { baseName, imageUrl, readSheet, storageJoin, uploadArtwork } from '@/composables/storage'

const { t, locale } = useI18n()
const configStore = useConfig()
const indexStore = useIndex()
const mediaStore = useMedia()
const route = useRoute()
const router = useRouter()
const scheduleStore = useSchedule()
const { mediaType, fileSize } = stringFormatter()
const uploads = useUploads()

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

type FileView = 'videos' | 'images' | 'all'

const FILE_VIEWS: FileView[] = ['videos', 'images', 'all']

const view = ref<FileView>(initialView())
const filter = ref('')
const selected = ref<string[]>([])
const sheet = ref<MediaSheet | null>(null)
const editorPath = ref<string | null>(null)
const treeVersion = ref(0)
const disk = ref<{ total: number; free: number } | null>(null)
const dropActive = ref(false)
const uploadInput = ref<HTMLInputElement>()

const renameTarget = ref<Row | null>(null)
const renameValue = ref('')
const showCreate = ref(false)
const newFolderName = ref('')
const deleteNames = ref<string[]>([])
const deleteRecursive = ref(false)
const moveNames = ref<string[]>([])
const moveTarget = ref('')
const preview = ref<{ name: string; options: any; failed: boolean } | null>(null)

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

function initialView(): FileView {
    const saved = readSetting('storage-view')

    if (FILE_VIEWS.includes(saved as FileView)) {
        return saved as FileView
    }

    // The earlier two-way choice.
    return readSetting('storage-all-files') === 'true' ? 'all' : 'videos'
}

const rows = computed<Row[]>(() => {
    const needle = filter.value.trim().toLowerCase()
    const folders: Row[] = mediaStore.folderTree.folders.map((folder) => ({ name: folder.name, folder: true }))
    const files: Row[] = mediaStore.folderTree.files
        .filter((file) => view.value === 'all' || isLiveFolder.value || isVideo(file))
        .map((file) => ({ name: file.name, folder: false, file }))

    return [...folders, ...files].filter((row) => !needle || row.name.toLowerCase().includes(needle))
})

/** Playable video or audio; images can be in the channel's extensions too. */
function isVideo(file?: FileObject): boolean {
    return Boolean(file) && file?.media !== false && !isImage(file?.name ?? '')
}

function isImage(name: string): boolean {
    return IMAGE_TYPES.includes(extension(name))
}

const videos = computed(() => mediaStore.folderTree.files.filter((file) => isVideo(file)))
const images = computed(() => mediaStore.folderTree.files.filter((file) => isImage(file.name)))
const shownImages = computed(() => {
    const needle = filter.value.trim().toLowerCase()

    return images.value.filter((file) => !needle || file.name.toLowerCase().includes(needle))
})

// Images are shown from short-lived addresses, fetched a few at a time.
const imageUrls = reactive(new Map<string, string>())
const imageDims = reactive(new Map<string, string>())
const retried = new Set<string>()
let imagesFolder: string | null = null

/** Changes when the file does, so a replaced image is not taken from the cache. */
function imageKey(file: FileObject): string {
    return `${file.name}:${file.modified ?? ''}:${file.size ?? ''}`
}

async function loadImageUrls() {
    const folder = current.value

    if (imagesFolder !== folder) {
        imageUrls.clear()
        imageDims.clear()
        retried.clear()
        imagesFolder = folder
    }

    if (view.value === 'videos' || !channelId.value) {
        return
    }

    const queue = images.value.filter((file) => !imageUrls.has(imageKey(file)))

    async function worker() {
        for (let file = queue.shift(); file; file = queue.shift()) {
            try {
                const url = await imageUrl(channelId.value, storageJoin(folder, file.name))

                if (folder === current.value) {
                    imageUrls.set(imageKey(file), url)
                }
            } catch {
                // The row keeps its icon.
            }
        }
    }

    await Promise.all([worker(), worker(), worker(), worker()])
}

/** An address expired (they last a quarter of an hour): ask for a new one once. */
function imageFailed(file: FileObject) {
    const key = imageKey(file)

    if (!retried.has(key)) {
        retried.add(key)
        imageUrls.delete(key)
        loadImageUrls()
    }
}

function imageLoaded(event: Event, file: FileObject) {
    const element = event.target as HTMLImageElement

    if (element.naturalWidth) {
        imageDims.set(imageKey(file), `${element.naturalWidth}×${element.naturalHeight}`)
    }
}

const imagesSummary = computed(() => {
    const bytes = images.value.reduce((sum, file) => sum + (file.size ?? 0), 0)

    return t(
        'storage.images.summary',
        { count: images.value.length, size: fileSize(bytes, 1) || '0 B' },
        images.value.length,
    )
})

/** Whose artwork the open folder holds, for the image labels. */
const artworkOwner = computed<'series' | 'movie' | 'folder'>(() => {
    if (!sheet.value?.file) {
        return 'folder'
    }

    return sheet.value.kind === 'movie' ? 'movie' : 'series'
})

/** Name of the open folder's poster or wide image, when it is a file here. */
function artworkHere(path: string | null | undefined): string | null {
    if (!path) {
        return null
    }

    const parts = storageJoin(path).split('/')
    const name = parts.pop() ?? ''

    return parts.join('/') === current.value ? name : null
}

const posterHere = computed(() => artworkHere(sheet.value?.poster))
const backdropHere = computed(() => artworkHere(sheet.value?.backdrop))

const viewerIndex = ref<number | null>(null)
const viewerBusy = ref(false)
const viewerImages = computed<ViewerImage[]>(() =>
    shownImages.value.map((file) => ({
        key: imageKey(file),
        name: file.name,
        url: imageUrls.get(imageKey(file)) ?? null,
        size: file.size ?? 0,
        dims: imageDims.get(imageKey(file)) ?? null,
    })),
)

// The image on screen was deleted or renamed away.
watch(
    () => viewerImages.value.length,
    (length) => {
        if (viewerIndex.value !== null && viewerIndex.value >= length) {
            viewerIndex.value = length ? length - 1 : null
        }
    },
)

function openViewer(name: string) {
    const position = shownImages.value.findIndex((file) => file.name === name)

    if (position >= 0) {
        viewerIndex.value = position
        loadImageUrls()
    }
}

/** Make an image of the folder its poster or wide image. */
async function useAsArtwork(kind: ArtworkKind, name: string) {
    const file = images.value.find((item) => item.name === name)
    const url = file ? imageUrls.get(imageKey(file)) : null

    if (!url) {
        return
    }

    viewerBusy.value = true

    try {
        const response = await fetch(url)

        if (!response.ok) {
            throw new Error(response.statusText || String(response.status))
        }

        const blob = await response.blob()

        await cardArtwork(kind, new File([blob], name, { type: blob.type }))
    } catch (e) {
        failed(e)
    } finally {
        viewerBusy.value = false
    }
}
/** How each video of the open folder shows up in the guide: title and episode. */
const guide = ref<Record<string, { title: string; sub_title: string | null }>>({})
/** The open folder's videos use a series or film sheet (its own or a parent's). */
const folderSheet = computed(() => {
    if (!sheet.value || (!sheet.value.file && !sheet.value.inherited_from)) {
        return null
    }

    return sheet.value.kind === 'movie' ? 'movie' : 'series'
})

async function loadGuide() {
    const storage = (configStore.channels[configStore.i]?.storage ?? '').replace(/\/+$/, '')
    const files = videos.value.slice(0, 300)

    if (!files.length || !storage) {
        guide.value = {}
        return
    }

    try {
        const found = await authFetch<{ title: string; sub_title: string | null }[]>(
            `/api/playlist/${channelId.value}/programs`,
            {
                method: 'POST',
                headers: configStore.contentType,
                body: JSON.stringify(
                    files.map((file) => ({ source: `${storage}/${storageJoin(current.value, file.name)}` })),
                ),
            },
        )

        guide.value = Object.fromEntries(
            files.map((file, index) => [file.name, found[index] ?? { title: '', sub_title: null }]),
        )
    } catch {
        guide.value = {}
    }
}
const summary = computed(() => {
    const seconds = videos.value.reduce((sum, file) => sum + (file.duration || 0), 0)

    return t('storage.summary', { count: videos.value.length, length: clock(seconds) }, videos.value.length)
})
const allSelected = computed(
    () => rows.value.length > 0 && rows.value.every((row) => selected.value.includes(row.name)),
)
const sheetFolders = computed(() => mediaStore.folderTree.sheet_folders ?? [])
const liveFolders = computed(() => mediaStore.folderTree.live_folders ?? [])

// A live's folder: its address file, the live as the Scheduler knows it, and
// the last signal test.
const liveFile = computed(() => mediaStore.folderTree.files.find((file) => extension(file.name) === 'live'))
const isLiveFolder = computed(() => Boolean(current.value && liveFile.value))
const currentLive = computed(() => {
    const file = liveFile.value

    if (!file) {
        return null
    }

    const path = storageJoin(current.value, file.name)

    return scheduleStore.lives.find((live) => live.path === path) ?? null
})
const liveEditor = ref<{ live: LiveSource | null } | null>(null)
const liveTests = reactive(new Map<string, { ok: boolean; text: string; at: string }>())
const liveTesting = ref(false)

const liveNext = computed(() => {
    const path = currentLive.value?.path

    if (!path) {
        return ''
    }

    const active = scheduleStore.status.active

    if (active?.kind === 'live' && active.item === path) {
        return t('storage.live.onAirNow')
    }

    const next = scheduleStore.status.next.find(
        (occurrence) => occurrence.event.kind === 'live' && occurrence.event.sources.includes(path),
    )

    if (!next) {
        return t('storage.live.noSchedule')
    }

    return t('storage.live.nextAt', {
        day:
            next.start.slice(0, 10) === dayjs().format('YYYY-MM-DD')
                ? t('storage.live.today')
                : t('storage.live.tomorrow'),
        start: next.start.slice(11, 16),
        end: next.end.slice(11, 16),
    })
})

async function loadLiveInfo() {
    await Promise.all([
        scheduleStore.loadLives().catch(() => undefined),
        scheduleStore.loadStatus().catch(() => undefined),
    ])
}

async function testLive() {
    const live = currentLive.value

    if (!live) {
        return
    }

    liveTesting.value = true

    try {
        const result = await scheduleStore.testSignal(live.url)

        liveTests.set(live.path, { ok: result.ok, text: result.error ?? '', at: dayjs().format('HH:mm') })
    } catch (e) {
        liveTests.set(live.path, {
            ok: false,
            text: e instanceof Error ? e.message : String(e),
            at: dayjs().format('HH:mm'),
        })
    } finally {
        liveTesting.value = false
    }
}

function scheduleLive(path?: string) {
    if (path) {
        router.push({ path: '/schedule', query: { live: path } })
    }
}

async function liveSaved(live: LiveSource, schedule: boolean) {
    liveEditor.value = null
    await loadLiveInfo()

    if (schedule) {
        scheduleLive(live.path)
        return
    }

    const folder = live.path.split('/').slice(0, -1).join('/')

    treeVersion.value++

    if (folder === current.value) {
        await refresh()
    } else {
        await load(folder)
    }
}

/** What each file of a live's folder is for. */
function liveRole(name: string): string {
    const lower = name.toLowerCase()

    if (extension(lower) === 'live') {
        return t('storage.live.fileAddress')
    }

    if (lower === 'tvshow.nfo') {
        return t('storage.live.fileGuide')
    }

    if (isImage(lower) && lower.startsWith('poster')) {
        return t('storage.live.filePoster')
    }

    if (isImage(lower) && /^(fanart|backdrop|background|landscape)/.test(lower)) {
        return t('storage.live.fileBackdrop')
    }

    return ''
}
const movieFolders = computed(() => mediaStore.folderTree.movie_folders ?? [])

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

    if (ext === 'live') {
        return 'bi-broadcast'
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
    await mediaStore.getTree(`/${path}`, false, true)
}

// A queued file reached the folder on screen (or the folder of its sheet).
watch(
    () => uploads.landed,
    (landed) => {
        if (landed && landed.channelId === channelId.value && landed.folder === current.value) {
            refresh()
        }
    },
)

async function refresh() {
    await mediaStore.getTree(`/${current.value}`, false, true)
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

// A link to a folder while the page is open (the uploads panel, for example).
watch(
    () => route.query.path,
    (path) => {
        if (typeof path === 'string' && storageJoin(path) !== current.value) {
            load(storageJoin(path))
        }
    },
)

watch(current, loadSheet, { immediate: true })
watch(isLiveFolder, (live) => live && loadLiveInfo(), { immediate: true })
watch(
    () => [mediaStore.folderTree.files, sheet.value],
    () => loadGuide(),
)

watch(channelId, async () => {
    await load('')
    loadDisk()
})

watch(view, (value) => {
    writeSetting('storage-view', value)
    selected.value = []
})

watch([() => mediaStore.folderTree.files, view], () => loadImageUrls())

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
    const files = Array.from(event.dataTransfer?.files ?? [])

    if (files.length) {
        // Files from the computer go straight into that folder.
        dropActive.value = false
        upload(files, target)
        return
    }

    const names = droppedNames(event)

    if (names.length) {
        move(names, target)
    }
}

/** A drop on a folder row goes into that folder, not the open one. */
function dropOnRow(row: Row, event: DragEvent) {
    if (!row.folder) {
        return
    }

    event.stopPropagation()
    dropOnFolder(storageJoin(current.value, row.name), event)
}

function allowFolderDrop(event: DragEvent) {
    const types = event.dataTransfer?.types ?? []

    if (types.includes(DRAG_TYPE) || types.includes('Files')) {
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

/** Queue files for a folder (the open one by default); they upload in the
 * background while the page is used, or left. */
function upload(files: File[], folder = current.value) {
    const existing = new Set(folder === current.value ? mediaStore.folderTree.files.map((file) => file.name) : [])
    const fresh = files.filter((file) => {
        if (existing.has(file.name)) {
            indexStore.msgAlert('warning', t('storage.fileExists', { name: file.name }), 4)
            return false
        }

        if (uploads.pending(channelId.value, folder, file.name)) {
            indexStore.msgAlert('warning', t('storage.alreadyQueued', { name: file.name }), 4)
            return false
        }

        return true
    })

    if (!fresh.length) {
        return
    }

    uploads.add(channelId.value, folder, fresh)
}

function uploadNfo(file: File) {
    // Saved as the folder's program sheet, whatever the file was called.
    upload([new File([file], 'tvshow.nfo', { type: 'text/xml' })])
}

async function cardArtwork(kind: ArtworkKind, file: File) {
    try {
        sheet.value = await uploadArtwork(channelId.value, current.value, kind, file)
        indexStore.msgAlert('success', t('storage.artwork.saved'), 2)
        await mediaStore.getTree(`/${current.value}`, false, true)
    } catch (e) {
        failed(e)
    }
}

async function onSheetSaved(next: MediaSheet) {
    if (!next.clip) {
        sheet.value = next
    }

    await mediaStore.getTree(`/${current.value}`, false, true)
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
        const ext = extension(row.name)

        preview.value = {
            name: row.name,
            failed: false,
            options: {
                liveui: false,
                controls: true,
                autoplay: true,
                playsinline: true,
                // The page says it in its own words, below the player.
                errorDisplay: false,
                preload: 'auto',
                sources: [
                    {
                        type: mediaType(row.name) === 'audio' ? `audio/${ext}` : `video/${ext}`,
                        src: url,
                    },
                ],
            },
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
            <button type="button" class="btn-quiet min-h-10.5 border-live/60" @click="liveEditor = { live: null }">
                <i class="bi bi-broadcast text-live" aria-hidden="true" />{{ t('storage.live.new') }}
            </button>
            <button type="button" class="btn-main min-h-10.5" @click="uploadInput?.click()">
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
                <LiveCard
                    v-if="isLiveFolder"
                    :channel-id="channelId"
                    :sheet="sheet"
                    :live="currentLive"
                    :next="liveNext"
                    :test="currentLive ? (liveTests.get(currentLive.path) ?? null) : null"
                    :testing="liveTesting"
                    @edit="liveEditor = { live: currentLive }"
                    @test="testLive"
                    @poster="cardArtwork('poster', $event)"
                    @schedule="scheduleLive(currentLive?.path)"
                />
                <SheetCard
                    v-else-if="current && sheet"
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
                        <h2 class="m-0 text-[15px] font-semibold">
                            {{ isLiveFolder ? t('storage.live.files') : t('storage.files') }}
                        </h2>
                        <span v-if="!isLiveFolder" class="font-mono text-xs bg-raised rounded-md px-2 py-1">{{
                            view === 'images' ? imagesSummary : summary
                        }}</span>
                        <div class="flex-auto" />
                        <div
                            v-if="!isLiveFolder"
                            role="group"
                            :aria-label="t('storage.show')"
                            class="inline-flex bg-base-200 border border-line-strong rounded-field p-[3px]"
                        >
                            <button
                                v-for="option in FILE_VIEWS"
                                :key="option"
                                type="button"
                                :aria-pressed="view === option"
                                class="border-0 rounded-md px-3 min-h-8 text-[13px] cursor-pointer inline-flex items-center gap-1.5"
                                :class="view === option ? 'bg-base-300 text-base-content' : 'bg-transparent text-muted'"
                                @click="view = option"
                            >
                                {{
                                    option === 'videos'
                                        ? t('storage.videos')
                                        : option === 'images'
                                          ? t('storage.images.tab')
                                          : t('storage.allFiles')
                                }}
                                <span v-if="option !== 'all'" class="font-mono text-xs">{{
                                    option === 'videos' ? videos.length : images.length
                                }}</span>
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

                    <div v-if="view === 'images' && !isLiveFolder" class="relative">
                        <div
                            v-if="mediaStore.isLoading"
                            class="absolute inset-0 z-10 flex justify-center pt-10 bg-base-100/60"
                        >
                            <span class="loading loading-spinner loading-lg" />
                        </div>
                        <ul
                            v-if="shownImages.length"
                            class="m-0 p-4 list-none grid grid-cols-[repeat(auto-fill,minmax(180px,1fr))] gap-3.5"
                        >
                            <li
                                v-for="file in shownImages"
                                :key="file.name"
                                class="group relative bg-raised border border-line rounded-xl overflow-hidden hover:border-line-strong focus-within:border-accent"
                                draggable="true"
                                @dragstart="dragRow($event, { name: file.name, folder: false, file })"
                            >
                                <button
                                    type="button"
                                    class="w-full h-[150px] bg-base-300 border-0 p-0 cursor-pointer flex items-center justify-center"
                                    :aria-label="t('storage.images.view', { name: file.name })"
                                    @click="openViewer(file.name)"
                                >
                                    <img
                                        v-if="imageUrls.get(imageKey(file))"
                                        :src="imageUrls.get(imageKey(file))"
                                        alt=""
                                        class="max-w-full max-h-full object-contain"
                                        loading="lazy"
                                        @load="imageLoaded($event, file)"
                                        @error="imageFailed(file)"
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
                                        @click="openViewer(file.name)"
                                    >
                                        <i class="bi bi-eye" aria-hidden="true" />
                                    </button>
                                    <button
                                        type="button"
                                        class="btn-icon bg-base-100/90 text-base-content"
                                        :aria-label="t('storage.rename', { name: file.name })"
                                        @click="startRename({ name: file.name, folder: false, file })"
                                    >
                                        <i class="bi bi-pencil" aria-hidden="true" />
                                    </button>
                                    <button
                                        type="button"
                                        class="btn-icon bg-base-100/90 text-error"
                                        :aria-label="t('storage.remove', { name: file.name })"
                                        @click="deleteNames = [file.name]"
                                    >
                                        <i class="bi bi-trash" aria-hidden="true" />
                                    </button>
                                </div>
                                <div class="px-3 py-2.5 flex flex-col gap-1 text-[13px]">
                                    <span class="truncate" :title="file.name">{{ file.name }}</span>
                                    <span class="font-mono text-xs text-muted">{{
                                        [imageDims.get(imageKey(file)), fileSize(file.size ?? 0, 1) || '0 B']
                                            .filter(Boolean)
                                            .join(' · ')
                                    }}</span>
                                    <span
                                        v-if="file.name === posterHere"
                                        class="self-start text-[11px] text-ok border border-ok/40 rounded-md px-1.5 leading-4"
                                    >
                                        {{ t(`storage.images.posterBadge.${artworkOwner}`) }}
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
                        <p v-else-if="!mediaStore.isLoading" class="m-0 px-4 pt-6 pb-2 text-muted">
                            {{ t('storage.images.empty') }} {{ t('storage.images.emptyHint') }}
                        </p>
                    </div>

                    <div v-else class="relative overflow-x-auto">
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
                                    <th class="px-2 py-2.5 font-medium">{{ t('storage.guideColumn') }}</th>
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
                                    @drop.prevent="dropOnRow(row, $event)"
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
                                                v-if="liveFolders.includes(row.name)"
                                                class="text-[11px] text-live border border-live/50 rounded-md px-1.5 leading-4"
                                            >
                                                {{ t('storage.live.badge') }}
                                            </span>
                                            <span
                                                v-else-if="sheetFolders.includes(row.name)"
                                                class="text-[11px] text-accent-text border border-accent/40 rounded-md px-1.5 leading-4"
                                            >
                                                {{
                                                    movieFolders.includes(row.name)
                                                        ? t('storage.movieBadge')
                                                        : t('storage.seriesBadge')
                                                }}
                                            </span>
                                        </button>
                                        <span v-else class="flex items-center gap-2.5 min-h-7 min-w-0">
                                            <img
                                                v-if="
                                                    row.file && isImage(row.name) && imageUrls.get(imageKey(row.file))
                                                "
                                                :src="imageUrls.get(imageKey(row.file))"
                                                alt=""
                                                class="size-7 shrink-0 rounded object-cover bg-base-300"
                                                loading="lazy"
                                                @load="imageLoaded($event, row.file)"
                                                @error="imageFailed(row.file)"
                                            />
                                            <i
                                                v-else
                                                class="bi"
                                                :class="[
                                                    fileIcon(row),
                                                    extension(row.name) === 'live' ? 'text-live' : 'text-muted',
                                                ]"
                                                aria-hidden="true"
                                            />
                                            <button
                                                v-if="isImage(row.name)"
                                                type="button"
                                                class="truncate bg-transparent border-0 p-0 text-inherit cursor-pointer hover:underline text-left"
                                                @click="openViewer(row.name)"
                                            >
                                                {{ row.name }}
                                            </button>
                                            <button
                                                v-else-if="isVideo(row.file)"
                                                type="button"
                                                class="truncate bg-transparent border-0 p-0 text-inherit cursor-pointer hover:underline text-left"
                                                :aria-label="t('storage.preview', { name: row.name })"
                                                @click="openPreview(row)"
                                            >
                                                {{ row.name }}
                                            </button>
                                            <span
                                                v-else
                                                class="truncate"
                                                :class="row.file?.media === false ? 'text-muted' : ''"
                                            >
                                                {{ row.name }}
                                            </span>
                                            <span
                                                v-if="isLiveFolder && liveRole(row.name)"
                                                class="text-xs text-subtle whitespace-nowrap"
                                            >
                                                · {{ liveRole(row.name) }}
                                            </span>
                                        </span>
                                    </td>
                                    <td class="px-2 py-2 font-mono">
                                        {{ isVideo(row.file) ? clock(row.file?.duration ?? 0) : '' }}
                                    </td>
                                    <td class="px-2 py-2 font-mono text-muted whitespace-nowrap">
                                        {{ row.file ? fileSize(row.file.size, 1) || '0 B' : '' }}
                                    </td>
                                    <td class="px-2 py-2 text-muted whitespace-nowrap">
                                        {{ row.file ? modifiedLabel(row.file) : '' }}
                                    </td>
                                    <td class="px-2 py-2 min-w-48">
                                        <span v-if="row.file && guide[row.name]?.title" class="block truncate max-w-64">
                                            {{ guide[row.name]?.title }}
                                            <span v-if="guide[row.name]?.sub_title" class="text-muted">
                                                · {{ guide[row.name]?.sub_title }}
                                            </span>
                                        </span>
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
                                                    t('storage.ownSheet')
                                                }}
                                            </button>
                                            <span
                                                v-else-if="folderSheet"
                                                class="inline-flex items-center gap-2 whitespace-nowrap"
                                            >
                                                <span class="text-muted">
                                                    {{
                                                        folderSheet === 'movie'
                                                            ? t('storage.movieSheetUsed')
                                                            : t('storage.seriesSheetUsed')
                                                    }}
                                                </span>
                                                <button
                                                    type="button"
                                                    class="link-accent text-xs"
                                                    :aria-label="t('storage.createEpisodeSheet', { name: row.name })"
                                                    @click="editorPath = storageJoin(current, row.name)"
                                                >
                                                    + {{ t('storage.episodeShort') }}
                                                </button>
                                            </span>
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
                                                :aria-label="
                                                    isImage(row.name)
                                                        ? t('storage.images.view', { name: row.name })
                                                        : t('storage.preview', { name: row.name })
                                                "
                                                @click="isImage(row.name) ? openViewer(row.name) : openPreview(row)"
                                            >
                                                <i
                                                    class="bi"
                                                    :class="isImage(row.name) ? 'bi-eye' : 'bi-play-fill'"
                                                    aria-hidden="true"
                                                />
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
            <div v-if="preview" class="w-5xl max-w-full flex flex-col gap-3">
                <div class="w-full aspect-video bg-black rounded-field overflow-hidden">
                    <VideoPlayer
                        reference="previewPlayer"
                        :options="preview.options"
                        @error="preview && (preview.failed = true)"
                    />
                </div>
                <p v-if="preview.failed" class="m-0 text-[13px] text-error" role="alert">
                    {{ t('storage.previewFailed') }}
                </p>
            </div>
        </GenericModal>

        <LiveEditor
            v-if="liveEditor"
            :channel-id="channelId"
            :live="liveEditor.live"
            :sheet="liveEditor.live ? sheet : null"
            @close="liveEditor = null"
            @saved="liveSaved"
        />

        <ImageViewer
            v-if="viewerIndex !== null && viewerImages.length"
            v-model="viewerIndex"
            :images="viewerImages"
            :folder="crumbs[crumbs.length - 1]?.name ?? ''"
            :poster="posterHere"
            :backdrop="backdropHere"
            :owner="artworkOwner"
            :can-use="Boolean(current)"
            :busy="viewerBusy"
            @close="viewerIndex = null"
            @use="useAsArtwork"
            @loaded="(key, dims) => imageDims.set(key, dims)"
        />
    </div>
</template>
