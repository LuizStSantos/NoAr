<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { cloneDeep } from 'es-toolkit/object'
import { isEqual } from 'es-toolkit/predicate'

import ArtworkSlot from '@/components/storage/ArtworkSlot.vue'
import CreditsEditor from '@/components/storage/CreditsEditor.vue'
import { authFetch } from '@/composables/authFetch'
import { baseName, emptyFields, readSheet, removeArtwork, saveSheet, uploadArtwork } from '@/composables/storage'
import { useIndex } from '@/stores/index'

/**
 * Side panel that edits the `.nfo` sheet of a program folder or a clip, as a
 * form or as the file text, and its images.
 */
const props = defineProps<{
    channelId: number
    /** Storage path of the folder or clip. */
    path: string
}>()

const emit = defineEmits<{ close: []; saved: [sheet: MediaSheet]; open: [path: string] }>()

const { t } = useI18n()
const indexStore = useIndex()

const RATINGS = ['Livre', '10', '12', '14', '16', '18']

const sheet = ref<MediaSheet | null>(null)
const kind = ref<SheetKind>('program')
const fields = ref<SheetFields>(emptyFields())
/** The form as it was opened, to tell whether something changed. */
const baseline = ref<SheetFields>(emptyFields())
const raw = ref('')
const tab = ref<'form' | 'file'>('form')
const saving = ref(false)
const busyArtwork = ref<ArtworkKind | null>(null)
const confirmClose = ref(false)
/** The sheet to open once the changes are kept or dropped. */
const leaveTo = ref<string | null>(null)
const confirmDelete = ref(false)
const genreInput = ref('')
const firstField = ref<HTMLInputElement>()

const isClip = computed(() => sheet.value?.clip ?? false)
const stem = computed(() => baseName(props.path).replace(/\.[^.]+$/, ''))
const dirty = computed(() => {
    if (!sheet.value) {
        return false
    }

    if (tab.value === 'file') {
        return raw.value !== sheet.value.raw
    }

    return kind.value !== sheet.value.kind || !isEqual(fields.value, baseline.value)
})
const fileName = computed(() => {
    if (sheet.value?.file) {
        return baseName(sheet.value.file)
    }

    if (isClip.value) {
        return `${stem.value}.nfo`
    }

    return kind.value === 'movie' ? 'movie.nfo' : 'tvshow.nfo'
})
const kinds = computed<SheetKind[]>(() => (isClip.value ? ['episode', 'movie'] : ['program', 'movie']))
const ratingOptions = computed(() =>
    fields.value.rating && !RATINGS.includes(fields.value.rating) ? [...RATINGS, fields.value.rating] : RATINGS,
)

function listText(list: string[]): string {
    return list.join(', ')
}

function textList(text: string): string[] {
    return text
        .split(',')
        .map((part) => part.trim())
        .filter(Boolean)
}

const countries = computed({
    get: () => listText(fields.value.countries),
    set: (value: string) => (fields.value.countries = textList(value)),
})

function apply(next: MediaSheet) {
    sheet.value = next
    kind.value = next.kind
    fields.value = cloneDeep(next.fields)
    raw.value = next.raw

    // A new sheet starts with the folder or file name as its title.
    if (!next.file && !fields.value.title) {
        fields.value.title = next.clip ? stem.value : baseName(props.path)
    }

    baseline.value = cloneDeep(fields.value)
}

/** Numbers typed and then cleared come back as empty strings. */
function wholeNumber(value: unknown): number | null {
    return typeof value === 'number' && Number.isFinite(value) ? Math.max(0, Math.round(value)) : null
}

async function load() {
    try {
        apply(await readSheet(props.channelId, props.path))
        await nextTick()
        firstField.value?.focus()
    } catch (e) {
        indexStore.msgAlert('error', e instanceof Error ? e.message : String(e), 5)
        emit('close')
    }
}

function addGenre() {
    for (const genre of textList(genreInput.value)) {
        if (!fields.value.genres.includes(genre)) {
            fields.value.genres.push(genre)
        }
    }

    genreInput.value = ''
}

function removeGenre(genre: string) {
    fields.value.genres = fields.value.genres.filter((item) => item !== genre)
}

function onGenreKey(event: KeyboardEvent) {
    if (event.key === 'Enter' || event.key === ',') {
        event.preventDefault()
        addGenre()
    } else if (event.key === 'Backspace' && !genreInput.value) {
        fields.value.genres.pop()
    }
}

async function save() {
    if (genreInput.value.trim()) {
        addGenre()
    }

    saving.value = true

    try {
        const next =
            tab.value === 'file'
                ? await saveSheet(props.channelId, props.path, { raw: raw.value })
                : await saveSheet(props.channelId, props.path, {
                      kind: kind.value,
                      fields: {
                          ...fields.value,
                          season: wholeNumber(fields.value.season),
                          episode: wholeNumber(fields.value.episode),
                      },
                  })

        apply(next)
        emit('saved', next)
        indexStore.msgAlert('success', t('storage.sheet.saved'), 2)
        emit('close')
    } catch (e: any) {
        indexStore.msgAlert('error', e?.data ?? e?.message ?? String(e), 6)
    } finally {
        saving.value = false
    }
}

async function changeArtwork(slot: ArtworkKind, file: File | null) {
    busyArtwork.value = slot

    try {
        const next = file
            ? await uploadArtwork(props.channelId, props.path, slot, file)
            : await removeArtwork(props.channelId, props.path, slot)

        // Keep what is being typed; only the images changed.
        if (sheet.value) {
            sheet.value = { ...sheet.value, poster: next.poster, backdrop: next.backdrop, still: next.still }
        }

        emit('saved', next)
    } catch (e: any) {
        indexStore.msgAlert('error', e?.data ?? e?.message ?? String(e), 6)
    } finally {
        busyArtwork.value = null
    }
}

async function deleteSheet() {
    if (!sheet.value?.file) {
        return
    }

    saving.value = true

    try {
        await authFetch(`/api/file/${props.channelId}/remove`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json;charset=UTF-8' },
            body: JSON.stringify({ source: `/${sheet.value.file}`, recursive: false }),
        })

        const next = await readSheet(props.channelId, props.path)

        apply(next)
        emit('saved', next)
        emit('close')
    } catch (e: any) {
        indexStore.msgAlert('error', e?.data ?? e?.message ?? String(e), 6)
    } finally {
        saving.value = false
        confirmDelete.value = false
    }
}

function requestClose() {
    leaveTo.value = null

    if (dirty.value) {
        confirmClose.value = true
    } else {
        emit('close')
    }
}

/** Open another sheet (the program of an episode) instead of this one. */
function requestOpen(path: string) {
    leaveTo.value = path

    if (dirty.value) {
        confirmClose.value = true
    } else {
        emit('open', path)
    }
}

function leave() {
    if (leaveTo.value !== null) {
        emit('open', leaveTo.value)
    } else {
        emit('close')
    }
}

function onPhotos(photos: Record<string, string>) {
    if (sheet.value) {
        sheet.value = { ...sheet.value, photos }
    }
}

function onKey(event: KeyboardEvent) {
    if (event.key === 'Escape') {
        requestClose()
    }
}

onMounted(() => {
    document.body.style.overflow = 'hidden'
    window.addEventListener('keydown', onKey)
    load()
})

onBeforeUnmount(() => {
    document.body.style.overflow = ''
    window.removeEventListener('keydown', onKey)
})
</script>
<template>
    <div class="z-50 fixed inset-0 flex justify-end bg-black/50" @click.self="requestClose">
        <section
            role="dialog"
            aria-modal="true"
            aria-labelledby="sheet-title"
            class="w-full max-w-[800px] h-full overflow-y-auto bg-base-100 border-l border-line-strong shadow-2xl flex flex-col"
        >
            <header class="flex items-start gap-3 px-5 pt-5 pb-3">
                <div class="flex-1 min-w-0">
                    <h2 id="sheet-title" class="m-0 text-lg font-semibold">
                        {{ isClip ? t('storage.sheet.clipTitle') : t('storage.sheet.programTitle') }}
                    </h2>
                    <p class="m-0 text-[13px] text-muted">
                        <template v-if="isClip">
                            {{ t('storage.sheet.clipHint', { file: fileName }) }}
                        </template>
                        <template v-else>
                            {{ t('storage.sheet.programHint', { file: fileName, folder: baseName(path) }) }}
                        </template>
                    </p>
                </div>
                <button type="button" class="btn-icon" :aria-label="t('storage.close')" @click="requestClose">
                    <i class="bi bi-x-lg" aria-hidden="true" />
                </button>
            </header>

            <div v-if="!sheet" class="flex-1 flex items-center justify-center py-20">
                <span class="loading loading-spinner loading-lg" />
            </div>

            <template v-else>
                <div role="tablist" :aria-label="t('storage.sheet.mode')" class="flex gap-1 px-5 border-b border-line">
                    <button
                        v-for="name in ['form', 'file'] as const"
                        :key="name"
                        type="button"
                        role="tab"
                        :aria-selected="tab === name"
                        class="bg-transparent border-0 border-b-2 px-3 min-h-10 cursor-pointer"
                        :class="tab === name ? 'border-accent text-base-content' : 'border-transparent text-muted'"
                        @click="tab = name"
                    >
                        {{ name === 'form' ? t('storage.sheet.form') : t('storage.sheet.file') }}
                    </button>
                </div>

                <div v-if="tab === 'form'" class="px-5 py-4 flex flex-col gap-4">
                    <div role="radiogroup" :aria-label="t('storage.sheet.kind')" class="grid grid-cols-2 gap-2.5">
                        <label
                            v-for="option in kinds"
                            :key="option"
                            class="flex items-center gap-2.5 rounded-[10px] border px-3 min-h-11 cursor-pointer"
                            :class="option === kind ? 'border-accent bg-accent-soft' : 'border-line-strong'"
                        >
                            <input v-model="kind" type="radio" :value="option" class="radio radio-xs radio-accent" />
                            {{ t(`storage.sheet.kinds.${option}`) }}
                        </label>
                    </div>

                    <div class="grid grid-cols-2 sm:grid-cols-4 gap-3">
                        <label class="field-label col-span-2 sm:col-span-4">
                            {{
                                isClip && kind === 'episode'
                                    ? t('storage.sheet.episodeTitle')
                                    : t('storage.sheet.title')
                            }}
                            <input ref="firstField" v-model="fields.title" class="field-input" />
                        </label>

                        <template v-if="kind === 'episode'">
                            <label class="field-label">
                                {{ t('storage.sheet.season') }}
                                <input
                                    v-model.number="fields.season"
                                    type="number"
                                    min="0"
                                    class="field-input font-mono"
                                />
                            </label>
                            <label class="field-label">
                                {{ t('storage.sheet.episode') }}
                                <input
                                    v-model.number="fields.episode"
                                    type="number"
                                    min="0"
                                    class="field-input font-mono"
                                />
                            </label>
                            <label class="field-label col-span-2">
                                {{ t('storage.sheet.aired') }}
                                <input v-model="fields.premiered" type="date" class="field-input font-mono" />
                            </label>
                        </template>

                        <div class="field-label col-span-2 sm:col-span-4">
                            <span id="genres-label">{{ t('storage.sheet.genres') }}</span>
                            <div
                                class="flex flex-wrap items-center gap-1.5 bg-field border border-line-strong rounded-field px-2 py-1.5 min-h-10"
                            >
                                <span
                                    v-for="genre in fields.genres"
                                    :key="genre"
                                    class="inline-flex items-center gap-1 bg-base-300 text-base-content rounded-md ps-2 text-[13px]"
                                >
                                    {{ genre }}
                                    <button
                                        type="button"
                                        class="btn-icon min-w-6 min-h-6 text-muted"
                                        :aria-label="t('storage.sheet.removeGenre', { genre })"
                                        @click="removeGenre(genre)"
                                    >
                                        <i class="bi bi-x" aria-hidden="true" />
                                    </button>
                                </span>
                                <input
                                    v-model="genreInput"
                                    aria-labelledby="genres-label"
                                    :placeholder="t('storage.sheet.addGenre')"
                                    class="flex-1 min-w-28 bg-transparent border-0 outline-none text-base-content text-[13px] min-h-7"
                                    @keydown="onGenreKey"
                                    @blur="addGenre"
                                />
                            </div>
                        </div>

                        <label class="field-label col-span-2">
                            {{ t('storage.sheet.rating') }}
                            <select v-model="fields.rating" class="field-input">
                                <option value="">{{ t('storage.sheet.noRating') }}</option>
                                <option v-for="rating in ratingOptions" :key="rating" :value="rating">
                                    {{
                                        rating === 'Livre'
                                            ? t('storage.sheet.ratingFree')
                                            : t('storage.sheet.ratingAge', { age: rating })
                                    }}
                                </option>
                            </select>
                        </label>
                        <label class="field-label">
                            {{ t('storage.sheet.year') }}
                            <input
                                v-model="fields.year"
                                inputmode="numeric"
                                maxlength="4"
                                class="field-input font-mono"
                            />
                        </label>
                        <label class="field-label">
                            {{ t('storage.sheet.country') }}
                            <input v-model.lazy="countries" class="field-input" />
                        </label>

                        <label class="field-label col-span-2 sm:col-span-4">
                            {{ t('storage.sheet.plot') }}
                            <textarea v-model="fields.plot" rows="4" class="field-input py-2 resize-y" />
                        </label>
                    </div>

                    <CreditsEditor
                        v-model="fields.credits"
                        class="border-t border-line pt-4"
                        :channel-id="channelId"
                        :path="path"
                        :episode="isClip && kind === 'episode'"
                        :photos="sheet.photos"
                        :program-credits="sheet.program_credits"
                        :program="sheet.program"
                        @photos="onPhotos"
                        @open-program="requestOpen"
                    />
                </div>

                <div v-else class="px-5 py-4 flex flex-col gap-2">
                    <label class="field-label">
                        {{ fileName }}
                        <textarea
                            v-model="raw"
                            rows="18"
                            spellcheck="false"
                            class="field-input py-2 font-mono text-[13px] leading-snug resize-y"
                            :placeholder="t('storage.sheet.rawPlaceholder')"
                        />
                    </label>
                    <p class="m-0 field-hint">{{ t('storage.sheet.rawHint') }}</p>
                </div>

                <section class="px-5 pb-4 flex flex-col gap-2" :aria-label="t('storage.artwork.title')">
                    <h3 class="m-0 text-sm font-semibold">{{ t('storage.artwork.title') }}</h3>
                    <div
                        class="grid gap-3"
                        :class="
                            isClip ? 'grid-cols-[minmax(0,320px)_minmax(0,140px)]' : 'grid-cols-[140px_minmax(0,1fr)]'
                        "
                    >
                        <template v-if="isClip">
                            <ArtworkSlot
                                :channel-id="channelId"
                                :image="sheet.still"
                                :label="t('storage.artwork.still')"
                                ratio="16 / 9"
                                :target="`${stem}-thumb.jpg`"
                                :busy="busyArtwork === 'still'"
                                @pick="changeArtwork('still', $event)"
                                @remove="changeArtwork('still', null)"
                            />
                            <ArtworkSlot
                                v-if="kind === 'movie'"
                                :channel-id="channelId"
                                :image="sheet.poster"
                                :label="t('storage.artwork.poster')"
                                ratio="2 / 3"
                                :target="`${stem}-poster.jpg`"
                                :busy="busyArtwork === 'poster'"
                                compact
                                @pick="changeArtwork('poster', $event)"
                                @remove="changeArtwork('poster', null)"
                            />
                        </template>
                        <template v-else>
                            <ArtworkSlot
                                :channel-id="channelId"
                                :image="sheet.poster"
                                :label="t('storage.artwork.poster')"
                                ratio="2 / 3"
                                target="poster.jpg"
                                :busy="busyArtwork === 'poster'"
                                compact
                                @pick="changeArtwork('poster', $event)"
                                @remove="changeArtwork('poster', null)"
                            />
                            <ArtworkSlot
                                :channel-id="channelId"
                                :image="sheet.backdrop"
                                :label="t('storage.artwork.backdrop')"
                                ratio="16 / 9"
                                target="fanart.jpg"
                                :busy="busyArtwork === 'backdrop'"
                                @pick="changeArtwork('backdrop', $event)"
                                @remove="changeArtwork('backdrop', null)"
                            />
                        </template>
                    </div>
                    <p class="m-0 field-hint">{{ t('storage.artwork.instant') }}</p>
                </section>

                <footer
                    class="sticky bottom-0 mt-auto bg-base-100 border-t border-line px-5 py-3 flex flex-wrap items-center gap-2.5"
                >
                    <template v-if="confirmClose">
                        <span class="flex-1 text-[13px]">{{ t('storage.sheet.discardQuestion') }}</span>
                        <button type="button" class="btn-quiet" @click="confirmClose = false">
                            {{ t('storage.sheet.keepEditing') }}
                        </button>
                        <button type="button" class="btn-quiet text-error" @click="leave">
                            {{ t('storage.sheet.discard') }}
                        </button>
                    </template>
                    <template v-else-if="confirmDelete">
                        <span class="flex-1 text-[13px]">{{
                            t('storage.sheet.deleteQuestion', { file: fileName })
                        }}</span>
                        <button type="button" class="btn-quiet" @click="confirmDelete = false">
                            {{ t('cancel') }}
                        </button>
                        <button type="button" class="btn-quiet text-error" :disabled="saving" @click="deleteSheet">
                            {{ t('storage.sheet.delete') }}
                        </button>
                    </template>
                    <template v-else>
                        <button
                            v-if="sheet.file"
                            type="button"
                            class="link-accent text-error min-h-10"
                            @click="confirmDelete = true"
                        >
                            {{ t('storage.sheet.delete') }}
                        </button>
                        <span v-else class="text-[13px] text-muted">{{ t('storage.sheet.guideHint') }}</span>
                        <div class="flex-auto" />
                        <button type="button" class="btn-quiet" @click="requestClose">{{ t('cancel') }}</button>
                        <button
                            type="button"
                            class="btn-main"
                            :disabled="saving || (!dirty && !!sheet.file)"
                            @click="save"
                        >
                            <span v-if="saving" class="loading loading-spinner loading-xs" />
                            {{ isClip ? t('storage.sheet.saveClip') : t('storage.sheet.save') }}
                        </button>
                    </template>
                </footer>
            </template>
        </section>
    </div>
</template>
