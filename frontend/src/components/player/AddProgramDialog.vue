<script setup lang="ts">
import dayjs from 'dayjs'
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import { authFetch } from '@/composables/authFetch'
import { useDialogFocus } from '@/composables/dialogFocus'
import { useOnAir } from '@/composables/onAir'
import { clockTime, lengthLabel, totalLength, type AddPlace, type LibraryFile } from '@/composables/player'
import { useConfig } from '@/stores/config'
import { usePlaylist } from '@/stores/playlist'

/**
 * Adds a program folder to the playlist: the next episode, the whole folder
 * in episode order, or chosen videos.
 */
const props = defineProps<{ folder: string; dropIndex: number | null }>()
const emit = defineEmits<{ close: []; add: [files: LibraryFile[], place: AddPlace, label: string] }>()

const { t, locale } = useI18n()
const configStore = useConfig()
const playlistStore = usePlaylist()
const { onAirIndex, viewingOnAir } = useOnAir()

interface Episode {
    source: string
    name: string
    duration: number
    trim?: { from: number; to: number } | null
}

interface ProgramEpisodes {
    episodes: Episode[]
    last: { source: string; date: string | null } | null
    next: number
}

const loading = ref(true)
const error = ref('')
const found = ref<ProgramEpisodes | null>(null)
const choice = ref<'next' | 'all' | 'pick'>('next')
const picked = ref<string[]>([])

const channel = computed(() => configStore.channels[configStore.i])
const label = computed(() => props.folder.split('/').filter(Boolean).pop() ?? props.folder)
const storage = computed(() => (channel.value?.storage ?? '').replace(/\/+$/, ''))
const folderPath = computed(() => `${storage.value}/${props.folder}`.replace(/\/+$/, ''))
const episodes = computed(() => found.value?.episodes ?? [])
const next = computed(() => episodes.value[found.value?.next ?? 0])
const total = computed(() => episodes.value.reduce((sum, episode) => sum + episode.duration, 0))
const summary = computed(() =>
    t(
        'onair.program.summary',
        { count: episodes.value.length, length: totalLength(total.value) },
        episodes.value.length,
    ),
)

const places = computed(() => {
    const list: { value: string; label: string }[] = []
    const end = playlistStore.playlist.reduce((sum, item) => sum + (item.out - item.in), 0)
    const startOfDay = playlistStore.playlist[0]?.begin ?? configStore.playout.playlist?.startInSec ?? 0

    if (props.dropIndex !== null) {
        const begin = playlistStore.playlist[props.dropIndex]?.begin ?? startOfDay + end

        list.push({ value: 'drop', label: t('onair.program.placeDrop', { time: clockTime(begin) }) })
    }

    const selection = selectionEnd.value

    if (selection !== null) {
        const begin = playlistStore.playlist[selection]?.begin ?? startOfDay + end

        list.push({ value: 'selection', label: t('onair.program.placeSelection', { time: clockTime(begin) }) })
    }

    list.push({ value: 'end', label: t('onair.program.placeEnd', { time: clockTime(startOfDay + end) }) })

    if (viewingOnAir.value && onAirIndex.value >= 0) {
        list.push({ value: 'onair', label: t('onair.program.placeOnAir') })
    }

    if (playlistStore.playlist.length) {
        list.push({ value: 'start', label: t('onair.program.placeStart', { time: clockTime(startOfDay) }) })
        list.push({ value: 'time', label: t('onair.program.placeTime') })
    }

    return list
})

/** Where clips go after the selected ones in the playlist, if any are selected. */
const selectionEnd = computed(() => {
    const indices = playlistStore.playlist
        .map((item, index) => (playlistStore.selectedUids.includes(item.uid) ? index : -1))
        .filter((index) => index >= 0)

    return indices.length ? (indices[indices.length - 1] as number) + 1 : null
})

/** The time typed for "at a time", `HH:MM:SS`. */
const atTime = ref('')

/** The clip that plays at `atTime`: the new clips go before it. */
const timeIndex = computed(() => {
    const [hours = 0, minutes = 0, seconds = 0] = atTime.value.split(':').map(Number)
    const dayStart = configStore.playout.playlist?.startInSec ?? 0
    let target = hours * 3600 + minutes * 60 + seconds

    if (target < dayStart) {
        target += 86400
    }

    const at = playlistStore.playlist.findIndex((item) => (item.begin ?? 0) + (item.out - item.in) > target)

    return at < 0 ? playlistStore.playlist.length : at
})

const timeTarget = computed(() => {
    const item = playlistStore.playlist[timeIndex.value]

    return item ? { time: clockTime(item.begin ?? 0), name: item.source.split('/').pop() ?? '' } : null
})
const place = ref(props.dropIndex !== null ? 'drop' : selectionEnd.value !== null ? 'selection' : 'end')

const lastLine = computed(() => {
    const last = found.value?.last

    if (!last) {
        return t('onair.program.neverAired')
    }

    const name = last.source.split('/').pop() ?? last.source

    if (!last.date) {
        return t('onair.program.alreadyToday', { name })
    }

    return t('onair.program.airedOn', {
        name,
        date: dayjs(`${last.date}T12:00:00`).locale(locale.value.toLowerCase()).format('ddd, D MMM'),
    })
})

const chosen = computed<Episode[]>(() => {
    if (choice.value === 'next') {
        return next.value ? [next.value] : []
    }

    if (choice.value === 'all') {
        return episodes.value
    }

    return episodes.value.filter((episode) => picked.value.includes(episode.source))
})

/** The folder's last clip in the day being edited, so the next one follows it. */
function lastInPlaylist(): string | undefined {
    const prefix = `${folderPath.value}/`

    return [...playlistStore.playlist].reverse().find((item) => item.source.startsWith(prefix))?.source
}

async function load() {
    loading.value = true
    error.value = ''

    try {
        found.value = await authFetch<ProgramEpisodes>(`/api/playlist/${channel.value?.id}/episodes`, {
            method: 'POST',
            headers: configStore.contentType,
            body: JSON.stringify({ folder: props.folder, date: playlistStore.listDate, after: lastInPlaylist() }),
        })
    } catch (e: any) {
        error.value = e?.data ?? e?.message ?? String(e)
    } finally {
        loading.value = false
    }
}

function add() {
    const files = chosen.value.map((episode) => ({
        source: episode.source,
        name: episode.name,
        duration: episode.duration || 10,
        trim: episode.trim ?? null,
    }))
    const where: AddPlace =
        place.value === 'drop'
            ? (props.dropIndex ?? 'end')
            : place.value === 'start'
              ? 0
              : place.value === 'time'
                ? timeIndex.value
                : (place.value as 'end' | 'onair' | 'selection')

    emit('add', files, where, label.value)
}

const dialog = ref<HTMLElement>()

useDialogFocus(dialog, { onEscape: () => emit('close') })

onMounted(() => {
    // The page behind does not scroll under a finger on the dialog.
    document.body.style.overflow = 'hidden'
    load()
})

onBeforeUnmount(() => {
    document.body.style.overflow = ''
})
</script>
<template>
    <!-- On a phone a sheet from the bottom; else centred. Only the middle scrolls, so the buttons stay in view. -->
    <div
        class="z-50 fixed inset-0 flex items-end sm:items-center justify-center bg-black/55 sm:py-6 sm:px-4 overscroll-contain"
        @click.self="emit('close')"
    >
        <section
            ref="dialog"
            role="dialog"
            aria-modal="true"
            aria-labelledby="program-title"
            tabindex="-1"
            class="outline-none w-full sm:max-w-[460px] max-h-[92dvh] sm:max-h-[calc(100dvh-3rem)] bg-base-100 border border-line-strong rounded-t-[16px] sm:rounded-[16px] shadow-2xl flex flex-col"
        >
            <header class="flex items-start gap-3 px-5 pt-5 pb-2 shrink-0">
                <div class="flex-1 min-w-0">
                    <h2 id="program-title" class="m-0 text-lg font-semibold [overflow-wrap:anywhere]">
                        {{ t('onair.program.title', { name: label }) }}
                    </h2>
                    <p v-if="!loading && found" class="m-0 text-[13px] text-muted">
                        {{ summary }}
                    </p>
                </div>
                <button type="button" class="btn-icon" :aria-label="t('storage.close')" @click="emit('close')">
                    <i class="bi bi-x-lg" aria-hidden="true" />
                </button>
            </header>

            <div v-if="loading" class="py-10 flex justify-center"><span class="loading loading-spinner" /></div>
            <p v-else-if="error" role="alert" class="mx-5 my-4 text-on-air-text">{{ error }}</p>
            <p v-else-if="!episodes.length" class="mx-5 my-4 text-muted">{{ t('onair.program.empty') }}</p>

            <div v-else class="px-5 py-3 flex flex-col gap-3.5 flex-1 min-h-0 overflow-y-auto overscroll-contain">
                <fieldset class="border-0 p-0 m-0 min-w-0 flex flex-col gap-2">
                    <legend class="text-xs text-muted mb-1.5">{{ t('onair.program.what') }}</legend>
                    <label
                        class="flex gap-2.5 items-start min-w-0 rounded-[10px] border px-3 py-2.5 cursor-pointer"
                        :class="choice === 'next' ? 'border-accent bg-accent-soft' : 'border-line-strong'"
                    >
                        <input v-model="choice" type="radio" value="next" class="radio radio-xs radio-accent mt-1" />
                        <span class="min-w-0 flex-1">
                            <span class="block font-semibold">{{ t('onair.program.next') }}</span>
                            <span class="block text-[13px] text-base-content/85 [overflow-wrap:anywhere]" :title="next?.name">
                                {{ next?.name }} · {{ lengthLabel(next?.duration ?? 0) }}
                            </span>
                            <span class="block text-xs text-muted [overflow-wrap:anywhere]">{{ lastLine }}</span>
                        </span>
                    </label>
                    <label
                        class="flex gap-2.5 items-start min-w-0 rounded-[10px] border px-3 py-2.5 cursor-pointer"
                        :class="choice === 'all' ? 'border-accent bg-accent-soft' : 'border-line-strong'"
                    >
                        <input v-model="choice" type="radio" value="all" class="radio radio-xs radio-accent mt-1" />
                        <span class="min-w-0 flex-1">
                            <span class="block font-semibold">{{ t('onair.program.all') }}</span>
                            <span class="block text-[13px] text-base-content/85">
                                {{ summary }}
                            </span>
                        </span>
                    </label>
                    <label
                        class="flex gap-2.5 items-start min-w-0 rounded-[10px] border px-3 py-2.5 cursor-pointer"
                        :class="choice === 'pick' ? 'border-accent bg-accent-soft' : 'border-line-strong'"
                    >
                        <input v-model="choice" type="radio" value="pick" class="radio radio-xs radio-accent mt-1" />
                        <span class="min-w-0 flex-1">
                            <span class="block font-semibold">{{ t('onair.program.pick') }}</span>
                            <span class="block text-[13px] text-base-content/85">{{
                                t('onair.program.pickHint')
                            }}</span>
                        </span>
                    </label>
                </fieldset>

                <ul
                    v-if="choice === 'pick'"
                    class="m-0 p-1 list-none border border-line rounded-[10px] bg-base-200 sm:max-h-56 sm:overflow-y-auto"
                >
                    <li v-for="episode in episodes" :key="episode.source">
                        <label
                            class="flex items-center gap-2.5 px-2 min-h-10 rounded-lg cursor-pointer hover:bg-raised"
                        >
                            <input
                                v-model="picked"
                                type="checkbox"
                                :value="episode.source"
                                class="checkbox checkbox-sm checkbox-accent rounded-[4px]"
                            />
                            <span class="flex-1 min-w-0 truncate" :title="episode.name">{{ episode.name }}</span>
                            <span class="font-mono text-xs text-muted">{{ lengthLabel(episode.duration) }}</span>
                        </label>
                    </li>
                </ul>

                <label class="field-label">
                    {{ t('onair.program.where') }}
                    <select v-model="place" class="field-input">
                        <option v-for="option in places" :key="option.value" :value="option.value">
                            {{ option.label }}
                        </option>
                    </select>
                </label>
                <label v-if="place === 'time'" class="field-label">
                    {{ t('onair.program.atTime') }}
                    <input v-model="atTime" type="time" step="1" class="field-input font-mono" />
                    <span v-if="atTime && timeTarget" class="field-hint">
                        {{ t('onair.program.atTimeHint', timeTarget) }}
                    </span>
                </label>
            </div>

            <footer
                class="flex justify-end gap-2.5 px-5 pt-4 pb-[max(1rem,env(safe-area-inset-bottom))] sm:pb-4 border-t border-line shrink-0"
            >
                <button type="button" class="btn-quiet" @click="emit('close')">{{ t('cancel') }}</button>
                <button
                    type="button"
                    class="btn-main"
                    :disabled="loading || !chosen.length || (place === 'time' && !atTime)"
                    @click="add"
                >
                    {{ t('onair.program.add', chosen.length) }}
                </button>
            </footer>
        </section>
    </div>
</template>
