<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { storeToRefs } from 'pinia'

import { authFetch } from '@/composables/authFetch'
import { stringFormatter } from '@/composables/helper'
import { formatLength } from '@/composables/schedule'
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
    files: { name: string; duration: number }[]
}

const source = ref('')
const folders = ref<string[]>([])
const files = ref<{ name: string; duration: number }[]>([])
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
        files.value = data.files ?? []
        search.value = ''
    } catch (err) {
        error.value = err instanceof Error ? err.message : String(err)
    } finally {
        loading.value = false
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
    <div class="flex flex-col gap-2 min-h-0">
        <label class="input input-sm w-full">
            <i class="bi bi-search opacity-60" />
            <input v-model="search" type="search" :placeholder="t('schedule.searchMaterial')" />
        </label>

        <nav class="text-xs text-base-content/70 flex flex-wrap items-center gap-1" :aria-label="t('schedule.folder')">
            <button class="hover:text-accent cursor-pointer" @click="open('')">
                <i class="bi bi-hdd" /> {{ t('button.media') }}
            </button>
            <template v-for="(crumb, index) in crumbs" :key="index">
                <span>/</span>
                <button class="hover:text-accent cursor-pointer" @click="open(crumbs.slice(0, index + 1).join('/'))">
                    {{ crumb }}
                </button>
            </template>
        </nav>

        <div v-if="loading" class="py-4 flex justify-center">
            <span class="loading loading-spinner loading-sm" />
        </div>
        <p v-else-if="error" class="text-error text-sm">{{ error }}</p>

        <ul v-else class="flex flex-col gap-0.5 overflow-auto">
            <li v-for="folder in shownFolders" :key="`d-${folder}`">
                <button
                    class="w-full flex items-center gap-2 px-2 py-1.5 rounded-md hover:bg-base-200 text-left cursor-pointer"
                    @click="open(joinPath(source, folder))"
                >
                    <i class="bi bi-folder-fill text-base-content/60" />
                    <span class="truncate">{{ folder }}</span>
                </button>
            </li>
            <li
                v-for="file in shownFiles"
                :key="`f-${file.name}`"
                class="flex items-center gap-2 px-2 py-1.5 rounded-md hover:bg-base-200"
                :class="{ 'cursor-grab': props.draggable }"
                :draggable="props.draggable"
                @dragstart="dragStart($event, file.name, file.duration)"
            >
                <i v-if="props.draggable" class="bi bi-grip-vertical text-base-content/50" aria-hidden="true" />
                <i v-else class="bi bi-film text-base-content/60" aria-hidden="true" />
                <span class="flex-1 min-w-0 truncate">{{ file.name }}</span>
                <span class="text-xs text-base-content/60 tabular-nums">{{ formatLength(file.duration) }}</span>
                <button
                    v-if="!props.draggable"
                    class="btn btn-xs btn-ghost"
                    :aria-label="t('schedule.addFile', { name: file.name })"
                    @click="emit('pick', item(file.name, file.duration))"
                >
                    <i class="bi bi-plus-lg" />
                </button>
            </li>
            <li v-if="!shownFolders.length && !shownFiles.length" class="text-sm text-base-content/60 px-2 py-3">
                {{ t('schedule.emptyFolder') }}
            </li>
        </ul>
    </div>
</template>
