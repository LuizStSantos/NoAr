<script setup lang="ts">
import dayjs from 'dayjs'
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import { authFetch } from '@/composables/authFetch'
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

    list.push({ value: 'end', label: t('onair.program.placeEnd', { time: clockTime(startOfDay + end) }) })

    if (viewingOnAir.value && onAirIndex.value >= 0) {
        list.push({ value: 'onair', label: t('onair.program.placeOnAir') })
    }

    return list
})
const place = ref(props.dropIndex !== null ? 'drop' : 'end')

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
    }))
    const where: AddPlace = place.value === 'drop' ? (props.dropIndex ?? 'end') : (place.value as 'end' | 'onair')

    emit('add', files, where, label.value)
}

function onKey(event: KeyboardEvent) {
    if (event.key === 'Escape') {
        emit('close')
    }
}

onMounted(() => {
    window.addEventListener('keydown', onKey)
    load()
})

onBeforeUnmount(() => window.removeEventListener('keydown', onKey))
</script>
<template>
    <div class="z-50 fixed inset-0 flex justify-center bg-black/55 overflow-auto py-6 px-4" @click.self="emit('close')">
        <section
            role="dialog"
            aria-modal="true"
            aria-labelledby="program-title"
            class="my-auto w-full max-w-[460px] bg-base-100 border border-line-strong rounded-[16px] shadow-2xl flex flex-col"
        >
            <header class="flex items-start gap-3 px-5 pt-5 pb-2">
                <div class="flex-1 min-w-0">
                    <h2 id="program-title" class="m-0 text-lg font-semibold">
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

            <div v-else class="px-5 py-3 flex flex-col gap-3.5">
                <fieldset class="border-0 p-0 m-0 flex flex-col gap-2">
                    <legend class="text-xs text-muted mb-1.5">{{ t('onair.program.what') }}</legend>
                    <label
                        class="flex gap-2.5 items-start rounded-[10px] border px-3 py-2.5 cursor-pointer"
                        :class="choice === 'next' ? 'border-accent bg-accent-soft' : 'border-line-strong'"
                    >
                        <input v-model="choice" type="radio" value="next" class="radio radio-xs radio-accent mt-1" />
                        <span class="min-w-0">
                            <span class="block font-semibold">{{ t('onair.program.next') }}</span>
                            <span class="block text-[13px] text-base-content/85 truncate" :title="next?.name">
                                {{ next?.name }} · {{ lengthLabel(next?.duration ?? 0) }}
                            </span>
                            <span class="block text-xs text-muted">{{ lastLine }}</span>
                        </span>
                    </label>
                    <label
                        class="flex gap-2.5 items-start rounded-[10px] border px-3 py-2.5 cursor-pointer"
                        :class="choice === 'all' ? 'border-accent bg-accent-soft' : 'border-line-strong'"
                    >
                        <input v-model="choice" type="radio" value="all" class="radio radio-xs radio-accent mt-1" />
                        <span>
                            <span class="block font-semibold">{{ t('onair.program.all') }}</span>
                            <span class="block text-[13px] text-base-content/85">
                                {{ summary }}
                            </span>
                        </span>
                    </label>
                    <label
                        class="flex gap-2.5 items-start rounded-[10px] border px-3 py-2.5 cursor-pointer"
                        :class="choice === 'pick' ? 'border-accent bg-accent-soft' : 'border-line-strong'"
                    >
                        <input v-model="choice" type="radio" value="pick" class="radio radio-xs radio-accent mt-1" />
                        <span>
                            <span class="block font-semibold">{{ t('onair.program.pick') }}</span>
                            <span class="block text-[13px] text-base-content/85">{{
                                t('onair.program.pickHint')
                            }}</span>
                        </span>
                    </label>
                </fieldset>

                <ul
                    v-if="choice === 'pick'"
                    class="m-0 p-1 list-none border border-line rounded-[10px] bg-base-200 max-h-56 overflow-y-auto"
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
            </div>

            <footer class="flex justify-end gap-2.5 px-5 py-4 mt-1 border-t border-line">
                <button type="button" class="btn-quiet" @click="emit('close')">{{ t('cancel') }}</button>
                <button type="button" class="btn-main" :disabled="loading || !chosen.length" @click="add">
                    {{ t('onair.program.add', chosen.length) }}
                </button>
            </footer>
        </section>
    </div>
</template>
