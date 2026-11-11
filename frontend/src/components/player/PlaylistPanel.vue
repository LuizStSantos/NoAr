<script setup lang="ts">
import dayjs from 'dayjs'
import 'dayjs/locale/de'
import 'dayjs/locale/pt-br'
import 'dayjs/locale/ru'

import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { cloneDeep } from 'es-toolkit/object'
import { onClickOutside, useElementSize, useEventListener } from '@vueuse/core'

import ClipEditor from '@/components/player/ClipEditor.vue'
import StoragePicker from '@/components/schedule/StoragePicker.vue'
import GenericModal from '@/components/utils/GenericModal.vue'
import { registerEditorNavigationGuard } from '@/composables/editorNavigation'
import { playlistOperations } from '@/composables/helper'
import { useOnAir } from '@/composables/onAir'
import {
    FOLDER_DRAG_TYPE,
    LIBRARY_DRAG_TYPE,
    type AddPlace,
    ROW_DRAG_TYPE,
    clockShort,
    clockTime,
    fileName,
    isRemoteSource,
    lengthLabel,
    totalLength,
    type LibraryFile,
    fileInOut,
} from '@/composables/player'
import { addDays } from '@/composables/schedule'
import { usePlaylistRows } from '@/composables/playlistRows'
import { usePlaylistDayActions } from '@/composables/playlistDayActions'
import PlaylistDayMap from '@/components/player/playlist/PlaylistDayMap.vue'
import PlaylistDayDialogs from '@/components/player/playlist/PlaylistDayDialogs.vue'
import { useConfig } from '@/stores/config'
import { useIndex } from '@/stores/index'
import { usePlaylist } from '@/stores/playlist'
import { useProgrammes } from '@/stores/programmes'

const props = withDefaults(
    defineProps<{
        /** Shown and in use; its keys stay off while the Player shows the air. */
        active?: boolean
        /** Fill the height given: the header stays, the list scrolls inside. */
        fill?: boolean
    }>(),
    { active: true, fill: false },
)
const emit = defineEmits<{ preview: [path: string]; generate: []; addProgram: [folder: string, index: number] }>()

const { t, locale } = useI18n()
const { genUID } = playlistOperations()
const configStore = useConfig()
const indexStore = useIndex()
const playlistStore = usePlaylist()
const programmes = useProgrammes()
const onAir = useOnAir()
const { onAirDate, onAirIndex } = onAir

/** The list's scroller: on a phone the table is wider and scrolls sideways. */
const tableScroller = ref<HTMLElement>()
const { width: scrollerWidth } = useElementSize(tableScroller)
/** The table scrolls sideways: the clip editor then keeps to the visible width. */
const narrowTable = computed(() => scrollerWidth.value > 0 && scrollerWidth.value < 640)

const editing = ref<{ uid: string; item: PlaylistItem } | null>(null)
/** Clips just edited; their program stays open so the change can be seen. */
const recent = ref<string[]>([])
const newRemote = ref<PlaylistItem | null>(null)
const picker = ref<'source' | 'audio' | null>(null)
const dropAt = ref<number | null>(null)
const dragRow = ref<string | null>(null)
const menuOpen = ref(false)
const menu = ref<HTMLElement>()
const dateInput = ref<HTMLInputElement>()
const {
    saving,
    showCopy,
    copyDate,
    showImport,
    importFile,
    showDelete,
    showDiscard,
    save,
    discard,
    openCopy,
    copyTo,
    importList,
    fillDay,
    deleteList,
} = usePlaylistDayActions(load, () => {
    menuOpen.value = false
})
const pendingDate = ref<string | null>(null)

/** Grouped by program, or one running list of clips. */
const view = ref<'program' | 'flat'>(readSetting('player-view') === 'flat' ? 'flat' : 'program')
/** uids of the selected clips. */
const selected = ref<string[]>([])
/** Last clip clicked, where a Shift-click range starts. */
const anchorUid = ref<string | null>(null)
const moveTime = ref<string | null>(null)

function readSetting(key: string): string | null {
    try {
        return localStorage.getItem(key)
    } catch {
        return null
    }
}

watch(selected, (uids) => (playlistStore.selectedUids = uids))

watch(view, (value) => {
    try {
        localStorage.setItem('player-view', value)
    } catch {
        // Only a convenience: the choice is not kept.
    }
})

onClickOutside(menu, () => (menuOpen.value = false))

const channel = computed(() => configStore.channels[configStore.i])
const storage = computed(() => channel.value?.storage ?? '')
const infinite = computed(() => Boolean(configStore.playout.playlist?.infinit))
const dayStart = computed(() => configStore.playout.playlist?.startInSec ?? 0)

const dateLabel = computed(() =>
    dayjs(`${playlistStore.listDate}T12:00:00`).locale(locale.value.toLowerCase()).format('ddd, D MMM YYYY'),
)
const shortDateLabel = computed(() =>
    dayjs(`${playlistStore.listDate}T12:00:00`).locale(locale.value.toLowerCase()).format('D MMM'),
)
const totalSeconds = computed(() => playlistStore.playlist.reduce((sum, item) => sum + (item.out - item.in), 0))
const unsaved = computed(() => (playlistStore.dirty ? Math.max(1, playlistStore.editCount) : 0))
/** How long a day is; the list should fill it. */
const dayLength = computed(() => configStore.playout.playlist?.lengthInSec || 86400)
/** Seconds the list misses (positive) or runs over (negative) the day. */
const dayGap = computed(() => (infinite.value ? 0 : Math.round(dayLength.value - totalSeconds.value)))
/** When the last clip ends. */
const listEnd = computed(() => {
    const last = playlistStore.playlist[playlistStore.playlist.length - 1]

    return last ? (last.begin ?? 0) + (last.out - last.in) : dayStart.value
})

// ---------------------------------------------------------------------------
// Loading

function listKey(date: string) {
    return `${channel.value?.id}:${date}`
}

async function load(date: string) {
    playlistStore.listDate = date
    editing.value = null
    newRemote.value = null
    recent.value = []
    selected.value = []
    resetGroups()

    await playlistStore.getPlaylist(date)
    await scrollToOnAir()
}

function changeDate(date: string) {
    if (date === playlistStore.listDate) {
        return
    }

    if (playlistStore.dirty) {
        pendingDate.value = date
        return
    }

    playlistStore.firstLoad = false
    load(date)
}

function confirmDateChange(discard: boolean) {
    const date = pendingDate.value
    pendingDate.value = null

    if (discard && date) {
        playlistStore.firstLoad = false
        load(date)
    }
}

function openDatePicker() {
    const input = dateInput.value

    if (input && 'showPicker' in input) {
        try {
            input.showPicker()
            return
        } catch {
            // Fall through to focusing the input.
        }
    }

    input?.focus()
}

async function scrollToOnAir() {
    // The program names merge the first folder groups and move every row, so
    // the on-air clip is only looked for once they are in.
    await programmes.refresh()
    await nextTick()

    const index = onAirIndex.value

    if (index < 0) {
        return
    }

    const uid = playlistStore.playlist[index]?.uid

    setTimeout(() => {
        document.getElementById(`clip-${uid}`)?.scrollIntoView({ block: 'center', behavior: 'smooth' })
    }, 300)
}

onMounted(() => {
    const date = playlistStore.firstLoad ? onAirDate.value : playlistStore.listDate

    if (playlistStore.dirty && playlistStore.loadedKey === listKey(date)) {
        playlistStore.listDate = date
        return
    }

    load(date)
})

// Another channel replaces the playlist: ask first while it has unsaved
// changes, so a channel picked by mistake does not lose them.
let channelSwitchConfirmed = false
const unregisterNavigationGuard = registerEditorNavigationGuard((to) => {
    const target = Number(Array.isArray(to.query.channel) ? to.query.channel[0] : to.query.channel)

    if (!playlistStore.dirty || !Number.isSafeInteger(target) || target === channel.value?.id) {
        return true
    }
    if (!window.confirm(t('onair.unsavedChannelQuestion'))) {
        return false
    }

    channelSwitchConfirmed = true
    return true
})

watch(
    () => channel.value?.id,
    (id, previous) => {
        if (id && previous && id !== previous) {
            // Changed without asking (a channel removed, for one): say so.
            if (playlistStore.dirty && !channelSwitchConfirmed) {
                indexStore.msgAlert('warning', t('onair.unsavedDropped'), 4)
            }
            channelSwitchConfirmed = false

            load(playlistStore.firstLoad ? onAirDate.value : playlistStore.listDate)
        }
    },
)

// Shown again: bring the clip on air into view, as on opening the page.
watch(
    () => props.active,
    (active) => {
        if (active) {
            scrollToOnAir()
        }
    },
)

watch(
    () => playlistStore.scrollToItem,
    (scroll) => {
        if (scroll) {
            playlistStore.scrollToItem = false
            scrollToOnAir()
        }
    },
)

// A drag that ends outside the table must not leave the drop marker behind.
useEventListener(window, 'dragend', () => dragEnd())
useEventListener(window, 'drop', () => dragEnd())

useEventListener(window, 'beforeunload', (event: BeforeUnloadEvent) => {
    if (playlistStore.dirty) {
        event.preventDefault()
    }
})

// ---------------------------------------------------------------------------
// Grouping and rows

const { rows, dayMap, toggleGroup, setAllGroups, openGroup, showMore, groupSummary, resetGroups } = usePlaylistRows({
    view,
    editing,
    recent,
    onAir,
})

/** Open a programme from the day map and bring its first row into view. */
async function jumpTo(id: string) {
    openGroup(id)
    await nextTick()
    document.getElementById(`group-${id}`)?.scrollIntoView({ block: 'start', behavior: 'smooth' })
}

// ---------------------------------------------------------------------------
// Selection and actions on several clips

const selectedIndices = computed(() =>
    playlistStore.playlist
        .map((item, index) => (selected.value.includes(item.uid) ? index : -1))
        .filter((index) => index >= 0),
)
const selectionIsBreak = computed(() => selectedIndices.value.every((index) => playlistStore.playlist[index]?.ad))

function toggleSelect(item: PlaylistItem, event: MouseEvent) {
    const items = playlistStore.playlist
    const anchor = anchorUid.value ? items.findIndex((entry) => entry.uid === anchorUid.value) : -1
    const index = items.findIndex((entry) => entry.uid === item.uid)

    if (event.shiftKey && anchor >= 0 && index >= 0) {
        const [from, to] = anchor < index ? [anchor, index] : [index, anchor]
        const range = items.slice(from, to + 1).map((entry) => entry.uid)

        selected.value = [...new Set([...selected.value, ...range])]
    } else if (selected.value.includes(item.uid)) {
        selected.value = selected.value.filter((uid) => uid !== item.uid)
    } else {
        selected.value = [...selected.value, item.uid]
    }

    anchorUid.value = item.uid
}

/** Take the selected clips out of the list, keeping their order. */
function takeSelected(): PlaylistItem[] {
    const taken = playlistStore.playlist.filter((item) => selected.value.includes(item.uid))

    playlistStore.playlist = playlistStore.playlist.filter((item) => !selected.value.includes(item.uid))

    return taken
}

/** Move the selected clips one place up or down, as one block. */
function shiftSelection(step: -1 | 1) {
    const indices = selectedIndices.value

    if (!indices.length) {
        return
    }

    const first = indices[0] as number
    const last = indices[indices.length - 1] as number

    if ((step < 0 && first === 0) || (step > 0 && last === playlistStore.playlist.length - 1)) {
        return
    }

    const neighbour = playlistStore.playlist[step < 0 ? first - 1 : last + 1]
    const taken = takeSelected()
    const at = playlistStore.playlist.findIndex((item) => item.uid === neighbour?.uid)

    playlistStore.playlist.splice(step < 0 ? at : at + 1, 0, ...taken)
    playlistStore.touch()
}

function duplicateSelection() {
    const indices = selectedIndices.value
    // A copy is another entry: the marks of one entry stay with it.
    const copies = indices.map((index) => ({
        ...cloneDeep(playlistStore.playlist[index]!),
        uid: genUID(),
        marks: undefined,
    }))

    if (copies.length) {
        insertAt((indices[indices.length - 1] as number) + 1, copies)
        selected.value = copies.map((item) => item.uid)
    }
}

function toggleBreakSelection() {
    const value = !selectionIsBreak.value

    for (const index of selectedIndices.value) {
        const item = playlistStore.playlist[index]

        if (item) {
            item.ad = value
        }
    }

    playlistStore.touch()
}

function removeSelection() {
    const count = takeSelected().length

    selected.value = []
    playlistStore.touch()

    if (count) {
        indexStore.msgAlert('success', t('onair.removedClips', { count }, count), 3)
    }
}

/** Move the selected clips to the start or the end of the list. */
function moveSelectionTo(where: 'start' | 'end') {
    const taken = takeSelected()

    if (!taken.length) {
        return
    }

    if (where === 'start') {
        playlistStore.playlist.unshift(...taken)
    } else {
        playlistStore.playlist.push(...taken)
    }

    taken.forEach((item) => markAdded(item.uid))
    playlistStore.touch()
}

function undo() {
    playlistStore.undo()
    selected.value = selected.value.filter((uid) => playlistStore.playlist.some((item) => item.uid === uid))
}

function redo() {
    playlistStore.redo()
}

/** Keys for the playlist: undo, redo, and acting on the selection. */
useEventListener(window, 'keydown', (event: KeyboardEvent) => {
    const target = event.target as HTMLElement | null
    const typing =
        target?.isContentEditable ||
        (target instanceof HTMLInputElement && !['checkbox', 'radio', 'button'].includes(target.type)) ||
        target instanceof HTMLTextAreaElement ||
        target instanceof HTMLSelectElement

    // Nothing while hidden, typing or while a dialog is open.
    if (
        !props.active ||
        typing ||
        editing.value ||
        document.querySelector('[role="dialog"][aria-modal="true"], dialog[open]')
    ) {
        return
    }

    const key = event.key.toLowerCase()
    const command = event.ctrlKey || event.metaKey

    if (command && key === 'z' && !event.shiftKey) {
        event.preventDefault()
        undo()
    } else if (command && (key === 'y' || (key === 'z' && event.shiftKey))) {
        event.preventDefault()
        redo()
    } else if (selected.value.length && (event.key === 'Delete' || event.key === 'Backspace')) {
        event.preventDefault()
        removeSelection()
    } else if (selected.value.length && event.key === 'Escape') {
        selected.value = []
    } else if (selected.value.length && event.altKey && (event.key === 'ArrowUp' || event.key === 'ArrowDown')) {
        event.preventDefault()
        shiftSelection(event.key === 'ArrowUp' ? -1 : 1)
    }
})

function openMoveTime() {
    const first = playlistStore.playlist[selectedIndices.value[0] ?? -1]

    moveTime.value = clockTime(first?.begin ?? dayStart.value).slice(0, 8)
}

/** Put the selected clips before the clip that plays at the chosen time. */
function moveToTime(confirm: boolean) {
    const value = moveTime.value

    moveTime.value = null

    if (!confirm || !value) {
        return
    }

    const [hours = 0, minutes = 0, seconds = 0] = value.split(':').map(Number)
    let target = hours * 3600 + minutes * 60 + seconds

    if (target < dayStart.value) {
        target += 86400
    }

    const taken = takeSelected()
    const at = playlistStore.playlist.findIndex((item) => (item.begin ?? 0) + (item.out - item.in) > target)

    playlistStore.playlist.splice(at < 0 ? playlistStore.playlist.length : at, 0, ...taken)
    taken.forEach((item) => markAdded(item.uid))
    playlistStore.touch()
}

// ---------------------------------------------------------------------------
// Editing

function markAdded(uid: string) {
    playlistStore.addedUids = [...playlistStore.addedUids, uid]
}

function itemFromFile(file: LibraryFile): PlaylistItem {
    return {
        uid: genUID(),
        begin: 0,
        source: file.source,
        ...fileInOut(file),
        duration: file.duration,
        ad: false,
    }
}

function insertAt(index: number, items: PlaylistItem[]) {
    playlistStore.playlist.splice(index, 0, ...items)
    items.forEach((item) => markAdded(item.uid))
    playlistStore.touch()
}

function placeIndex(place: AddPlace): number {
    if (typeof place === 'number') {
        return Math.min(Math.max(0, place), playlistStore.playlist.length)
    }

    if (place === 'onair' && onAirIndex.value >= 0) {
        return onAirIndex.value + 1
    }

    if (place === 'selection' && selectedIndices.value.length) {
        return (selectedIndices.value[selectedIndices.value.length - 1] as number) + 1
    }

    return playlistStore.playlist.length
}

/** Add several clips together (a program) and say where they start. */
function addFiles(files: LibraryFile[], place: AddPlace, label: string) {
    const index = placeIndex(place)

    insertAt(index, files.map(itemFromFile))
    playlistStore.scrollToItem = true
    indexStore.msgAlert(
        'success',
        t(
            'onair.addedProgram',
            {
                name: label,
                count: files.length,
                time: clockTime(playlistStore.playlist[index]?.begin ?? 0),
            },
            files.length,
        ),
        3,
    )
}

/**
 * Add a file from the library: after the selected clips when some are
 * selected, else after the clip on air, else at the end. The message says
 * which.
 */
function addFile(file: LibraryFile) {
    const place: AddPlace = selectedIndices.value.length ? 'selection' : onAirIndex.value >= 0 ? 'onair' : 'end'
    const index = placeIndex(place)
    const item = itemFromFile(file)

    insertAt(index, [item])
    // Several files added one after the other stay in that order.
    if (place === 'selection') {
        selected.value = [item.uid]
    }

    indexStore.msgAlert(
        'success',
        t('onair.addedWhere', {
            name: file.name,
            time: clockTime(playlistStore.playlist[index]?.begin ?? 0),
            where: t(`onair.addedPlace.${place}`),
        }),
        3,
    )
}

function removeItem(index: number) {
    playlistStore.playlist.splice(index, 1)
    playlistStore.touch()
}

function startEdit(item: PlaylistItem) {
    newRemote.value = null
    editing.value = { uid: item.uid, item: cloneDeep(item) }
}

function applyEdit(updated: PlaylistItem, parts: number) {
    if (!editing.value) {
        return
    }

    const index = playlistStore.playlist.findIndex((item) => item.uid === editing.value?.uid)

    if (index >= 0) {
        const items = split(updated, parts)

        playlistStore.playlist.splice(index, 1, ...items)
        recent.value = items.map((item) => item.uid)
        playlistStore.touch()
    }

    editing.value = null
}

function addRemote() {
    menuOpen.value = false
    editing.value = null
    newRemote.value = { uid: genUID(), begin: 0, source: '', in: 0, out: 60, duration: 60, ad: false }
}

function applyRemote(item: PlaylistItem, parts: number) {
    insertAt(playlistStore.playlist.length, split(item, parts))
    newRemote.value = null
}

/** Cut a clip into equal parts that play one after the other. */
function split(item: PlaylistItem, parts: number): PlaylistItem[] {
    if (parts <= 1) {
        return [item]
    }

    const length = (item.out - item.in) / parts

    return Array.from({ length: parts }, (_, part) => ({
        ...cloneDeep(item),
        uid: part === 0 ? item.uid : genUID(),
        in: item.in + length * part,
        out: part === parts - 1 ? item.out : item.in + length * (part + 1),
    }))
}

function pick(file: { path: string; name: string; duration: number }) {
    const target = editing.value?.item ?? newRemote.value
    const absolute = `${storage.value.replace(/\/+$/, '')}/${file.path}`

    if (target && picker.value === 'source') {
        target.source = absolute
        target.duration = file.duration || 10
        target.in = 0
        target.out = target.duration
    } else if (target && picker.value === 'audio') {
        target.audio = absolute
    }

    picker.value = null
}

// ---------------------------------------------------------------------------
// Drag and drop

function hasPayload(event: DragEvent) {
    const types = event.dataTransfer?.types ?? []

    return types.includes(LIBRARY_DRAG_TYPE) || types.includes(ROW_DRAG_TYPE) || types.includes(FOLDER_DRAG_TYPE)
}

function rowDragStart(event: DragEvent, item: PlaylistItem) {
    const uids = selected.value.includes(item.uid) ? selected.value : [item.uid]

    dragRow.value = item.uid
    event.dataTransfer?.setData(ROW_DRAG_TYPE, JSON.stringify(uids))

    if (event.dataTransfer) {
        event.dataTransfer.effectAllowed = 'move'
    }
}

function dragOverRow(event: DragEvent, before: number, after: number) {
    if (!hasPayload(event)) {
        return
    }

    event.preventDefault()

    const target = event.currentTarget as HTMLElement
    const rect = target.getBoundingClientRect()

    dropAt.value = event.clientY - rect.top < rect.height / 2 ? before : after

    if (event.dataTransfer) {
        event.dataTransfer.dropEffect = event.dataTransfer.types.includes(ROW_DRAG_TYPE) ? 'move' : 'copy'
    }
}

function dragOverEnd(event: DragEvent) {
    if (!hasPayload(event)) {
        return
    }

    event.preventDefault()
    dropAt.value = playlistStore.playlist.length
}

function dragEnd() {
    dropAt.value = null
    dragRow.value = null
}

function drop(event: DragEvent) {
    const index = dropAt.value
    const media = event.dataTransfer?.getData(LIBRARY_DRAG_TYPE)
    const folder = event.dataTransfer?.getData(FOLDER_DRAG_TYPE)
    const uid = event.dataTransfer?.getData(ROW_DRAG_TYPE)

    event.preventDefault()
    dragEnd()

    if (index === null) {
        return
    }

    if (folder) {
        // A whole program: ask what to add from it.
        emit('addProgram', folder, index)
        return
    }

    if (media) {
        insertAt(index, [itemFromFile(JSON.parse(media) as LibraryFile)])
        return
    }

    let uids: string[] = []

    try {
        uids = JSON.parse(uid || '[]')
    } catch {
        uids = uid ? [uid] : []
    }

    const items = playlistStore.playlist
    const moving = items.filter((item) => uids.includes(item.uid))
    const target = items[index]

    if (!moving.length || (target && uids.includes(target.uid))) {
        return
    }

    // Insert before the clip that was at the drop place, or at the end.
    const rest = items.filter((item) => !uids.includes(item.uid))
    const at = target ? rest.findIndex((item) => item.uid === target.uid) : rest.length

    rest.splice(at, 0, ...moving)
    playlistStore.playlist = rest
    moving.forEach((item) => markAdded(item.uid))
    playlistStore.touch()
}

function dropClass(before: number, after: number) {
    if (dropAt.value === before) {
        return 'drop-before'
    }

    return dropAt.value === after ? 'drop-after' : ''
}

// ---------------------------------------------------------------------------
// Saving and the actions menu

/** Start a list by hand: take the user to the library search. */
function focusLibrary() {
    const search = document.getElementById('library-search') as HTMLInputElement | null

    search?.scrollIntoView({ block: 'center', behavior: 'smooth' })
    search?.focus()
}

onBeforeUnmount(() => {
    menuOpen.value = false
    unregisterNavigationGuard()
})

defineExpose({ addFile, addFiles })
</script>
<template>
    <section
        :aria-label="t('onair.playlist')"
        class="card-box min-w-0"
        :class="fill ? 'lg:h-full lg:flex lg:flex-col lg:min-h-0' : ''"
    >
        <div
            class="flex flex-wrap items-center gap-x-3.5 gap-y-2.5 px-4 py-3.5 border-b border-line bg-base-100 rounded-t-box z-10"
            :class="fill ? '' : 'lg:sticky lg:top-[57px]'"
        >
            <h2 class="m-0 text-[15px] font-semibold">{{ t('onair.playlist') }}</h2>
            <div v-if="!infinite" class="flex items-center bg-raised border border-line-strong rounded-field">
                <button
                    class="btn-icon"
                    :aria-label="t('onair.previousDay')"
                    @click="changeDate(addDays(playlistStore.listDate, -1))"
                >
                    <i class="bi bi-chevron-left text-xs" aria-hidden="true" />
                </button>
                <button
                    class="px-1 min-h-9 capitalize cursor-pointer"
                    :aria-label="t('onair.pickDay')"
                    @click="openDatePicker"
                >
                    {{ dateLabel }}
                </button>
                <input
                    ref="dateInput"
                    type="date"
                    class="sr-only"
                    tabindex="-1"
                    :value="playlistStore.listDate"
                    @change="changeDate(($event.target as HTMLInputElement).value)"
                />
                <button
                    class="btn-icon"
                    :aria-label="t('onair.nextDay')"
                    @click="changeDate(addDays(playlistStore.listDate, 1))"
                >
                    <i class="bi bi-chevron-right text-xs" aria-hidden="true" />
                </button>
            </div>
            <button
                v-if="!infinite && playlistStore.listDate !== onAirDate"
                class="link-accent text-[13px]"
                @click="changeDate(onAirDate)"
            >
                {{ t('onair.backToToday') }}
            </button>
            <span class="font-mono text-xs text-muted bg-raised rounded-md px-2 py-1">
                {{ totalLength(totalSeconds) }} ·
                {{
                    t(
                        'onair.clipCount',
                        { total: playlistStore.playlist.length.toLocaleString(locale) },
                        playlistStore.playlist.length,
                    )
                }}
            </span>
            <span
                v-if="playlistStore.playlist.length && Math.abs(dayGap) >= 1"
                class="font-mono text-xs rounded-md px-2 py-1"
                :class="dayGap > 0 ? 'bg-accent-soft text-accent-text' : 'bg-error/15 text-error'"
                :title="t('onair.dayLengthHint', { length: totalLength(dayLength) })"
            >
                {{
                    dayGap > 0
                        ? t('onair.dayMissing', { time: totalLength(dayGap) })
                        : t('onair.dayOver', { time: totalLength(-dayGap) })
                }}
            </span>
            <div
                role="group"
                :aria-label="t('onair.viewMode')"
                class="inline-flex bg-base-200 border border-line-strong rounded-field p-[3px]"
            >
                <button
                    v-for="mode in ['program', 'flat'] as const"
                    :key="mode"
                    type="button"
                    :aria-pressed="view === mode"
                    class="border-0 rounded-md px-3 min-h-8 text-[13px] cursor-pointer"
                    :class="view === mode ? 'bg-base-300 text-base-content' : 'bg-transparent text-muted'"
                    @click="view = mode"
                >
                    {{ mode === 'program' ? t('onair.viewByProgram') : t('onair.viewFlat') }}
                </button>
            </div>
            <template v-if="view === 'program' && programmes.groups.length > 1">
                <button type="button" class="link-accent text-[13px] text-muted" @click="setAllGroups(true)">
                    {{ t('onair.openAll') }}
                </button>
                <button type="button" class="link-accent text-[13px] text-muted" @click="setAllGroups(false)">
                    {{ t('onair.closeAll') }}
                </button>
            </template>
            <div class="flex-auto" />
            <div class="flex flex-wrap items-center gap-2">
                <span v-if="unsaved" class="inline-flex items-center gap-1.5 text-accent-text text-[13px]">
                    <span class="dot w-1.75 h-1.75 bg-accent" aria-hidden="true" />{{ t('onair.unsaved', unsaved) }}
                </span>
                <button
                    class="btn-icon border border-line-strong min-w-9.5 min-h-9.5"
                    :disabled="!playlistStore.undoCount"
                    :aria-label="t('onair.undo')"
                    :title="t('onair.undoHint')"
                    @click="undo"
                >
                    <i class="bi bi-arrow-counterclockwise" aria-hidden="true" />
                </button>
                <button
                    class="btn-icon border border-line-strong min-w-9.5 min-h-9.5"
                    :disabled="!playlistStore.redoCount"
                    :aria-label="t('onair.redo')"
                    :title="t('onair.redoHint')"
                    @click="redo"
                >
                    <i class="bi bi-arrow-clockwise" aria-hidden="true" />
                </button>
                <button
                    class="btn-quiet min-h-9.5"
                    :disabled="!playlistStore.dirty || playlistStore.isLoading"
                    :title="playlistStore.dirty ? '' : t('onair.noChanges')"
                    @click="showDiscard = true"
                >
                    {{ t('onair.discard') }}
                </button>
                <button
                    class="btn-main min-h-9.5"
                    :disabled="!playlistStore.dirty || saving || playlistStore.loadFailed || playlistStore.isLoading"
                    :title="playlistStore.dirty ? '' : t('onair.noChanges')"
                    @click="save()"
                >
                    {{ t('onair.save') }}
                </button>
            </div>
            <button class="btn-quiet min-h-9.5" @click="emit('generate')">
                <i class="bi bi-stars" aria-hidden="true" />{{ t('onair.generate') }}
            </button>
            <div ref="menu" class="relative">
                <button
                    class="btn-icon border border-line-strong min-w-9.5 min-h-9.5"
                    :aria-label="t('onair.moreActions')"
                    :aria-expanded="menuOpen"
                    @click="menuOpen = !menuOpen"
                >
                    <i class="bi bi-three-dots" aria-hidden="true" />
                </button>
                <div
                    v-if="menuOpen"
                    role="menu"
                    :aria-label="t('onair.moreActions')"
                    class="absolute end-0 mt-2 w-[340px] max-w-[calc(100vw-32px)] z-20 bg-raised border border-line-strong rounded-xl p-1.5 shadow-xl flex flex-col"
                >
                    <button
                        role="menuitem"
                        class="menu-row"
                        :disabled="!playlistStore.playlist.length"
                        @click="openCopy"
                    >
                        <i class="bi bi-copy" aria-hidden="true" />{{ t('onair.copyToDay') }}
                    </button>
                    <button role="menuitem" class="menu-row" @click="((menuOpen = false), (showImport = true))">
                        <i class="bi bi-download" aria-hidden="true" />{{ t('onair.importList') }}
                    </button>
                    <button role="menuitem" class="menu-row" @click="addRemote">
                        <i class="bi bi-link-45deg" aria-hidden="true" />{{ t('onair.addRemote') }}
                    </button>
                    <button
                        role="menuitem"
                        class="menu-row"
                        :disabled="infinite || !playlistStore.playlist.length"
                        @click="fillDay"
                    >
                        <i class="bi bi-repeat" aria-hidden="true" />{{ t('onair.fillDay') }}
                    </button>
                    <div class="h-px bg-line-strong my-1.5 mx-1" aria-hidden="true" />
                    <button
                        role="menuitem"
                        class="menu-row text-[#ff9da0]"
                        @click="((menuOpen = false), (showDelete = true))"
                    >
                        <i class="bi bi-trash3" aria-hidden="true" />{{
                            t('onair.deleteList', { date: shortDateLabel })
                        }}
                    </button>
                </div>
            </div>
        </div>

        <div v-if="newRemote" class="px-4 py-4 border-b border-line bg-raised">
            <ClipEditor :item="newRemote" :storage="storage" remote @apply="applyRemote" @cancel="newRemote = null" />
        </div>

        <PlaylistDayMap
            v-if="!infinite && playlistStore.playlist.length && !playlistStore.isLoading"
            :map="dayMap"
            @jump="jumpTo"
        />

        <div
            v-if="selected.length"
            class="bg-accent-soft border-b border-accent/30 px-4 py-2.5 flex flex-wrap items-center gap-x-3 gap-y-2 text-[13px]"
        >
            <span class="font-semibold">{{
                t('onair.selectedClips', { count: selected.length }, selected.length)
            }}</span>
            <button type="button" class="btn-quiet min-h-8.5 text-[13px]" @click="shiftSelection(-1)">
                <i class="bi bi-arrow-up" aria-hidden="true" />{{ t('onair.moveUp') }}
            </button>
            <button type="button" class="btn-quiet min-h-8.5 text-[13px]" @click="shiftSelection(1)">
                <i class="bi bi-arrow-down" aria-hidden="true" />{{ t('onair.moveDown') }}
            </button>
            <button type="button" class="btn-quiet min-h-8.5 text-[13px]" @click="moveSelectionTo('start')">
                <i class="bi bi-chevron-bar-up" aria-hidden="true" />{{ t('onair.moveToStart') }}
            </button>
            <button type="button" class="btn-quiet min-h-8.5 text-[13px]" @click="moveSelectionTo('end')">
                <i class="bi bi-chevron-bar-down" aria-hidden="true" />{{ t('onair.moveToEnd') }}
            </button>
            <button type="button" class="btn-quiet min-h-8.5 text-[13px]" @click="openMoveTime">
                {{ t('onair.moveToTime') }}
            </button>
            <button type="button" class="btn-quiet min-h-8.5 text-[13px]" @click="duplicateSelection">
                {{ t('onair.duplicate') }}
            </button>
            <button type="button" class="btn-quiet min-h-8.5 text-[13px]" @click="toggleBreakSelection">
                {{ selectionIsBreak ? t('onair.unmarkBreak') : t('onair.markBreak') }}
            </button>
            <button type="button" class="btn-quiet min-h-8.5 text-[13px] text-error" @click="removeSelection">
                {{ t('onair.removeSelected') }}
            </button>
            <div class="flex-auto" />
            <span class="text-xs text-muted hidden xl:inline">{{ t('onair.selectionKeys') }}</span>
            <button type="button" class="link-accent text-base-content/80" @click="selected = []">
                {{ t('onair.clearSelection') }}
            </button>
        </div>

        <div v-if="playlistStore.isLoading" class="py-16 flex justify-center">
            <span class="loading loading-spinner" />
        </div>

        <div
            v-else-if="!playlistStore.playlist.length"
            class="px-6 py-14 flex flex-col items-center gap-3 text-center text-muted"
            :class="{ 'bg-accent-soft/40': dropAt !== null }"
            @dragover="dragOverEnd"
            @dragleave="dropAt = null"
            @drop="drop"
        >
            <i class="bi bi-collection-play text-3xl" aria-hidden="true" />
            <p class="m-0 max-w-md">{{ t('onair.emptyList') }}</p>
            <div class="flex flex-wrap justify-center gap-2">
                <button class="btn-quiet" @click="focusLibrary">
                    <i class="bi bi-plus-lg" aria-hidden="true" />{{ t('onair.buildByHand') }}
                </button>
                <button class="btn-main" @click="emit('generate')">{{ t('onair.generate') }}</button>
                <button class="btn-quiet" @click="showImport = true">{{ t('onair.importList') }}</button>
            </div>
            <p class="m-0 max-w-md text-xs">{{ t('onair.buildByHandHint') }}</p>
        </div>

        <div
            v-else
            ref="tableScroller"
            class="relative overflow-x-auto"
            :class="fill ? 'lg:flex-1 lg:min-h-0 lg:overflow-y-auto' : ''"
        >
            <table class="w-full min-w-[640px] md:min-w-[760px] border-collapse text-[13px]" @dragend="dragEnd">
                <thead :class="fill ? 'lg:sticky lg:top-0 lg:z-[5] lg:bg-base-100' : ''">
                    <tr class="text-muted text-left text-xs">
                        <th class="ps-4 pe-1 py-2.5 w-[36px]">
                            <span class="sr-only">{{ t('onair.select') }}</span>
                        </th>
                        <th class="px-2 py-2.5 font-medium w-[96px]">{{ t('onair.time') }}</th>
                        <th class="px-2 py-2.5 font-medium">{{ t('onair.programFile') }}</th>
                        <th class="px-2 py-2.5 font-medium w-[72px]">{{ t('onair.length') }}</th>
                        <th class="hidden md:table-cell px-2 py-2.5 font-medium w-[120px]">{{ t('onair.inOut') }}</th>
                        <th class="px-4 py-2.5 w-[128px]">
                            <span class="sr-only">{{ t('onair.actions') }}</span>
                        </th>
                    </tr>
                </thead>
                <tbody @drop="drop">
                    <template
                        v-for="row in rows"
                        :key="
                            row.type === 'group'
                                ? `g-${row.id}`
                                : row.type === 'clip'
                                  ? row.item.uid
                                  : row.type === 'editor'
                                    ? `e-${row.index}`
                                    : row.type === 'more'
                                      ? `m-${row.id}-${row.direction}`
                                      : `l-${row.live.key}`
                        "
                    >
                        <tr
                            v-if="row.type === 'group'"
                            :id="`group-${row.id}`"
                            class="border-t border-line scroll-mt-20"
                            :class="[
                                row.status === 'past' ? 'text-subtle' : '',
                                row.expanded && row.status !== 'past'
                                    ? 'bg-[color-mix(in_srgb,var(--color-base-100)_92%,var(--color-base-content))]'
                                    : '',
                                dropClass(row.group.first, row.group.last + 1),
                            ]"
                            @dragover="dragOverRow($event, row.group.first, row.group.last + 1)"
                        >
                            <td />
                            <td class="px-2 py-2.5 font-mono" :class="row.status === 'past' ? '' : 'text-muted'">
                                {{ clockTime(row.group.start) }}
                            </td>
                            <td class="px-2 py-2.5" colspan="4">
                                <button
                                    class="flex flex-wrap items-center gap-x-2 gap-y-1 min-h-7 text-left bg-transparent border-0 p-0 cursor-pointer"
                                    :aria-expanded="row.expanded"
                                    @click="toggleGroup(row.id, row.expanded)"
                                >
                                    <i
                                        class="bi text-xs"
                                        :class="row.expanded ? 'bi-chevron-down' : 'bi-chevron-right'"
                                        aria-hidden="true"
                                    />
                                    <span
                                        class="font-semibold"
                                        :class="row.status === 'past' ? '' : 'text-base-content'"
                                        >{{ row.group.title }}</span
                                    >
                                    <span v-if="row.group.subTitle" :class="row.status === 'past' ? '' : 'text-muted'"
                                        >— {{ row.group.subTitle }}</span
                                    >
                                    <span :class="row.status === 'past' ? '' : 'text-muted'">{{
                                        groupSummary(row.group, row.status)
                                    }}</span>
                                    <span
                                        v-for="chip in row.chips"
                                        :key="chip.label"
                                        class="text-xs rounded-md px-2 py-0.5"
                                        :class="
                                            chip.kind === 'live'
                                                ? 'bg-live-soft text-live'
                                                : 'bg-accent-soft text-accent-text'
                                        "
                                    >
                                        {{ chip.label }}
                                    </span>
                                </button>
                            </td>
                        </tr>

                        <tr
                            v-else-if="row.type === 'clip'"
                            :id="`clip-${row.item.uid}`"
                            class="group border-t border-raised"
                            :class="[
                                row.index === onAirIndex ? 'bg-accent-soft' : '',
                                row.past ? 'text-subtle' : '',
                                row.item.overtime ? 'opacity-60' : '',
                                dragRow === row.item.uid ? 'opacity-40' : '',
                                dropClass(row.index, row.index + 1),
                            ]"
                            @dragover="dragOverRow($event, row.index, row.index + 1)"
                        >
                            <td class="ps-4 pe-1 py-1.5">
                                <input
                                    type="checkbox"
                                    class="checkbox checkbox-xs checkbox-accent rounded-[4px]"
                                    :checked="selected.includes(row.item.uid)"
                                    :aria-label="t('onair.selectClip', { name: fileName(row.item.source) })"
                                    @click="toggleSelect(row.item, $event)"
                                />
                            </td>
                            <td
                                class="py-1.5 font-mono"
                                :class="[
                                    view === 'program' ? 'ps-5 pe-2' : 'px-2',
                                    row.index === onAirIndex ? 'text-accent-text' : row.past ? '' : 'text-muted',
                                ]"
                            >
                                {{ clockTime(row.item.begin) }}
                            </td>
                            <td class="px-2 py-1.5 min-w-0">
                                <span class="flex flex-wrap items-center gap-x-2 gap-y-0.5 min-w-0">
                                    <span
                                        class="cursor-grab text-subtle flex-none"
                                        draggable="true"
                                        :title="t('onair.dragToMove')"
                                        @dragstart="rowDragStart($event, row.item)"
                                    >
                                        <svg
                                            width="12"
                                            height="14"
                                            viewBox="0 0 12 16"
                                            fill="currentColor"
                                            aria-hidden="true"
                                        >
                                            <circle cx="3.5" cy="3.5" r="1.2" />
                                            <circle cx="8.5" cy="3.5" r="1.2" />
                                            <circle cx="3.5" cy="8" r="1.2" />
                                            <circle cx="8.5" cy="8" r="1.2" />
                                            <circle cx="3.5" cy="12.5" r="1.2" />
                                            <circle cx="8.5" cy="12.5" r="1.2" />
                                        </svg>
                                    </span>
                                    <span
                                        v-if="row.index === onAirIndex"
                                        class="bg-on-air text-white text-[10px] font-semibold tracking-wider px-1.5 py-0.5 rounded flex-none"
                                    >
                                        {{ t('nav.onAir') }}
                                    </span>
                                    <span class="truncate max-w-full" :title="row.item.source">
                                        <template v-if="row.item.title">
                                            {{ row.item.title }}
                                            <span class="text-muted">· {{ fileName(row.item.source) }}</span>
                                        </template>
                                        <template v-else>{{
                                            isRemoteSource(row.item.source)
                                                ? row.item.source
                                                : fileName(row.item.source)
                                        }}</template>
                                    </span>
                                    <template v-if="programmes.isCommercial(row.index)">
                                        <span
                                            class="text-[11px] text-after-clip border border-after-clip/50 rounded px-1.5 flex-none"
                                            :title="t('onair.commercialHint')"
                                        >
                                            {{ t('onair.commercialTag') }}
                                        </span>
                                        <span class="hidden sm:inline text-xs text-subtle flex-none">{{
                                            t('onair.noLogo')
                                        }}</span>
                                    </template>
                                    <span
                                        v-else-if="row.item.ad || programmes.roleOf(row.index) === 'break'"
                                        class="text-[11px] text-muted border border-line-strong rounded px-1.5 flex-none"
                                    >
                                        {{ t('onair.breakTag') }}
                                    </span>
                                    <span
                                        v-else-if="programmes.roleOf(row.index) === 'insert'"
                                        class="text-[11px] text-after-clip border border-after-clip/40 rounded px-1.5 flex-none"
                                    >
                                        {{ t('onair.insertTag') }}
                                    </span>
                                    <span
                                        v-if="view === 'flat' && programmes.groupOf(row.index)"
                                        class="text-xs text-subtle truncate flex-none max-w-[30%]"
                                    >
                                        {{ programmes.groupOf(row.index)?.title }}
                                    </span>
                                    <span
                                        v-if="playlistStore.addedUids.includes(row.item.uid)"
                                        class="text-accent-text text-xs flex-none"
                                    >
                                        {{ t('onair.new') }}
                                    </span>
                                    <span v-if="row.item.overtime" class="text-error text-xs flex-none">{{
                                        t('onair.overtime')
                                    }}</span>
                                </span>
                            </td>
                            <td class="px-2 py-1.5 font-mono">{{ lengthLabel(row.item.out - row.item.in) }}</td>
                            <td
                                class="hidden md:table-cell px-2 py-1.5 font-mono"
                                :class="row.past ? '' : 'text-muted'"
                            >
                                {{ lengthLabel(row.item.in) }} · {{ lengthLabel(row.item.out) }}
                            </td>
                            <td class="px-4 py-1.5 text-right whitespace-nowrap">
                                <span class="row-actions">
                                    <button
                                        class="btn-icon"
                                        :aria-label="t('onair.preview')"
                                        @click="emit('preview', row.item.source)"
                                    >
                                        <i class="bi bi-play-fill" aria-hidden="true" />
                                    </button>
                                    <button class="btn-icon" :aria-label="t('onair.edit')" @click="startEdit(row.item)">
                                        <i class="bi bi-pencil" aria-hidden="true" />
                                    </button>
                                    <button
                                        class="btn-icon"
                                        :aria-label="t('onair.remove')"
                                        @click="removeItem(row.index)"
                                    >
                                        <i class="bi bi-x-lg" aria-hidden="true" />
                                    </button>
                                </span>
                            </td>
                        </tr>

                        <tr v-else-if="row.type === 'editor' && editing" class="border-t border-raised bg-raised">
                            <td v-if="!narrowTable" />
                            <td v-if="!narrowTable" class="px-2 pt-3 pb-1 font-mono align-top">
                                {{ clockTime(playlistStore.playlist[row.index]?.begin ?? 0) }}
                            </td>
                            <td
                                class="pt-3 pb-4"
                                :class="narrowTable ? 'p-0' : 'px-2 pe-4'"
                                :colspan="narrowTable ? 6 : 4"
                            >
                                <!-- On a phone the editor stays in view while the table scrolls sideways. -->
                                <div
                                    :class="narrowTable ? 'sticky left-0 px-4' : ''"
                                    :style="narrowTable ? { width: `${scrollerWidth}px` } : undefined"
                                >
                                    <ClipEditor
                                        :item="editing.item"
                                        :storage="storage"
                                        :commercial="
                                            programmes.isCommercial(
                                                playlistStore.playlist.findIndex((item) => item.uid === editing?.uid),
                                            )
                                        "
                                        @apply="applyEdit"
                                        @cancel="editing = null"
                                        @pick="picker = $event"
                                    />
                                </div>
                            </td>
                        </tr>

                        <tr v-else-if="row.type === 'more'" class="border-t border-raised">
                            <td class="ps-14 pe-4 py-2" colspan="6">
                                <button class="link-accent min-h-7" @click="showMore(row.id, row.direction)">
                                    {{
                                        row.direction === 'before'
                                            ? t('onair.showEarlier', row.count)
                                            : t('onair.showMore', row.count)
                                    }}
                                </button>
                            </td>
                        </tr>

                        <tr v-else-if="row.type === 'live'" class="border-t border-line bg-live-soft">
                            <td />
                            <td class="px-2 py-2.5 font-mono text-live">{{ clockTime(row.live.start) }}</td>
                            <td class="px-2 py-2.5" colspan="4">
                                <span class="flex flex-wrap items-center gap-2 min-h-7">
                                    <span
                                        class="bg-live text-base-100 text-[10px] font-semibold tracking-wider px-1.5 py-0.5 rounded"
                                        >LIVE</span
                                    >
                                    <span class="font-semibold">{{ row.live.title }}</span>
                                    <span class="text-muted">
                                        · {{ clockShort(row.live.start) }}–{{ clockShort(row.live.end) }}
                                        <template v-if="row.live.protocol"> · {{ row.live.protocol }}</template>
                                        ·
                                        {{
                                            row.live.status === 'running'
                                                ? t('onair.liveRunning')
                                                : t('onair.liveScheduled')
                                        }}
                                    </span>
                                    <RouterLink
                                        :to="{ path: '/schedule', query: { channel: channel?.id } }"
                                        class="link-accent ms-auto text-[13px]"
                                    >
                                        {{ t('onair.openScheduler') }}
                                    </RouterLink>
                                </span>
                            </td>
                        </tr>
                    </template>
                    <tr
                        class="border-t border-line"
                        :class="dropAt === playlistStore.playlist.length ? 'drop-before' : ''"
                        @dragover="dragOverEnd"
                    >
                        <td colspan="6" class="px-4 py-3 text-xs text-subtle text-center">
                            <span v-if="!infinite" class="font-mono me-1">
                                {{ t('onair.listEndsAt', { time: clockTime(listEnd) }) }}
                                <template v-if="dayGap >= 1">
                                    · {{ t('onair.dayMissing', { time: totalLength(dayGap) }) }}</template
                                >
                                ·
                            </span>
                            <button
                                v-if="!infinite && dayGap >= 1"
                                type="button"
                                class="link-accent text-xs me-1"
                                @click="fillDay"
                            >
                                {{ t('onair.fillDay') }}
                            </button>
                            {{ t('onair.dropAtEnd') }} · {{ t('onair.selectHint') }}
                        </td>
                    </tr>
                </tbody>
            </table>
        </div>

        <GenericModal
            :show="picker !== null"
            :title="picker === 'audio' ? t('onair.chooseAudio') : t('onair.chooseFile')"
            hide-buttons
            :modal-action="() => (picker = null)"
        >
            <div class="w-[min(520px,90vw)] max-h-[60vh] flex flex-col">
                <StoragePicker @pick="pick" />
            </div>
        </GenericModal>

        <GenericModal
            :show="moveTime !== null"
            :title="t('onair.moveToTitle', { count: selected.length }, selected.length)"
            :ok-label="t('onair.move')"
            :modal-action="moveToTime"
        >
            <div class="flex flex-col gap-3 w-[min(380px,85vw)]">
                <label class="field-label">
                    {{ t('onair.moveToLabel') }}
                    <input
                        :value="moveTime ?? ''"
                        type="time"
                        step="1"
                        class="field-input font-mono"
                        @input="moveTime = ($event.target as HTMLInputElement).value"
                    />
                </label>
                <p class="m-0 text-sm text-muted">{{ t('onair.moveToHint') }}</p>
            </div>
        </GenericModal>

        <PlaylistDayDialogs
            v-model:show-copy="showCopy"
            v-model:copy-date="copyDate"
            v-model:show-import="showImport"
            v-model:import-file="importFile"
            v-model:show-delete="showDelete"
            v-model:show-discard="showDiscard"
            v-model:pending-date="pendingDate"
            :date-label="dateLabel"
            :short-date-label="shortDateLabel"
            :unsaved="unsaved"
            @copy="copyTo"
            @import="importList"
            @remove="deleteList"
            @discard="discard"
            @change-date="confirmDateChange"
        />
    </section>
</template>
<style scoped>
@reference '@/assets/css/main.css';

.menu-row {
    @apply flex items-center gap-3 px-3 min-h-11 rounded-field text-left bg-transparent border-0 cursor-pointer;
}

.menu-row:hover:not(:disabled) {
    @apply bg-base-300;
}

.menu-row:disabled {
    @apply opacity-45 cursor-not-allowed;
}

.drop-before > td {
    box-shadow: inset 0 2px 0 var(--color-accent);
}

.drop-after > td {
    box-shadow: inset 0 -2px 0 var(--color-accent);
}

.row-actions {
    opacity: 0;
    transition: opacity 0.12s;
}

tr:hover .row-actions,
tr:focus-within .row-actions {
    opacity: 1;
}

@media (hover: none) {
    .row-actions {
        opacity: 1;
    }
}
</style>
