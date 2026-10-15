<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import FolderTree from '@/components/storage/FolderTree.vue'
import { authFetch } from '@/composables/authFetch'
import { playlistOperations } from '@/composables/helper'
import { clockShort, totalLength } from '@/composables/player'
import { baseName, listFolders, storageJoin } from '@/composables/storage'
import { useConfig } from '@/stores/config'
import { useIndex } from '@/stores/index'
import { usePlaylist } from '@/stores/playlist'

/**
 * Builds a day from storage folders, or from blocks of folders at set times.
 * The result goes into the playlist as unsaved changes to review and save.
 */
const props = defineProps<{ close: () => void }>()

const { t } = useI18n()
const configStore = useConfig()
const indexStore = useIndex()
const playlistStore = usePlaylist()
const { processPlaylist } = playlistOperations()

interface Block {
    id: number
    /** `HH:MM:SS` */
    start: string
    /** `HH:MM:SS` */
    duration: string
    shuffle: boolean
    paths: string[]
}

const mode = ref<'folders' | 'blocks'>('folders')
const channelId = computed(() => configStore.channels[configStore.i]?.id ?? 0)
const dayStart = computed(() => configStore.playout.playlist?.startInSec ?? 0)
const dayLength = computed(() => configStore.playout.playlist?.lengthInSec || 86400)

// Folders mode
const browsePath = ref('')
const folders = ref<string[]>([])
const sheetFolders = ref<string[]>([])
const loadingFolders = ref(false)
const chosen = ref<string[]>([])
const wholeStorage = ref(false)
const shuffle = ref(true)
const fill = ref<'replace' | 'complete'>('replace')

// Blocks mode
let nextId = 1
const blocks = ref<Block[]>([newBlock(dayStart.value)])
const pickerFor = ref<number | null>(null)

const generating = ref(false)

const currentLength = computed(() => playlistStore.playlist.reduce((sum, item) => sum + (item.out - item.in), 0))
const missing = computed(() => Math.max(0, dayLength.value - currentLength.value))
const crumbs = computed(() => {
    const parts = browsePath.value ? browsePath.value.split('/') : []

    return [
        { name: t('storage.root'), path: '' },
        ...parts.map((name, index) => ({ name, path: parts.slice(0, index + 1).join('/') })),
    ]
})

function toClock(seconds: number): string {
    const value = ((Math.round(seconds) % 86400) + 86400) % 86400
    const pad = (number: number) => String(number).padStart(2, '0')

    return `${pad(Math.floor(value / 3600))}:${pad(Math.floor((value % 3600) / 60))}:${pad(value % 60)}`
}

function toSeconds(clock: string): number {
    const [hours = 0, minutes = 0, seconds = 0] = clock.split(':').map(Number)

    return (hours || 0) * 3600 + (minutes || 0) * 60 + (seconds || 0)
}

function newBlock(start: number, duration = 7200): Block {
    return { id: nextId++, start: toClock(start), duration: toClock(duration), shuffle: false, paths: [] }
}

/** Seconds after the day start, so blocks after midnight sort after the evening. */
function offsetOf(block: Block): number {
    return (toSeconds(block.start) - dayStart.value + 86400) % 86400
}

const coverage = computed(() => {
    const spans = blocks.value
        .map((block) => ({ block, from: offsetOf(block), to: offsetOf(block) + toSeconds(block.duration) }))
        .sort((a, b) => a.from - b.from)
    const gaps: { from: number; to: number }[] = []
    const overlaps: string[] = []
    let reached = 0

    for (const span of spans) {
        if (span.from > reached) {
            gaps.push({ from: reached, to: span.from })
        } else if (span.from < reached) {
            overlaps.push(span.block.start.slice(0, 5))
        }

        reached = Math.max(reached, span.to)
    }

    if (reached < dayLength.value) {
        gaps.push({ from: reached, to: dayLength.value })
    }

    const percent = (seconds: number) => Math.min(100, (seconds / dayLength.value) * 100)

    return {
        spans: spans.map((span) => ({
            id: span.block.id,
            left: percent(span.from),
            width: Math.max(0.5, percent(span.to) - percent(span.from)),
            empty: !span.block.paths.length,
        })),
        gaps: gaps.map((gap) => ({
            ...gap,
            left: percent(gap.from),
            width: percent(gap.to) - percent(gap.from),
            label: `${clockShort(dayStart.value + gap.from)}–${clockShort(dayStart.value + gap.to)}`,
        })),
        overlaps,
        open: gaps.reduce((sum, gap) => sum + gap.to - gap.from, 0),
        end: reached,
    }
})

const ticks = computed(() =>
    Array.from({ length: 5 }, (_, index) => clockShort(dayStart.value + (dayLength.value / 4) * index)),
)

const canGenerate = computed(() => {
    if (generating.value) {
        return false
    }

    if (mode.value === 'folders') {
        return wholeStorage.value || chosen.value.length > 0
    }

    return blocks.value.length > 0 && blocks.value.every((block) => block.paths.length && toSeconds(block.duration) > 0)
})

async function browse(path: string) {
    loadingFolders.value = true

    try {
        const listing = await listFolders(channelId.value, path)

        browsePath.value = path
        folders.value = listing.folders
        sheetFolders.value = listing.sheetFolders
    } catch (e) {
        indexStore.msgAlert('error', e instanceof Error ? e.message : String(e), 4)
    } finally {
        loadingFolders.value = false
    }
}

function toggleFolder(path: string) {
    chosen.value = chosen.value.includes(path) ? chosen.value.filter((item) => item !== path) : [...chosen.value, path]
}

function addBlock() {
    const end = coverage.value.end

    blocks.value.push(newBlock(dayStart.value + end, Math.min(7200, Math.max(1800, dayLength.value - end))))
}

function removeBlock(id: number) {
    blocks.value = blocks.value.filter((block) => block.id !== id)
}

function addPath(block: Block, path: string) {
    if (path && !block.paths.includes(path)) {
        block.paths.push(path)
    }

    pickerFor.value = null
}

async function generate() {
    generating.value = true

    const body: Record<string, unknown> = { draft: true }

    if (mode.value === 'folders') {
        body.shuffle = shuffle.value

        if (!wholeStorage.value) {
            body.paths = chosen.value
        }
    } else {
        body.template = {
            sources: [...blocks.value]
                .sort((a, b) => offsetOf(a) - offsetOf(b))
                .map((block) => ({
                    start: block.start,
                    duration: block.duration,
                    shuffle: block.shuffle,
                    paths: block.paths,
                })),
        }
    }

    try {
        const response = await authFetch<{ program: PlaylistItem[] }>(
            `/api/playlist/${channelId.value}/generate/${playlistStore.listDate}`,
            {
                method: 'POST',
                headers: configStore.contentType,
                body: JSON.stringify(body),
            },
        )
        const generated = processPlaylist(playlistStore.listDate, response.program, false)

        if (mode.value === 'folders' && fill.value === 'complete') {
            const added: PlaylistItem[] = []
            let length = 0

            for (const item of generated) {
                if (length >= missing.value) {
                    break
                }

                added.push(item)
                length += item.out - item.in
            }

            playlistStore.playlist = [...playlistStore.playlist, ...added]
            playlistStore.addedUids = [...playlistStore.addedUids, ...added.map((item) => item.uid)]
        } else {
            playlistStore.playlist = generated
        }

        playlistStore.touch()
        playlistStore.scrollToItem = true
        indexStore.msgAlert('success', t('generator.done'), 3)
        props.close()
    } catch (e: any) {
        indexStore.msgAlert('error', e?.data ?? e?.message ?? String(e), 6)
    } finally {
        generating.value = false
    }
}

function onKey(event: KeyboardEvent) {
    if (event.key === 'Escape') {
        if (pickerFor.value !== null) {
            pickerFor.value = null
        } else {
            props.close()
        }
    }
}

watch(missing, (value) => {
    if (value <= 0 && fill.value === 'complete') {
        fill.value = 'replace'
    }
})

onMounted(() => {
    document.body.style.overflow = 'hidden'
    window.addEventListener('keydown', onKey)
    browse('')
})

onBeforeUnmount(() => {
    document.body.style.overflow = ''
    window.removeEventListener('keydown', onKey)
})
</script>
<template>
    <div class="z-50 fixed inset-0 flex justify-center bg-black/55 overflow-auto py-6 px-4" @click.self="close()">
        <section
            role="dialog"
            aria-modal="true"
            aria-labelledby="generator-title"
            class="my-auto w-full max-w-[720px] bg-base-100 border border-line-strong rounded-[16px] shadow-2xl flex flex-col"
        >
            <header class="flex items-start gap-3 px-5 pt-5 pb-3">
                <div class="flex-1 min-w-0">
                    <h2 id="generator-title" class="m-0 text-lg font-semibold">{{ t('generator.title') }}</h2>
                    <p class="m-0 text-[13px] text-muted">
                        {{ t('generator.day', { date: playlistStore.listDate, start: clockShort(dayStart) }) }}
                    </p>
                </div>
                <button type="button" class="btn-icon" :aria-label="t('storage.close')" @click="close()">
                    <i class="bi bi-x-lg" aria-hidden="true" />
                </button>
            </header>

            <div class="px-5">
                <div
                    role="tablist"
                    :aria-label="t('generator.how')"
                    class="grid grid-cols-2 bg-base-200 border border-line-strong rounded-[10px] p-[3px]"
                >
                    <button
                        v-for="option in ['folders', 'blocks'] as const"
                        :key="option"
                        type="button"
                        role="tab"
                        :aria-selected="mode === option"
                        class="border-0 rounded-lg min-h-9.5 cursor-pointer"
                        :class="mode === option ? 'bg-base-300 text-base-content' : 'bg-transparent text-muted'"
                        @click="mode = option"
                    >
                        {{ option === 'folders' ? t('generator.folders') : t('generator.blocks') }}
                    </button>
                </div>
            </div>

            <div v-if="mode === 'folders'" class="px-5 py-4 flex flex-col gap-3.5">
                <div class="flex flex-wrap items-center gap-2 text-[13px] text-muted">
                    <nav :aria-label="t('storage.path')" class="flex flex-wrap items-center gap-1.5 flex-1 min-w-0">
                        <template v-for="(crumb, index) in crumbs" :key="crumb.path">
                            <i v-if="index > 0" class="bi bi-chevron-right text-[10px]" aria-hidden="true" />
                            <button
                                type="button"
                                class="bg-transparent border-0 p-0 cursor-pointer"
                                :class="
                                    index === crumbs.length - 1
                                        ? 'text-base-content'
                                        : 'text-muted hover:text-base-content'
                                "
                                :disabled="wholeStorage"
                                @click="browse(crumb.path)"
                            >
                                {{ crumb.name }}
                            </button>
                        </template>
                    </nav>
                    <span>{{ t('generator.chosen', { count: chosen.length }, chosen.length) }}</span>
                </div>

                <ul
                    class="relative m-0 p-1 list-none border border-line-strong rounded-[10px] bg-base-200 max-h-[300px] min-h-[120px] overflow-y-auto"
                    :class="wholeStorage ? 'opacity-50 pointer-events-none' : ''"
                >
                    <li v-if="loadingFolders" class="absolute inset-0 flex items-center justify-center bg-base-200/70">
                        <span class="loading loading-spinner" />
                    </li>
                    <li
                        v-for="folder in folders"
                        :key="folder"
                        class="flex items-center gap-2.5 px-2 min-h-11 rounded-lg"
                        :class="chosen.includes(storageJoin(browsePath, folder)) ? 'bg-accent-soft' : ''"
                    >
                        <input
                            type="checkbox"
                            class="checkbox checkbox-sm checkbox-accent rounded-[4px]"
                            :checked="chosen.includes(storageJoin(browsePath, folder))"
                            :aria-label="folder"
                            @change="toggleFolder(storageJoin(browsePath, folder))"
                        />
                        <span class="flex-1 min-w-0 truncate">{{ folder }}</span>
                        <span
                            v-if="sheetFolders.includes(folder)"
                            class="text-[11px] text-accent-text border border-accent/40 rounded-md px-1.5 leading-4"
                        >
                            {{ t('storage.sheetBadge') }}
                        </span>
                        <button
                            type="button"
                            class="btn-icon min-w-8 min-h-8 text-muted"
                            :aria-label="t('storage.open', { name: folder })"
                            @click="browse(storageJoin(browsePath, folder))"
                        >
                            <i class="bi bi-chevron-right text-xs" aria-hidden="true" />
                        </button>
                    </li>
                    <li v-if="!loadingFolders && !folders.length" class="px-3 py-4 text-muted text-[13px]">
                        {{ t('generator.noFolders') }}
                    </li>
                </ul>

                <div v-if="chosen.length && !wholeStorage" class="flex flex-wrap gap-1.5">
                    <span
                        v-for="path in chosen"
                        :key="path"
                        class="inline-flex items-center gap-1 bg-base-300 rounded-md ps-2 text-[13px]"
                        :title="path"
                    >
                        {{ baseName(path) }}
                        <button
                            type="button"
                            class="btn-icon min-w-6 min-h-6 text-muted"
                            :aria-label="t('generator.removeFolder', { name: baseName(path) })"
                            @click="toggleFolder(path)"
                        >
                            <i class="bi bi-x" aria-hidden="true" />
                        </button>
                    </span>
                </div>

                <label class="flex items-center gap-2.5 text-[13px] cursor-pointer">
                    <input
                        v-model="wholeStorage"
                        type="checkbox"
                        class="checkbox checkbox-sm checkbox-accent rounded-[4px]"
                    />
                    {{ t('generator.wholeStorage') }}
                </label>

                <div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
                    <fieldset
                        class="border border-line-strong rounded-[10px] px-3 pt-1.5 pb-2.5 m-0 flex flex-col gap-1"
                    >
                        <legend class="text-xs text-muted px-1">{{ t('generator.order') }}</legend>
                        <label class="flex items-center gap-2.5 min-h-8.5 cursor-pointer">
                            <input v-model="shuffle" type="radio" :value="false" class="radio radio-xs radio-accent" />
                            {{ t('generator.inOrder') }}
                        </label>
                        <label class="flex items-center gap-2.5 min-h-8.5 cursor-pointer">
                            <input v-model="shuffle" type="radio" :value="true" class="radio radio-xs radio-accent" />
                            {{ t('generator.shuffled') }}
                        </label>
                    </fieldset>
                    <fieldset
                        class="border border-line-strong rounded-[10px] px-3 pt-1.5 pb-2.5 m-0 flex flex-col gap-1"
                    >
                        <legend class="text-xs text-muted px-1">{{ t('generator.fill') }}</legend>
                        <label class="flex items-center gap-2.5 min-h-8.5 cursor-pointer">
                            <input v-model="fill" type="radio" value="replace" class="radio radio-xs radio-accent" />
                            {{ t('generator.replace') }}
                        </label>
                        <label
                            class="flex items-center gap-2.5 min-h-8.5"
                            :class="missing > 0 && playlistStore.playlist.length ? 'cursor-pointer' : 'opacity-50'"
                        >
                            <input
                                v-model="fill"
                                type="radio"
                                value="complete"
                                class="radio radio-xs radio-accent"
                                :disabled="missing <= 0 || !playlistStore.playlist.length"
                            />
                            {{ t('generator.complete', { length: totalLength(missing) }) }}
                        </label>
                    </fieldset>
                </div>
            </div>

            <div v-else class="px-5 py-4 flex flex-col gap-3">
                <div class="flex flex-col gap-1" :aria-label="t('generator.coverage')" role="group">
                    <div class="relative h-5.5 rounded-md overflow-hidden bg-base-200">
                        <span
                            v-for="gap in coverage.gaps"
                            :key="`gap-${gap.from}`"
                            class="absolute top-0 bottom-0 bg-on-air-soft"
                            :style="{ left: `${gap.left}%`, width: `${gap.width}%` }"
                        />
                        <span
                            v-for="(span, index) in coverage.spans"
                            :key="span.id"
                            class="absolute top-0 bottom-0 border-e-2 border-base-100"
                            :class="span.empty ? 'bg-base-300' : index % 2 ? 'bg-accent-soft' : 'bg-accent/60'"
                            :style="{ left: `${span.left}%`, width: `${span.width}%` }"
                        />
                    </div>
                    <div class="flex justify-between font-mono text-[11px] text-subtle" aria-hidden="true">
                        <span v-for="(tick, index) in ticks" :key="index">{{ tick }}</span>
                    </div>
                </div>

                <ol class="m-0 p-0 list-none flex flex-col gap-2">
                    <li
                        v-for="block in blocks"
                        :key="block.id"
                        class="relative border border-line-strong rounded-[10px] p-3 grid gap-2.5 grid-cols-2 sm:grid-cols-[124px_100px_minmax(0,1fr)_auto_auto] items-end"
                    >
                        <label class="field-label">
                            {{ t('generator.start') }}
                            <input v-model="block.start" type="time" step="1" class="field-input font-mono px-2" />
                        </label>
                        <label class="field-label">
                            {{ t('generator.length') }}
                            <input
                                v-model="block.duration"
                                pattern="[0-9]{2}:[0-9]{2}(:[0-9]{2})?"
                                placeholder="02:00:00"
                                class="field-input font-mono px-2"
                            />
                        </label>
                        <div class="field-label col-span-2 sm:col-span-1">
                            {{ t('generator.blockFolders') }}
                            <div class="flex flex-wrap gap-1.5 items-center min-h-10">
                                <span
                                    v-for="path in block.paths"
                                    :key="path"
                                    class="inline-flex items-center gap-1 bg-base-300 text-base-content rounded-md ps-2 text-[13px]"
                                    :title="path"
                                >
                                    {{ baseName(path) }}
                                    <button
                                        type="button"
                                        class="btn-icon min-w-6 min-h-6 text-muted"
                                        :aria-label="t('generator.removeFolder', { name: baseName(path) })"
                                        @click="block.paths = block.paths.filter((item) => item !== path)"
                                    >
                                        <i class="bi bi-x" aria-hidden="true" />
                                    </button>
                                </span>
                                <button
                                    type="button"
                                    class="border border-dashed border-accent/50 text-accent-text rounded-md px-2 min-h-8 text-[13px] bg-transparent cursor-pointer"
                                    :aria-expanded="pickerFor === block.id"
                                    @click="pickerFor = pickerFor === block.id ? null : block.id"
                                >
                                    + {{ t('generator.addFolder') }}
                                </button>
                            </div>
                        </div>
                        <label class="flex items-center gap-2 min-h-10 text-[13px] cursor-pointer">
                            <input
                                v-model="block.shuffle"
                                type="checkbox"
                                class="checkbox checkbox-sm checkbox-accent rounded-[4px]"
                            />
                            {{ t('generator.shuffledShort') }}
                        </label>
                        <button
                            type="button"
                            class="btn-icon text-error justify-self-end"
                            :aria-label="t('generator.removeBlock', { start: block.start.slice(0, 5) })"
                            @click="removeBlock(block.id)"
                        >
                            <i class="bi bi-trash" aria-hidden="true" />
                        </button>

                        <div
                            v-if="pickerFor === block.id"
                            class="col-span-full border border-line rounded-lg bg-base-200 p-1.5 max-h-60 overflow-y-auto"
                        >
                            <p class="m-0 px-2 pb-1 text-xs text-muted">{{ t('generator.pickHint') }}</p>
                            <ul class="m-0 p-0">
                                <FolderTree
                                    :channel-id="channelId"
                                    path=""
                                    :label="t('storage.root')"
                                    current=""
                                    @open="(path) => addPath(block, path)"
                                />
                            </ul>
                        </div>
                    </li>
                </ol>

                <div class="flex flex-wrap items-center gap-2.5">
                    <button type="button" class="btn-quiet" :disabled="coverage.end >= dayLength" @click="addBlock">
                        + {{ t('generator.addBlock', { time: clockShort(dayStart + coverage.end) }) }}
                    </button>
                    <span
                        v-for="gap in coverage.gaps"
                        :key="`warn-${gap.from}`"
                        class="inline-flex items-center gap-1.5 text-[13px] text-on-air-text"
                    >
                        <i class="bi bi-exclamation-circle" aria-hidden="true" />{{
                            t('generator.gap', { range: gap.label })
                        }}
                    </span>
                    <span
                        v-if="coverage.overlaps.length"
                        class="inline-flex items-center gap-1.5 text-[13px] text-accent-text"
                    >
                        <i class="bi bi-exclamation-triangle" aria-hidden="true" />{{
                            t('generator.overlap', { times: coverage.overlaps.join(', ') })
                        }}
                    </span>
                </div>
            </div>

            <p
                class="mx-5 mb-0 flex gap-2 items-start text-[13px] text-base-content/85 bg-raised rounded-[10px] px-3 py-2.5"
            >
                <i class="bi bi-info-circle mt-0.5" aria-hidden="true" />{{ t('generator.draftHint') }}
            </p>

            <footer class="flex flex-wrap items-center gap-2.5 px-5 py-4 mt-4 border-t border-line">
                <span class="text-[13px] text-muted">
                    <template v-if="mode === 'blocks'">
                        {{
                            coverage.open
                                ? t('generator.missing', { length: totalLength(coverage.open) })
                                : t('generator.covered')
                        }}
                    </template>
                    <template v-else-if="fill === 'complete'">
                        {{ t('generator.completeSummary', { length: totalLength(missing) }) }}
                    </template>
                    <template v-else>{{ t('generator.replaceSummary', { length: totalLength(dayLength) }) }}</template>
                </span>
                <div class="flex-auto" />
                <button type="button" class="btn-quiet" @click="close()">{{ t('cancel') }}</button>
                <button type="button" class="btn-main" :disabled="!canGenerate" @click="generate">
                    <span v-if="generating" class="loading loading-spinner loading-xs" />{{ t('generator.generate') }}
                </button>
            </footer>
        </section>
    </div>
</template>
