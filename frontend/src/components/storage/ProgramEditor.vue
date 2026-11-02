<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import StoragePicker from '@/components/schedule/StoragePicker.vue'
import { useDialogFocus } from '@/composables/dialogFocus'
import { formatLength } from '@/composables/schedule'
import { useIndex } from '@/stores/index'
import { useSchedule } from '@/stores/schedule'

/**
 * Create or change an assembled program: a title, its blocks in order and
 * breaks of commercials between them. The storage stays open beside the
 * program: drag a clip to where it goes, or press + to add it at the end (a
 * clip becomes the next block, a commercial goes into the break at the end).
 * Playlists and the Scheduler point to the file, so a change applies to every
 * airing still to come.
 */
const props = defineProps<{
    /** The program file to change; a new one is made in `folder`. */
    path: string | null
    /** Folder of a new program, relative to the storage. */
    folder: string
}>()

const emit = defineEmits<{
    close: []
    saved: [path: string]
}>()

const { t } = useI18n()
const indexStore = useIndex()
const scheduleStore = useSchedule()

interface Clip {
    source: string
    /** Seconds that air, as listed or read by the server on save. */
    length: number
}

type Part = { kind: 'block'; clip: Clip } | { kind: 'break'; clips: Clip[] }

/** Where a dragged clip or part lands. */
interface DropTarget {
    index: number
    where: 'before' | 'after' | 'inside'
}

const STORAGE_TYPE = 'application/x-noar-schedule'
const COMMERCIAL_TYPE = 'application/x-noar-commercial'
const PART_TYPE = 'application/x-noar-program-part'

const fileName = ref('')
const title = ref('')
const parts = ref<Part[]>([])
const loading = ref(Boolean(props.path))
const saving = ref(false)
const error = ref('')
const dropTarget = ref<DropTarget | null>(null)
/** The end of the list is the target. */
const dropAtEnd = ref(false)
const dialog = ref<HTMLElement>()

useDialogFocus(dialog, { onEscape: () => emit('close') })

const total = computed(() => parts.value.reduce((sum, part) => sum + partLength(part), 0))
const blockCount = computed(() => parts.value.filter((part) => part.kind === 'block').length)

function partLength(part: Part): number {
    return part.kind === 'block' ? part.clip.length : part.clips.reduce((sum, clip) => sum + clip.length, 0)
}

/** Number of block `index` among the blocks. */
function blockNumber(index: number): number {
    return parts.value.slice(0, index + 1).filter((part) => part.kind === 'block').length
}

function clipName(source: string): string {
    return source.split('/').pop() ?? source
}

function fromProgram(program: AssembledProgram) {
    title.value = program.title
    parts.value = program.segments.map((segment) =>
        segment.kind === 'block'
            ? { kind: 'block', clip: { source: segment.source, length: segment.out - segment.in } }
            : {
                  kind: 'break',
                  clips: segment.clips.map((clip) => ({ source: clip.source, length: clip.out - clip.in })),
              },
    )
}

onMounted(async () => {
    if (!props.path) {
        return
    }

    fileName.value = clipName(props.path).replace(/\.program$/i, '')

    try {
        fromProgram(await scheduleStore.getProgram(props.path))
    } catch (err) {
        error.value = err instanceof Error ? err.message : String(err)
    } finally {
        loading.value = false
    }
})

/**
 * Put a clip at `position` in the list: a commercial joins a break next to
 * that place, or starts one there; any other clip becomes a block there.
 */
function insertClip(clip: Clip, commercial: boolean, position: number) {
    if (!commercial) {
        parts.value.splice(position, 0, { kind: 'block', clip })
        return
    }

    const before = parts.value[position - 1]
    const after = parts.value[position]

    if (before?.kind === 'break') {
        before.clips.push(clip)
    } else if (after?.kind === 'break') {
        after.clips.unshift(clip)
    } else {
        parts.value.splice(position, 0, { kind: 'break', clips: [clip] })
    }
}

/** The + of the storage: at the end of the program. */
function add(item: { path: string; duration: number; commercial: boolean }) {
    insertClip({ source: item.path, length: item.duration }, item.commercial, parts.value.length)
}

/** A break right after block `index`, to drop commercials into. */
function addBreak(index: number) {
    parts.value.splice(index + 1, 0, { kind: 'break', clips: [] })
}

function move(index: number, step: number) {
    const to = index + step

    if (to < 0 || to >= parts.value.length) {
        return
    }

    const [part] = parts.value.splice(index, 1)
    parts.value.splice(to, 0, part!)
}

function remove(index: number) {
    parts.value.splice(index, 1)
}

function removeCommercial(part: number, clip: number) {
    const target = parts.value[part]

    if (target?.kind === 'break') {
        target.clips.splice(clip, 1)
    }
}

// ---------------------------------------------------------------------------
// Dragging: clips from the storage, and the parts themselves to reorder.

function accepts(event: DragEvent): boolean {
    const types = event.dataTransfer?.types ?? []

    return types.includes(STORAGE_TYPE) || types.includes(PART_TYPE)
}

function partDragStart(event: DragEvent, index: number) {
    event.dataTransfer?.setData(PART_TYPE, String(index))

    if (event.dataTransfer) {
        event.dataTransfer.effectAllowed = 'move'
    }
}

/** Upper part of a row: before it; lower part: after it; the middle of a break: into it. */
function overPart(event: DragEvent, index: number) {
    if (!accepts(event)) {
        return
    }

    event.preventDefault()
    dropAtEnd.value = false

    const row = event.currentTarget as HTMLElement
    const box = row.getBoundingClientRect()
    const ratio = (event.clientY - box.top) / Math.max(box.height, 1)
    const commercial = event.dataTransfer?.types.includes(COMMERCIAL_TYPE) ?? false
    const intoBreak = parts.value[index]?.kind === 'break' && commercial && ratio > 0.25 && ratio < 0.75

    dropTarget.value = { index, where: intoBreak ? 'inside' : ratio < 0.5 ? 'before' : 'after' }
}

function overEnd(event: DragEvent) {
    if (!accepts(event)) {
        return
    }

    event.preventDefault()
    dropTarget.value = null
    dropAtEnd.value = true
}

function leaveList(event: DragEvent) {
    const list = event.currentTarget as HTMLElement

    if (!list.contains(event.relatedTarget as Node | null)) {
        dropTarget.value = null
        dropAtEnd.value = false
    }
}

function drop(event: DragEvent) {
    event.preventDefault()

    const target = dropTarget.value
    const position =
        dropAtEnd.value || !target ? parts.value.length : target.where === 'before' ? target.index : target.index + 1

    dropTarget.value = null
    dropAtEnd.value = false

    const moved = event.dataTransfer?.getData(PART_TYPE)

    if (moved) {
        const from = Number(moved)
        const [part] = parts.value.splice(from, 1)

        if (part) {
            parts.value.splice(from < position ? position - 1 : position, 0, part)
        }
        return
    }

    const raw = event.dataTransfer?.getData(STORAGE_TYPE)

    if (!raw) {
        return
    }

    const item = JSON.parse(raw) as ScheduleDragItem

    if (item.type !== 'media') {
        return
    }

    const clip = { source: item.path, length: item.duration }
    const commercial = Boolean(item.commercial)
    const into = target?.where === 'inside' ? parts.value[target.index] : undefined

    if (into?.kind === 'break') {
        into.clips.push(clip)
    } else {
        insertClip(clip, commercial, position)
    }
}

function rowClass(index: number): string {
    const target = dropTarget.value

    if (target?.index !== index) {
        return ''
    }

    return target.where === 'inside'
        ? 'outline-2 outline-accent'
        : target.where === 'before'
          ? 'shadow-[0_-3px_0_0_var(--color-accent)]'
          : 'shadow-[0_3px_0_0_var(--color-accent)]'
}

// ---------------------------------------------------------------------------
// Saving

function check(): string {
    if (!title.value.trim()) {
        return t('storage.program.needTitle')
    }
    if (!props.path && !safeName(fileName.value || title.value)) {
        return t('storage.program.needName')
    }
    if (!blockCount.value) {
        return t('storage.program.needBlock')
    }
    if (parts.value.some((part) => part.kind === 'break' && !part.clips.length)) {
        return t('storage.program.emptyBreak')
    }

    return ''
}

function safeName(name: string): string {
    return name.trim().replace(/[\\/:*?"<>|]/g, '')
}

async function save() {
    error.value = check()

    if (error.value) {
        return
    }

    const path =
        props.path ?? [props.folder, `${safeName(fileName.value || title.value)}.program`].filter(Boolean).join('/')

    saving.value = true

    try {
        const program = await scheduleStore.saveProgram(
            path,
            title.value.trim(),
            parts.value.map((part) =>
                part.kind === 'block'
                    ? { kind: 'block', source: part.clip.source }
                    : { kind: 'break', clips: part.clips.map((clip) => clip.source) },
            ),
        )

        fromProgram(program)
        indexStore.msgAlert('success', t('storage.program.saved'), 2)
        emit('saved', path)
    } catch (err) {
        error.value = err instanceof Error ? err.message : String(err)
    } finally {
        saving.value = false
    }
}
</script>

<template>
    <div class="z-50 fixed inset-0 bg-black/50 overflow-auto flex justify-center items-start py-6 px-3">
        <div
            ref="dialog"
            role="dialog"
            aria-modal="true"
            aria-labelledby="program-editor-title"
            tabindex="-1"
            class="outline-none w-full max-w-300 bg-base-100 border border-base-content/20 rounded-lg shadow-xl flex flex-col"
        >
            <div class="flex items-center justify-between px-5 py-3 border-b border-base-300">
                <h2 id="program-editor-title" class="text-lg font-semibold flex items-center gap-2 m-0">
                    <i class="bi bi-collection-play" aria-hidden="true" />
                    {{ path ? t('storage.program.editTitle') : t('storage.program.newTitle') }}
                </h2>
                <button type="button" class="btn-icon" :aria-label="t('schedule.close')" @click="emit('close')">
                    <i class="bi bi-x-lg" aria-hidden="true" />
                </button>
            </div>

            <div v-if="loading" class="px-5 py-8 flex justify-center">
                <span class="loading loading-spinner" />
            </div>

            <div v-else class="grid grid-cols-1 lg:grid-cols-[minmax(0,22rem)_minmax(0,1fr)] gap-4 px-5 py-4">
                <!-- The storage, always open: drag a clip or press + -->
                <aside
                    class="border border-line-strong rounded-lg p-3 flex flex-col gap-2 min-h-0 lg:max-h-[70vh]"
                    :aria-label="t('storage.program.storage')"
                >
                    <span class="font-semibold text-sm">{{ t('storage.program.storage') }}</span>
                    <p class="m-0 text-xs text-muted">{{ t('storage.program.storageHint') }}</p>
                    <StoragePicker hide-lives draggable addable class="min-h-0 overflow-hidden" @pick="add" />
                </aside>

                <div class="flex flex-col gap-4 min-w-0">
                    <div class="grid grid-cols-1 sm:grid-cols-2 gap-3.5">
                        <label class="field-label">
                            {{ t('storage.program.title') }}
                            <input v-model="title" class="field-input" maxlength="200" />
                        </label>
                        <label class="field-label">
                            {{ t('storage.program.fileName') }}
                            <input
                                v-model="fileName"
                                class="field-input"
                                :disabled="Boolean(path)"
                                :placeholder="title"
                                spellcheck="false"
                            />
                        </label>
                    </div>

                    <ol
                        class="list-none m-0 p-0 flex flex-col gap-2 min-h-24"
                        :aria-label="t('storage.program.parts')"
                        @dragleave="leaveList"
                        @drop="drop"
                    >
                        <li
                            v-for="(part, index) in parts"
                            :key="index"
                            class="border rounded-lg px-3 py-2 flex flex-col gap-1.5 bg-base-100"
                            :class="[
                                part.kind === 'break' ? 'border-warning/60 bg-warning/5' : 'border-line-strong',
                                rowClass(index),
                            ]"
                            @dragover="overPart($event, index)"
                        >
                            <div class="flex items-center gap-2">
                                <span
                                    class="cursor-grab text-subtle"
                                    draggable="true"
                                    :title="t('storage.program.dragToMove')"
                                    @dragstart="partDragStart($event, index)"
                                >
                                    <i class="bi bi-grip-vertical" aria-hidden="true" />
                                </span>
                                <i
                                    class="bi"
                                    :class="part.kind === 'block' ? 'bi-film' : 'bi-megaphone'"
                                    aria-hidden="true"
                                />
                                <span class="font-semibold whitespace-nowrap">
                                    {{
                                        part.kind === 'block'
                                            ? t('storage.program.block', { n: blockNumber(index) })
                                            : t('storage.program.break')
                                    }}
                                </span>
                                <span v-if="part.kind === 'block'" class="truncate text-[13px] font-mono text-muted">
                                    {{ clipName(part.clip.source) }}
                                </span>
                                <span class="ms-auto text-[13px] font-mono">{{ formatLength(partLength(part)) }}</span>
                                <button
                                    type="button"
                                    class="btn-icon"
                                    :disabled="index === 0"
                                    :aria-label="t('storage.program.up')"
                                    @click="move(index, -1)"
                                >
                                    <i class="bi bi-arrow-up" aria-hidden="true" />
                                </button>
                                <button
                                    type="button"
                                    class="btn-icon"
                                    :disabled="index === parts.length - 1"
                                    :aria-label="t('storage.program.down')"
                                    @click="move(index, 1)"
                                >
                                    <i class="bi bi-arrow-down" aria-hidden="true" />
                                </button>
                                <button
                                    type="button"
                                    class="btn-icon"
                                    :aria-label="t('storage.program.remove')"
                                    @click="remove(index)"
                                >
                                    <i class="bi bi-trash" aria-hidden="true" />
                                </button>
                            </div>

                            <template v-if="part.kind === 'break'">
                                <ul v-if="part.clips.length" class="list-none m-0 p-0 flex flex-col gap-1 ps-7">
                                    <li
                                        v-for="(clip, clipIndex) in part.clips"
                                        :key="clipIndex"
                                        class="flex items-center gap-2 text-[13px]"
                                    >
                                        <span class="truncate font-mono">{{ clipName(clip.source) }}</span>
                                        <span class="ms-auto font-mono text-muted">{{
                                            formatLength(clip.length)
                                        }}</span>
                                        <button
                                            type="button"
                                            class="btn-icon"
                                            :aria-label="t('storage.program.remove')"
                                            @click="removeCommercial(index, clipIndex)"
                                        >
                                            <i class="bi bi-x" aria-hidden="true" />
                                        </button>
                                    </li>
                                </ul>
                                <p v-else class="m-0 ps-7 text-[13px] text-muted">
                                    {{ t('storage.program.emptyBreakHint') }}
                                </p>
                            </template>
                            <button
                                v-else-if="index < parts.length - 1 && parts[index + 1]?.kind !== 'break'"
                                type="button"
                                class="link-accent self-start min-h-7 ps-7 text-[13px]"
                                @click="addBreak(index)"
                            >
                                + {{ t('storage.program.addBreak') }}
                            </button>
                        </li>

                        <li
                            class="border-2 border-dashed rounded-lg px-3 py-5 text-center text-sm text-muted"
                            :class="dropAtEnd ? 'border-accent text-base-content' : 'border-line-strong'"
                            @dragover="overEnd"
                        >
                            <i class="bi bi-box-arrow-in-down me-1" aria-hidden="true" />
                            {{ parts.length ? t('storage.program.dropHere') : t('storage.program.dropFirst') }}
                        </li>
                    </ol>

                    <div class="flex items-center gap-3">
                        <span class="text-sm text-muted">
                            {{ t('storage.program.counts', { blocks: blockCount, breaks: parts.length - blockCount }) }}
                        </span>
                        <span class="ms-auto text-sm">
                            {{ t('storage.program.total') }}
                            <strong class="font-mono">{{ formatLength(total) }}</strong>
                        </span>
                    </div>
                    <p class="m-0 field-hint">{{ t('storage.program.hint') }}</p>
                    <p v-if="error" class="m-0 text-error text-sm" role="alert">{{ error }}</p>
                </div>
            </div>

            <div class="flex flex-wrap justify-end gap-2 px-5 py-3 border-t border-base-300">
                <button type="button" class="btn-quiet" @click="emit('close')">{{ t('cancel') }}</button>
                <button type="button" class="btn-main" :disabled="saving || loading" @click="save">
                    <span v-if="saving" class="loading loading-spinner loading-sm" />
                    {{ t('storage.program.save') }}
                </button>
            </div>
        </div>
    </div>
</template>
