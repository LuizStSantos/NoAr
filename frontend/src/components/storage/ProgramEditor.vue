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
 * breaks of commercials between them. Playlists and the Scheduler point to
 * the file, so a change applies to every airing still to come.
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
    /** Seconds that air; `null` until the server reads it on save. */
    length: number | null
}

type Part = { kind: 'block'; clip: Clip } | { kind: 'break'; clips: Clip[] }

const fileName = ref('')
const title = ref('')
const parts = ref<Part[]>([])
const loading = ref(Boolean(props.path))
const saving = ref(false)
const error = ref('')
/** Where a clip picked in the storage goes. */
const picking = ref<{ kind: 'block'; at: number } | { kind: 'commercial'; part: number } | null>(null)
const dialog = ref<HTMLElement>()

useDialogFocus(dialog, { onEscape: () => (picking.value ? (picking.value = null) : emit('close')) })

const blockCount = computed(() => parts.value.filter((part) => part.kind === 'block').length)
const total = computed(() =>
    parts.value.reduce(
        (sum, part) =>
            sum +
            (part.kind === 'block'
                ? (part.clip.length ?? 0)
                : part.clips.reduce((inner, clip) => inner + (clip.length ?? 0), 0)),
        0,
    ),
)
const breakLength = (clips: Clip[]) => clips.reduce((sum, clip) => sum + (clip.length ?? 0), 0)

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

function addBlock() {
    picking.value = { kind: 'block', at: parts.value.length }
}

/** A break goes after block `index`, before the next one. */
function addBreak(index: number) {
    parts.value.splice(index + 1, 0, { kind: 'break', clips: [] })
    picking.value = { kind: 'commercial', part: index + 1 }
}

function pick(item: { path: string; duration: number }) {
    const target = picking.value

    if (!target) {
        return
    }

    const clip = { source: item.path, length: item.duration || null }

    if (target.kind === 'block') {
        parts.value.splice(target.at, 0, { kind: 'block', clip })
        picking.value = null
    } else {
        const part = parts.value[target.part]

        if (part?.kind === 'break') {
            part.clips.push(clip)
        }
    }
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

function check(): string {
    if (!title.value.trim()) {
        return t('storage.program.needTitle')
    }
    if (!props.path && !fileName.value.trim().replace(/[\\/:*?"<>|]/g, '')) {
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

async function save() {
    error.value = check()

    if (error.value) {
        return
    }

    const path =
        props.path ??
        [props.folder, `${fileName.value.trim().replace(/[\\/:*?"<>|]/g, '')}.program`].filter(Boolean).join('/')

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
    <div class="z-50 fixed inset-0 bg-black/50 overflow-auto flex justify-center items-start py-8 px-3">
        <div
            ref="dialog"
            role="dialog"
            aria-modal="true"
            aria-labelledby="program-editor-title"
            tabindex="-1"
            class="outline-none w-full max-w-240 bg-base-100 border border-base-content/20 rounded-lg shadow-xl flex flex-col"
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

            <div v-else class="flex flex-col lg:flex-row gap-4 px-5 py-4">
                <div class="flex flex-col gap-4 flex-1 min-w-0">
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
                    <p class="m-0 field-hint">{{ t('storage.program.hint') }}</p>

                    <ol class="list-none m-0 p-0 flex flex-col gap-2" :aria-label="t('storage.program.parts')">
                        <li
                            v-for="(part, index) in parts"
                            :key="index"
                            class="border rounded-lg px-3 py-2 flex flex-col gap-1.5"
                            :class="part.kind === 'break' ? 'border-warning/60 bg-warning/5' : 'border-line-strong'"
                        >
                            <div class="flex items-center gap-2">
                                <i
                                    class="bi"
                                    :class="part.kind === 'block' ? 'bi-film' : 'bi-megaphone'"
                                    aria-hidden="true"
                                />
                                <span class="font-semibold">
                                    {{
                                        part.kind === 'block'
                                            ? t('storage.program.block', {
                                                  n: parts.slice(0, index + 1).filter((p) => p.kind === 'block')
                                                      .length,
                                              })
                                            : t('storage.program.break')
                                    }}
                                </span>
                                <span v-if="part.kind === 'block'" class="truncate text-[13px] font-mono text-muted">
                                    {{ clipName(part.clip.source) }}
                                </span>
                                <span class="ms-auto text-[13px] font-mono">
                                    {{
                                        part.kind === 'block'
                                            ? part.clip.length !== null
                                                ? formatLength(part.clip.length)
                                                : '—'
                                            : formatLength(breakLength(part.clips))
                                    }}
                                </span>
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
                                <ul class="list-none m-0 p-0 flex flex-col gap-1 ps-6">
                                    <li
                                        v-for="(clip, clipIndex) in part.clips"
                                        :key="clipIndex"
                                        class="flex items-center gap-2 text-[13px]"
                                    >
                                        <span class="truncate font-mono">{{ clipName(clip.source) }}</span>
                                        <span class="ms-auto font-mono text-muted">
                                            {{ clip.length !== null ? formatLength(clip.length) : '—' }}
                                        </span>
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
                                <button
                                    type="button"
                                    class="link-accent self-start min-h-8 ps-6"
                                    @click="picking = { kind: 'commercial', part: index }"
                                >
                                    + {{ t('storage.program.addCommercial') }}
                                </button>
                            </template>
                            <button
                                v-else-if="parts[index + 1]?.kind !== 'break' && index < parts.length - 1"
                                type="button"
                                class="link-accent self-start min-h-8 ps-6 text-[13px]"
                                @click="addBreak(index)"
                            >
                                + {{ t('storage.program.addBreak') }}
                            </button>
                        </li>
                    </ol>

                    <div class="flex flex-wrap items-center gap-3">
                        <button type="button" class="btn-quiet" @click="addBlock">
                            <i class="bi bi-plus-lg" aria-hidden="true" />{{ t('storage.program.addBlock') }}
                        </button>
                        <span class="ms-auto text-sm">
                            {{ t('storage.program.total') }}
                            <strong class="font-mono">{{ formatLength(total) }}</strong>
                        </span>
                    </div>
                    <p v-if="error" class="m-0 text-error text-sm" role="alert">{{ error }}</p>
                </div>

                <aside
                    v-if="picking"
                    class="lg:w-90 shrink-0 border border-line-strong rounded-lg p-3 flex flex-col gap-2"
                    :aria-label="t('storage.program.pickTitle')"
                >
                    <div class="flex items-center justify-between">
                        <span class="font-semibold text-sm">
                            {{
                                picking.kind === 'block'
                                    ? t('storage.program.pickBlock')
                                    : t('storage.program.pickCommercial')
                            }}
                        </span>
                        <button type="button" class="btn-icon" :aria-label="t('schedule.close')" @click="picking = null">
                            <i class="bi bi-x-lg" aria-hidden="true" />
                        </button>
                    </div>
                    <StoragePicker hide-lives @pick="pick" />
                </aside>
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
