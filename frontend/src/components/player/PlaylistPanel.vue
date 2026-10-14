<script setup lang="ts">
import dayjs from 'dayjs'
import 'dayjs/locale/de'
import 'dayjs/locale/pt-br'
import 'dayjs/locale/ru'

import { computed, nextTick, onBeforeUnmount, onMounted, reactive, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { cloneDeep } from 'es-toolkit/object'
import { onClickOutside, useEventListener } from '@vueuse/core'

import ClipEditor from '@/components/player/ClipEditor.vue'
import StoragePicker from '@/components/schedule/StoragePicker.vue'
import GenericModal from '@/components/utils/GenericModal.vue'
import { authFetch } from '@/composables/authFetch'
import { playlistOperations } from '@/composables/helper'
import { useOnAir } from '@/composables/onAir'
import {
    LIBRARY_DRAG_TYPE,
    ROW_DRAG_TYPE,
    clockShort,
    clockTime,
    fileName,
    isRemoteSource,
    lengthLabel,
    totalLength,
    type LibraryFile,
} from '@/composables/player'
import { addDays, secondsFromDay, splitDateTime } from '@/composables/schedule'
import { useAuth } from '@/stores/auth'
import { useConfig } from '@/stores/config'
import { useIndex } from '@/stores/index'
import { usePlaylist } from '@/stores/playlist'
import { useProgrammes, type ProgrammeGroup } from '@/stores/programmes'
import { useSchedule } from '@/stores/schedule'

const emit = defineEmits<{ preview: [path: string]; generate: [] }>()

const { t, locale } = useI18n()
const { genUID } = playlistOperations()
const authStore = useAuth()
const configStore = useConfig()
const indexStore = useIndex()
const playlistStore = usePlaylist()
const programmes = useProgrammes()
const scheduleStore = useSchedule()
const { onAirDate, onAirIndex, playlistNow, viewingOnAir } = useOnAir()

const PAGE = 10
const MORE = 20

/** Groups the user opened or closed; others follow `defaultExpanded`. */
const toggled = reactive(new Map<string, boolean>())
/** Extra rows revealed above and below the default window of a group. */
const reveal = reactive(new Map<string, { before: number; after: number }>())

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
const saving = ref(false)

const showCopy = ref(false)
const copyDate = ref('')
const showImport = ref(false)
const importFile = ref<File | null>(null)
const showDelete = ref(false)
const pendingDate = ref<string | null>(null)

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
    toggled.clear()
    reveal.clear()

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

watch(
    () => channel.value?.id,
    (id, previous) => {
        if (id && previous && id !== previous) {
            if (playlistStore.dirty) {
                indexStore.msgAlert('warning', t('onair.unsavedDropped'), 4)
            }

            load(playlistStore.firstLoad ? onAirDate.value : playlistStore.listDate)
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

interface LiveRow {
    key: string
    title: string
    start: number
    end: number
    protocol: string
    status: RunStatus | null
}

interface GroupChip {
    label: string
    kind: 'live' | 'event'
}

const dayEvents = computed(() => {
    const date = playlistStore.listDate

    if (scheduleStore.date !== date) {
        return []
    }

    return scheduleStore.occurrences
        .map((occurrence) => ({
            occurrence,
            start: secondsFromDay(occurrence.start, date),
            end: secondsFromDay(occurrence.end, date),
        }))
        .filter((event) => event.start >= dayStart.value)
})

const lives = computed<LiveRow[]>(() =>
    dayEvents.value
        .filter((event) => event.occurrence.event.kind === 'live')
        .map((event) => ({
            key: `${event.occurrence.event.id}-${event.occurrence.start}`,
            title: event.occurrence.event.title,
            start: event.start,
            end: event.end,
            protocol: protocolOf(event.occurrence.event.sources[0] ?? ''),
            status: event.occurrence.status,
        })),
)

function protocolOf(path: string): string {
    const live = scheduleStore.lives.find((item) => item.path === path)

    return live?.protocol?.toUpperCase() ?? ''
}

/** Scheduled clips that land inside a program; lives get their own row. */
function chipsOf(group: ProgrammeGroup): GroupChip[] {
    return dayEvents.value
        .filter((event) => event.occurrence.event.kind !== 'live')
        .filter((event) => group.start < event.start && event.start < group.end)
        .map((event) => ({
            kind: 'event',
            label: t('onair.eventChip', {
                title: event.occurrence.event.title,
                time: splitDateTime(event.occurrence.start).time.slice(0, 5),
                mode:
                    event.occurrence.event.entry_mode === 'interrupt'
                        ? t('onair.eventInterrupts')
                        : t('onair.eventAfterClip'),
            }),
        }))
}

/** Groups get an id that survives edits inside them: key plus occurrence count. */
const keyedGroups = computed(() => {
    const seen = new Map<string, number>()

    return programmes.groups.map((group) => {
        const count = seen.get(group.key) ?? 0
        seen.set(group.key, count + 1)

        return { group, id: `${group.key}#${count}` }
    })
})

function groupStatus(group: ProgrammeGroup): 'past' | 'current' | 'future' {
    if (onAirIndex.value >= 0) {
        if (group.last < onAirIndex.value) {
            return 'past'
        }

        return group.first <= onAirIndex.value ? 'current' : 'future'
    }

    if (viewingOnAir.value && !infinite.value && playlistStore.playoutIsRunning) {
        if (group.end <= playlistNow.value) {
            return 'past'
        }

        return group.start <= playlistNow.value ? 'current' : 'future'
    }

    return 'future'
}

function containsUid(group: ProgrammeGroup, uids: string[]): number {
    for (let index = group.last; index >= group.first; index--) {
        if (uids.includes(playlistStore.playlist[index]?.uid ?? '')) {
            return index
        }
    }

    return -1
}

/** The row a group's window is centred on. */
function anchorOf(group: ProgrammeGroup): number {
    if (editing.value) {
        const index = containsUid(group, [editing.value.uid])

        if (index >= 0) {
            return index
        }
    }

    const added = containsUid(group, [...playlistStore.addedUids, ...recent.value])

    if (added >= 0) {
        return added
    }

    if (group.first <= onAirIndex.value && onAirIndex.value <= group.last) {
        return onAirIndex.value
    }

    return group.first
}

function isExpanded(id: string, group: ProgrammeGroup, position: number): boolean {
    const choice = toggled.get(id)

    if (choice !== undefined) {
        return choice
    }

    if (groupStatus(group) === 'current') {
        return true
    }

    // Programs with the clip being edited, or clips just added or changed.
    const touched = [...playlistStore.addedUids, ...recent.value, ...(editing.value ? [editing.value.uid] : [])]

    if (containsUid(group, touched) >= 0) {
        return true
    }

    // Nothing on air in this list: open the first program.
    return onAirIndex.value < 0 && position === 0 && !viewingOnAir.value
}

function toggleGroup(id: string, expanded: boolean) {
    toggled.set(id, !expanded)
}

type Row =
    | { type: 'group'; id: string; group: ProgrammeGroup; status: string; expanded: boolean; chips: GroupChip[] }
    | { type: 'clip'; index: number; item: PlaylistItem; past: boolean }
    | { type: 'editor'; index: number }
    | { type: 'more'; id: string; direction: 'before' | 'after'; count: number }
    | { type: 'live'; live: LiveRow }

const rows = computed<Row[]>(() => {
    const list: Row[] = []
    const pendingLives = [...lives.value].sort((a, b) => a.start - b.start)

    keyedGroups.value.forEach(({ group, id }, position) => {
        while (pendingLives[0] && pendingLives[0].start <= group.start) {
            list.push({ type: 'live', live: pendingLives.shift()! })
        }

        const status = groupStatus(group)
        const expanded = isExpanded(id, group, position)

        list.push({ type: 'group', id, group, status, expanded, chips: chipsOf(group) })

        if (!expanded) {
            return
        }

        const extra = reveal.get(id) ?? { before: 0, after: 0 }
        const anchor = anchorOf(group)
        const from = Math.max(group.first, anchor - 1 - extra.before)
        const to = Math.min(group.last, Math.max(anchor, from) + PAGE - 1 + extra.after)

        if (from > group.first) {
            list.push({ type: 'more', id, direction: 'before', count: Math.min(MORE, from - group.first) })
        }

        for (let index = from; index <= to; index++) {
            const item = playlistStore.playlist[index]

            if (!item) {
                continue
            }

            if (editing.value?.uid === item.uid) {
                list.push({ type: 'editor', index })
            } else {
                list.push({ type: 'clip', index, item, past: onAirIndex.value >= 0 && index < onAirIndex.value })
            }
        }

        if (to < group.last) {
            list.push({ type: 'more', id, direction: 'after', count: Math.min(MORE, group.last - to) })
        }
    })

    for (const live of pendingLives) {
        list.push({ type: 'live', live })
    }

    return list
})

function showMore(id: string, direction: 'before' | 'after') {
    const current = reveal.get(id) ?? { before: 0, after: 0 }

    reveal.set(id, { ...current, [direction]: current[direction] + MORE })
}

function groupSummary(group: ProgrammeGroup, status: string): string {
    const clips = group.last - group.first + 1
    const parts = [
        `${clockShort(group.start)}–${clockShort(group.end)}`,
        t('onair.clipCount', { total: clips.toLocaleString(locale.value) }, clips),
    ]

    if (status === 'past') {
        parts.push(t('onair.aired'))
    }

    return `· ${parts.join(' · ')}`
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
        in: 0,
        out: file.duration,
        duration: file.duration,
        ad: false,
    }
}

function insertAt(index: number, items: PlaylistItem[]) {
    playlistStore.playlist.splice(index, 0, ...items)
    items.forEach((item) => markAdded(item.uid))
    playlistStore.touch()
}

/** Add a file from the library, after the clip on air or at the end. */
function addFile(file: LibraryFile) {
    const index = onAirIndex.value >= 0 ? onAirIndex.value + 1 : playlistStore.playlist.length

    insertAt(index, [itemFromFile(file)])
    indexStore.msgAlert(
        'success',
        t('onair.added', { name: file.name, time: clockTime(playlistStore.playlist[index]?.begin ?? 0) }),
        2,
    )
}

function removeItem(index: number) {
    playlistStore.playlist.splice(index, 1)
    playlistStore.touch()
}

function toggleAd(item: PlaylistItem) {
    item.ad = !item.ad
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

    return types.includes(LIBRARY_DRAG_TYPE) || types.includes(ROW_DRAG_TYPE)
}

function rowDragStart(event: DragEvent, item: PlaylistItem) {
    dragRow.value = item.uid
    event.dataTransfer?.setData(ROW_DRAG_TYPE, item.uid)

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
    const uid = event.dataTransfer?.getData(ROW_DRAG_TYPE)

    event.preventDefault()
    dragEnd()

    if (index === null) {
        return
    }

    if (media) {
        insertAt(index, [itemFromFile(JSON.parse(media) as LibraryFile)])
        return
    }

    const from = playlistStore.playlist.findIndex((item) => item.uid === uid)

    if (from < 0 || from === index || from + 1 === index) {
        return
    }

    const [moved] = playlistStore.playlist.splice(from, 1)

    if (moved) {
        playlistStore.playlist.splice(from < index ? index - 1 : index, 0, moved)
        markAdded(moved.uid)
        playlistStore.touch()
    }
}

function dropClass(before: number, after: number) {
    if (dropAt.value === before) {
        return 'drop-before'
    }

    return dropAt.value === after ? 'drop-after' : ''
}

// ---------------------------------------------------------------------------
// Saving and the actions menu

async function save(date = playlistStore.listDate) {
    if (!playlistStore.playlist.length) {
        return
    }

    saving.value = true

    try {
        const { processPlaylist } = playlistOperations()
        const program = processPlaylist(date, cloneDeep(playlistStore.playlist), true).map((item) => {
            const copy = { ...item } as Partial<PlaylistItem>

            delete copy.uid
            delete copy.begin
            delete copy.overtime

            return copy
        })

        await authFetch(`/api/playlist/${channel.value?.id}`, {
            method: 'POST',
            headers: { ...configStore.contentType, ...authStore.authHeader },
            body: JSON.stringify({ channel: channel.value?.name, date, program }),
        })

        if (date === playlistStore.listDate) {
            playlistStore.markSaved()
        }

        indexStore.msgAlert('success', t('onair.saved', { date }), 2)
    } catch (e: any) {
        if (e?.status === 409) {
            if (date === playlistStore.listDate) {
                playlistStore.markSaved()
            }

            indexStore.msgAlert('success', t('onair.alreadySaved'), 2)
        } else {
            indexStore.msgAlert('error', e?.data ?? e, 4)
        }
    } finally {
        saving.value = false
    }
}

function discard() {
    load(playlistStore.listDate)
}

function openCopy() {
    menuOpen.value = false
    copyDate.value = addDays(playlistStore.listDate, 1)
    showCopy.value = true
}

async function copyTo(confirm: boolean) {
    showCopy.value = false

    if (confirm && copyDate.value) {
        await save(copyDate.value)
    }
}

async function importList(confirm: boolean) {
    showImport.value = false

    const file = importFile.value
    importFile.value = null

    if (!confirm || !file) {
        return
    }

    const formData = new FormData()
    formData.append(file.name, file)

    try {
        await authFetch(
            `/api/file/${channel.value?.id}/import?file=${encodeURIComponent(file.name)}&date=${playlistStore.listDate}`,
            { method: 'PUT', headers: authStore.authHeader, body: formData },
        )
        indexStore.msgAlert('success', t('onair.imported'), 2)
        await load(playlistStore.listDate)
    } catch (e: any) {
        indexStore.msgAlert('error', e?.data ?? e, 4)
    }
}

function fillDay() {
    menuOpen.value = false

    const original = playlistStore.playlist
    const target = configStore.playout.playlist.lengthInSec || 86400
    const filled: PlaylistItem[] = []
    let length = 0

    if (!original.length) {
        return
    }

    while (length < target) {
        for (const item of original) {
            if (length >= target) {
                break
            }

            filled.push({ ...cloneDeep(item), uid: genUID() })
            length += item.out - item.in
        }
    }

    playlistStore.playlist = filled
    playlistStore.touch()
}

async function deleteList(confirm: boolean) {
    showDelete.value = false

    if (!confirm) {
        return
    }

    try {
        await authFetch(`/api/playlist/${channel.value?.id}/${playlistStore.listDate}`, {
            method: 'DELETE',
            headers: { ...configStore.contentType, ...authStore.authHeader },
        })
        playlistStore.playlist = []
        playlistStore.markSaved()
        indexStore.msgAlert('warning', t('onair.deleted'), 2)
    } catch (e: any) {
        indexStore.msgAlert('error', e?.data ?? e, 4)
    }
}

onBeforeUnmount(() => {
    menuOpen.value = false
})

defineExpose({ addFile })
</script>
<template>
    <section :aria-label="t('onair.playlist')" class="card-box min-w-0">
        <div class="flex flex-wrap items-center gap-x-3.5 gap-y-2.5 px-4 py-3.5 border-b border-line">
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
            <div class="flex-auto" />
            <span v-if="unsaved" class="inline-flex items-center gap-1.5 text-accent-text text-[13px]">
                <span class="dot w-1.75 h-1.75 bg-accent" aria-hidden="true" />{{ t('onair.unsaved', unsaved) }}
            </span>
            <button class="btn-quiet min-h-9.5" :disabled="!playlistStore.dirty" @click="discard">
                {{ t('onair.discard') }}
            </button>
            <button class="btn-main min-h-9.5" :disabled="!playlistStore.dirty || saving" @click="save()">
                {{ t('onair.save') }}
            </button>
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
            <div class="flex gap-2">
                <button class="btn-main" @click="emit('generate')">{{ t('onair.generate') }}</button>
                <button class="btn-quiet" @click="showImport = true">{{ t('onair.importList') }}</button>
            </div>
        </div>

        <div v-else class="relative overflow-x-auto">
            <table class="w-full min-w-[760px] border-collapse text-[13px]" @dragend="dragEnd">
                <thead>
                    <tr class="text-muted text-left text-xs">
                        <th class="px-4 py-2.5 font-medium w-[100px]">{{ t('onair.time') }}</th>
                        <th class="px-2 py-2.5 font-medium">{{ t('onair.programFile') }}</th>
                        <th class="px-2 py-2.5 font-medium w-[72px]">{{ t('onair.length') }}</th>
                        <th class="px-2 py-2.5 font-medium w-[120px]">{{ t('onair.inOut') }}</th>
                        <th class="px-2 py-2.5 font-medium w-[48px]" :title="t('onair.commercial')">Ad</th>
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
                            class="border-t border-line"
                            :class="[
                                row.status === 'past' ? 'text-subtle' : '',
                                row.expanded && row.status !== 'past'
                                    ? 'bg-[color-mix(in_srgb,var(--color-base-100)_92%,var(--color-base-content))]'
                                    : '',
                                dropClass(row.group.first, row.group.last + 1),
                            ]"
                            @dragover="dragOverRow($event, row.group.first, row.group.last + 1)"
                        >
                            <td class="px-4 py-2.5 font-mono" :class="row.status === 'past' ? '' : 'text-muted'">
                                {{ clockTime(row.group.start) }}
                            </td>
                            <td class="px-2 py-2.5" colspan="5">
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
                            <td
                                class="ps-7.5 pe-4 py-1.5 font-mono"
                                :class="row.index === onAirIndex ? 'text-accent-text' : row.past ? '' : 'text-muted'"
                            >
                                {{ clockTime(row.item.begin) }}
                            </td>
                            <td class="px-2 py-1.5 min-w-0">
                                <span class="flex items-center gap-2 min-w-0">
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
                                    <span class="truncate" :title="row.item.source">
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
                            <td class="px-2 py-1.5 font-mono" :class="row.past ? '' : 'text-muted'">
                                {{ lengthLabel(row.item.in) }} · {{ lengthLabel(row.item.out) }}
                            </td>
                            <td class="px-2 py-1.5">
                                <input
                                    type="checkbox"
                                    class="checkbox checkbox-xs checkbox-accent rounded-[4px]"
                                    :checked="row.item.ad"
                                    :aria-label="t('onair.markCommercial')"
                                    @change="toggleAd(row.item)"
                                />
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
                            <td class="ps-7.5 pe-4 pt-3 pb-1 font-mono align-top">
                                {{ clockTime(playlistStore.playlist[row.index]?.begin ?? 0) }}
                            </td>
                            <td class="px-2 pe-4 pt-3 pb-4" colspan="5">
                                <ClipEditor
                                    :item="editing.item"
                                    :storage="storage"
                                    @apply="applyEdit"
                                    @cancel="editing = null"
                                    @pick="picker = $event"
                                />
                            </td>
                        </tr>

                        <tr v-else-if="row.type === 'more'" class="border-t border-raised">
                            <td class="ps-7.5 pe-4 py-2" colspan="6">
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
                            <td class="px-4 py-2.5 font-mono text-live">{{ clockTime(row.live.start) }}</td>
                            <td class="px-2 py-2.5" colspan="5">
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
                            {{ t('onair.dropAtEnd') }}
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
            :show="showCopy"
            :title="t('onair.copyToDay')"
            :ok-label="t('onair.copy')"
            :ok-disabled="!copyDate"
            :modal-action="copyTo"
        >
            <div class="flex flex-col gap-3 w-[min(380px,85vw)]">
                <label class="field-label">
                    {{ t('onair.targetDay') }}
                    <input v-model="copyDate" type="date" class="field-input" />
                </label>
                <p class="m-0 text-sm text-muted">{{ t('onair.copyHint') }}</p>
            </div>
        </GenericModal>

        <GenericModal
            :show="showImport"
            :title="t('onair.importList')"
            :ok-label="t('onair.import')"
            :ok-disabled="!importFile"
            :modal-action="importList"
        >
            <div class="flex flex-col gap-3 w-[min(420px,85vw)]">
                <input
                    type="file"
                    accept=".m3u,.m3u8,.txt,text/plain"
                    class="file-input file-input-sm w-full"
                    @change="importFile = ($event.target as HTMLInputElement).files?.[0] ?? null"
                />
                <p class="m-0 text-sm text-muted">{{ t('onair.importHint', { date: dateLabel }) }}</p>
            </div>
        </GenericModal>

        <GenericModal
            :show="showDelete"
            :title="t('onair.deleteList', { date: shortDateLabel })"
            :ok-label="t('onair.delete')"
            danger
            :modal-action="deleteList"
        >
            <p class="m-0 max-w-sm">{{ t('onair.deleteQuestion', { date: dateLabel }) }}</p>
        </GenericModal>

        <GenericModal
            :show="pendingDate !== null"
            :title="t('onair.unsavedTitle')"
            :ok-label="t('onair.discardAndContinue')"
            danger
            :modal-action="confirmDateChange"
        >
            <p class="m-0 max-w-sm">{{ t('onair.unsavedQuestion', unsaved) }}</p>
        </GenericModal>
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
