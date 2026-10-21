<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { storeToRefs } from 'pinia'

import FolderBadge from '@/components/storage/FolderBadge.vue'
import { authFetch } from '@/composables/authFetch'
import { stringFormatter } from '@/composables/helper'
import { formatLength } from '@/composables/schedule'
import { guideEntries, type GuideEntry } from '@/composables/storage'
import { useAuth } from '@/stores/auth'
import { useConfig } from '@/stores/config'

const props = defineProps({
    /** Items can be dragged onto the scheduler timeline. */
    draggable: { type: Boolean, default: false },
})

const emit = defineEmits<{ pick: [item: { path: string; name: string; duration: number }] }>()

const { t } = useI18n()
const { mediaType } = stringFormatter()
const authStore = useAuth()
const configStore = useConfig()
const { i } = storeToRefs(configStore)

interface BrowseResult {
    source: string
    parent: string
    folders: string[]
    sheet_folders?: string[]
    movie_folders?: string[]
    live_folders?: string[]
    commercial_folders?: string[]
    /** The commercials folder the browsed folder lies in, if any. */
    commercial?: string | null
    files: { name: string; duration: number }[]
}

const source = ref('')
const folders = ref<string[]>([])
const sheetFolders = ref<string[]>([])
const movieFolders = ref<string[]>([])
const liveFolders = ref<string[]>([])
const commercialFolders = ref<string[]>([])
/** The open folder is, or lies in, a commercials folder. */
const inCommercials = ref(false)
const files = ref<{ name: string; duration: number }[]>([])
/** How each clip of the open folder shows up in the guide, as in Storage. */
const guide = ref<Record<string, GuideEntry>>({})
const search = ref('')
const loading = ref(false)
const error = ref('')

const crumbs = computed(() => source.value.split('/').filter(Boolean))
const shownFiles = computed(() =>
    files.value.filter(
        (file) =>
            ['video', 'audio', 'image'].includes(mediaType(file.name) ?? '') &&
            file.name.toLowerCase().includes(search.value.trim().toLowerCase()),
    ),
)
const shownFolders = computed(() =>
    folders.value.filter((folder) => folder.toLowerCase().includes(search.value.trim().toLowerCase())),
)

function joinPath(...parts: string[]): string {
    return parts.join('/').split('/').filter(Boolean).join('/')
}

async function open(path: string) {
    const id = configStore.channels[configStore.i]?.id

    if (!id) {
        return
    }

    loading.value = true
    error.value = ''

    try {
        const data = await authFetch<BrowseResult>(`/api/file/${id}/browse`, {
            method: 'POST',
            headers: { ...configStore.contentType, ...authStore.authHeader },
            body: JSON.stringify({ source: `/${path}`, folders_only: false }),
        })

        source.value = joinPath(data.source ?? path)
        folders.value = data.folders ?? []
        sheetFolders.value = data.sheet_folders ?? []
        movieFolders.value = data.movie_folders ?? []
        liveFolders.value = data.live_folders ?? []
        commercialFolders.value = data.commercial_folders ?? []
        inCommercials.value = !!data.commercial
        files.value = data.files ?? []
        search.value = ''
        loadGuide(id)
    } catch (err) {
        error.value = err instanceof Error ? err.message : String(err)
    } finally {
        loading.value = false
    }
}

async function loadGuide(id: number) {
    const storage = configStore.channels[configStore.i]?.storage ?? ''
    const clips = files.value.filter((file) => mediaType(file.name) === 'video').slice(0, 300)
    const folder = source.value

    guide.value = {}

    if (!clips.length || !storage || inCommercials.value) {
        return
    }

    try {
        const found = await guideEntries(
            id,
            storage,
            clips.map((file) => joinPath(folder, file.name)),
        )

        // Another folder may have opened meanwhile.
        if (source.value === folder) {
            guide.value = Object.fromEntries(clips.map((file, index) => [file.name, found[index]]))
        }
    } catch {
        guide.value = {}
    }
}

function item(name: string, duration: number) {
    return { path: joinPath(source.value, name), name, duration }
}

function dragStart(event: DragEvent, name: string, duration: number) {
    const payload: ScheduleDragItem = { type: 'media', ...item(name, duration) }

    event.dataTransfer?.setData('application/x-noar-schedule', JSON.stringify(payload))
    event.dataTransfer?.setData('text/plain', payload.path)

    if (event.dataTransfer) {
        event.dataTransfer.effectAllowed = 'copy'
    }
}

onMounted(() => open(''))
watch(i, () => open(''))
</script>

<template>
    <div class="flex flex-col gap-2.5 min-h-0">
        <label
            class="flex items-center gap-2 bg-field border border-line-strong rounded-[10px] px-3 min-h-10 text-muted focus-within:outline-2 focus-within:outline-accent focus-within:outline-offset-1"
        >
            <i class="bi bi-search" aria-hidden="true" />
            <input
                v-model="search"
                type="search"
                :placeholder="t('schedule.searchMaterial')"
                class="flex-1 min-w-0 bg-transparent border-0 outline-none text-base-content"
            />
        </label>

        <nav class="text-[13px] text-muted flex flex-wrap items-center gap-1.5" :aria-label="t('schedule.folder')">
            <button
                class="bg-transparent border-0 p-0 text-inherit hover:text-base-content cursor-pointer"
                @click="open('')"
            >
                <i class="bi bi-hdd me-1" aria-hidden="true" />{{ t('button.media') }}
            </button>
            <template v-for="(crumb, index) in crumbs" :key="index">
                <i class="bi bi-chevron-right text-[10px]" aria-hidden="true" />
                <button
                    class="bg-transparent border-0 p-0 text-inherit hover:text-base-content cursor-pointer"
                    @click="open(crumbs.slice(0, index + 1).join('/'))"
                >
                    {{ crumb }}
                </button>
            </template>
        </nav>

        <div v-if="loading" class="py-4 flex justify-center">
            <span class="loading loading-spinner loading-sm" />
        </div>
        <p v-else-if="error" class="m-0 text-error">{{ error }}</p>

        <ul v-else class="m-0 p-0 list-none flex flex-col gap-0.5 overflow-auto">
            <li v-for="folder in shownFolders" :key="`d-${folder}`">
                <button
                    class="w-full flex items-center gap-2.5 px-2.5 min-h-10 rounded-lg bg-transparent border-0 text-inherit hover:bg-raised text-left cursor-pointer"
                    @click="open(joinPath(source, folder))"
                >
                    <i class="bi bi-folder2 text-muted" aria-hidden="true" />
                    <span class="flex-1 min-w-0 truncate">{{ folder }}</span>
                    <FolderBadge
                        :has-sheet="sheetFolders.includes(folder)"
                        :movie="movieFolders.includes(folder)"
                        :live="liveFolders.includes(folder)"
                        :commercial="commercialFolders.includes(folder)"
                    />
                    <i class="bi bi-chevron-right text-[11px] text-subtle" aria-hidden="true" />
                </button>
            </li>
            <li
                v-for="file in shownFiles"
                :key="`f-${file.name}`"
                class="flex items-center gap-2.5 px-2.5 min-h-10 rounded-lg hover:bg-raised"
                :class="{ 'cursor-grab': props.draggable }"
                :draggable="props.draggable"
                @dragstart="dragStart($event, file.name, file.duration)"
            >
                <i v-if="props.draggable" class="bi bi-grip-vertical text-subtle" aria-hidden="true" />
                <i v-else class="bi bi-film text-muted" aria-hidden="true" />
                <span class="flex-1 min-w-0 flex flex-col">
                    <span class="truncate" :title="file.name">{{ file.name }}</span>
                    <span v-if="guide[file.name]?.title" class="truncate text-xs text-muted">
                        {{ t('storage.guideColumn') }}: {{ guide[file.name]?.title }}
                        <template v-if="guide[file.name]?.sub_title"> · {{ guide[file.name]?.sub_title }}</template>
                    </span>
                </span>
                <span
                    v-if="inCommercials"
                    class="text-[11px] text-after-clip border border-after-clip/50 rounded px-1.5 flex-none"
                    :title="t('onair.commercialHint')"
                >
                    {{ t('onair.commercialTag') }}
                </span>
                <span class="font-mono text-xs text-muted">{{ formatLength(file.duration) }}</span>
                <button
                    v-if="!props.draggable"
                    class="btn-icon min-w-8 min-h-8"
                    :aria-label="t('schedule.addFile', { name: file.name })"
                    @click="emit('pick', item(file.name, file.duration))"
                >
                    <i class="bi bi-plus-lg" aria-hidden="true" />
                </button>
            </li>
            <li v-if="!shownFolders.length && !shownFiles.length" class="text-muted px-2.5 py-3">
                {{ t('schedule.emptyFolder') }}
            </li>
        </ul>
    </div>
</template>
