<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import { authFetch } from '@/composables/authFetch'
import type { Conversion } from '@/composables/conversions'
import { stringFormatter } from '@/composables/helper'
import { baseName } from '@/composables/storage'
import { useAuth } from '@/stores/auth'
import { useUploads, type UploadItem } from '@/stores/uploads'

/** The uploads of the session, in a corner of every page. */
const { t } = useI18n()
const { fileSize } = stringFormatter()
const uploads = useUploads()
const authStore = useAuth()

const open = ref(true)

/**
 * After the upload, the server converts the video to the output format
 * (when the channel does): followed here, by channel and storage path, while
 * any is queued or converting.
 */
const conversions = ref(new Map<string, Conversion>())
let conversionTimer: ReturnType<typeof setTimeout> | undefined

function storagePath(folder: string, name: string): string {
    return [folder, name].filter(Boolean).join('/').replace(/^\/+/, '')
}

function conversionOf(item: UploadItem): Conversion | undefined {
    return item.status === 'done'
        ? conversions.value.get(`${item.channelId}\0${storagePath(item.folder, item.name)}`)
        : undefined
}

function converting(conversion: Conversion | undefined): boolean {
    return conversion?.state === 'queued' || conversion?.state === 'converting'
}

const convertingCount = computed(() => uploads.items.filter((item) => converting(conversionOf(item))).length)

async function pollConversions() {
    clearTimeout(conversionTimer)

    const channels = [...new Set(uploads.items.filter((item) => item.status === 'done').map((item) => item.channelId))]
    const next = new Map<string, Conversion>()

    for (const channel of channels) {
        try {
            const list = await authFetch<Conversion[]>(`/api/file/${channel}/conversions`, {
                headers: authStore.authHeader,
            })

            for (const conversion of list) {
                next.set(`${channel}\0${conversion.path.replace(/^\/+/, '')}`, conversion)
            }
        } catch {
            // Asked again with the next upload.
        }
    }

    conversions.value = next

    if ([...next.values()].some((conversion) => converting(conversion))) {
        conversionTimer = setTimeout(pollConversions, 3000)
    }
}

// Each finished upload may start a conversion.
watch(() => uploads.landed?.at, pollConversions, { immediate: true })
onBeforeUnmount(() => clearTimeout(conversionTimer))

const failed = computed(() => uploads.items.filter((item) => item.status === 'error').length)
const finished = computed(() => uploads.items.some((item) => item.status !== 'waiting' && item.status !== 'uploading'))
const percent = computed(() =>
    uploads.totalBytes > 0 ? Math.round((uploads.sentBytes / uploads.totalBytes) * 100) : 100,
)

const title = computed(() => {
    if (uploads.busy) {
        return t('uploads.sending', { count: uploads.active.length }, uploads.active.length)
    }

    if (convertingCount.value) {
        return t('uploads.converting', { count: convertingCount.value }, convertingCount.value)
    }

    return failed.value ? t('uploads.failed', { count: failed.value }, failed.value) : t('uploads.done')
})

const detail = computed(() => {
    if (!uploads.busy) {
        return ''
    }

    const left = uploads.totalBytes - uploads.sentBytes
    const parts = [t('uploads.bytes', { sent: size(uploads.sentBytes), total: size(uploads.totalBytes) })]

    if (uploads.speed > 0) {
        parts.push(`${size(uploads.speed)}/s`)
        parts.push(t('uploads.left', { time: duration(left / uploads.speed) }))
    }

    return parts.join(' · ')
})

function size(bytes: number): string {
    return bytes > 0 ? String(fileSize(bytes, 1)) : '0 B'
}

function duration(seconds: number): string {
    if (seconds < 60) {
        return t('uploads.seconds', { count: Math.max(1, Math.round(seconds)) })
    }

    if (seconds < 3600) {
        return t('uploads.minutes', { count: Math.round(seconds / 60) })
    }

    return t('uploads.hours', { count: Math.floor(seconds / 3600), minutes: Math.round((seconds % 3600) / 60) })
}

function folderName(item: UploadItem): string {
    return baseName(item.folder) || t('storage.root')
}

function itemLine(item: UploadItem): string {
    switch (item.status) {
        case 'uploading':
            return t('uploads.uploading', {
                folder: folderName(item),
                percent: item.size ? Math.floor((item.loaded / item.size) * 100) : 0,
            })
        case 'waiting':
            return t('uploads.waiting', { folder: folderName(item) })
        case 'done': {
            const conversion = conversionOf(item)

            if (conversion?.state === 'converting') {
                return t('uploads.conversion.converting', { percent: Math.floor(conversion.progress * 100) })
            }

            return conversion
                ? t(`uploads.conversion.${conversion.state}`, { folder: folderName(item) })
                : t('uploads.sent', { folder: folderName(item) })
        }
        case 'canceled':
            return t('uploads.canceled')
        default:
            return t('uploads.error', { error: item.error })
    }
}

function clearAll() {
    uploads.clearFinished()
}
</script>
<template>
    <section
        v-if="uploads.items.length"
        aria-labelledby="uploads-title"
        class="z-40 fixed bottom-3 inset-x-3 sm:inset-x-auto sm:right-6 sm:bottom-6 sm:w-[420px] bg-base-100 border border-line-strong rounded-[14px] shadow-2xl flex flex-col max-h-[70vh]"
    >
        <div class="flex items-center gap-2.5 px-3.5 py-3 border-b border-line">
            <i
                class="bi text-lg"
                :class="
                    uploads.busy
                        ? 'bi-cloud-arrow-up text-accent-text'
                        : convertingCount
                          ? 'bi-arrow-repeat text-info'
                          : failed
                            ? 'bi-exclamation-circle text-on-air-text'
                            : 'bi-check2-circle text-ok'
                "
                aria-hidden="true"
            />
            <div class="flex-1 min-w-0">
                <h2 id="uploads-title" class="m-0 text-[15px] font-semibold">{{ title }}</h2>
                <div v-if="detail" class="text-xs text-muted font-mono truncate">{{ detail }}</div>
            </div>
            <button
                type="button"
                class="btn-icon text-muted"
                :aria-label="open ? t('uploads.collapse') : t('uploads.expand')"
                :aria-expanded="open"
                @click="open = !open"
            >
                <i class="bi" :class="open ? 'bi-chevron-down' : 'bi-chevron-up'" aria-hidden="true" />
            </button>
            <button
                v-if="!uploads.busy"
                type="button"
                class="btn-icon text-muted"
                :aria-label="t('uploads.close')"
                @click="clearAll"
            >
                <i class="bi bi-x-lg" aria-hidden="true" />
            </button>
        </div>
        <div v-if="uploads.busy" class="h-1 bg-base-300" aria-hidden="true">
            <div class="h-1 bg-accent transition-[width]" :style="{ width: `${percent}%` }" />
        </div>

        <template v-if="open">
            <ul class="m-0 p-1.5 list-none flex flex-col gap-0.5 overflow-y-auto min-h-0">
                <li
                    v-for="item in uploads.items"
                    :key="item.id"
                    class="grid grid-cols-[minmax(0,1fr)_auto] gap-x-2.5 gap-y-1 items-center px-2.5 py-2 rounded-lg"
                    :class="item.status === 'uploading' ? 'bg-raised' : ''"
                >
                    <div class="min-w-0">
                        <div class="truncate" :title="item.name">{{ item.name }}</div>
                        <div
                            class="text-xs truncate"
                            :class="
                                converting(conversionOf(item))
                                    ? 'text-info'
                                    : conversionOf(item)?.state === 'failed' || item.status === 'error'
                                      ? 'text-on-air-text'
                                      : item.status === 'done'
                                        ? 'text-ok'
                                        : 'text-muted'
                            "
                            :title="item.error || conversionOf(item)?.error || undefined"
                        >
                            <i
                                v-if="converting(conversionOf(item))"
                                class="bi bi-arrow-repeat me-1"
                                aria-hidden="true"
                            /><i
                                v-else-if="conversionOf(item)?.state === 'failed'"
                                class="bi bi-exclamation-circle me-1"
                                aria-hidden="true"
                            /><i v-else-if="item.status === 'done'" class="bi bi-check2 me-1" aria-hidden="true" />
                            <i
                                v-else-if="item.status === 'error'"
                                class="bi bi-exclamation-circle me-1"
                                aria-hidden="true"
                            />{{ itemLine(item) }}
                        </div>
                    </div>
                    <button
                        v-if="item.status === 'waiting' || item.status === 'uploading'"
                        type="button"
                        class="btn-icon text-muted"
                        :aria-label="t('uploads.cancel', { name: item.name })"
                        @click="uploads.cancel(item.id)"
                    >
                        <i class="bi bi-x-lg" aria-hidden="true" />
                    </button>
                    <RouterLink
                        v-else-if="item.status === 'done'"
                        :to="{ path: '/media', query: { channel: item.channelId, path: item.folder } }"
                        class="link-accent text-[13px] whitespace-nowrap"
                    >
                        {{ t('uploads.openFolder') }}
                    </RouterLink>
                    <button
                        v-else
                        type="button"
                        class="link-accent text-[13px] whitespace-nowrap min-h-8"
                        @click="uploads.retry(item.id)"
                    >
                        {{ t('uploads.retry') }}
                    </button>
                    <div
                        v-if="item.status === 'uploading'"
                        class="col-span-full h-1 rounded-full bg-base-300 overflow-hidden"
                        aria-hidden="true"
                    >
                        <div
                            class="h-full bg-accent transition-[width]"
                            :style="{ width: `${item.size ? (item.loaded / item.size) * 100 : 0}%` }"
                        />
                    </div>
                    <!-- Blue: the server converting it, after the upload. -->
                    <div
                        v-else-if="converting(conversionOf(item))"
                        class="col-span-full h-1 rounded-full bg-base-300 overflow-hidden"
                        role="progressbar"
                        :aria-label="t('uploads.conversionOf', { name: item.name })"
                        aria-valuemin="0"
                        aria-valuemax="100"
                        :aria-valuenow="Math.floor((conversionOf(item)?.progress ?? 0) * 100)"
                    >
                        <div
                            class="h-full bg-info transition-[width]"
                            :class="conversionOf(item)?.state === 'queued' ? 'w-full opacity-30 animate-pulse' : ''"
                            :style="
                                conversionOf(item)?.state === 'converting'
                                    ? { width: `${(conversionOf(item)?.progress ?? 0) * 100}%` }
                                    : undefined
                            "
                        />
                    </div>
                </li>
            </ul>
            <div class="flex flex-wrap items-center gap-x-3 gap-y-1 px-3.5 pt-1.5 pb-3 text-[13px] text-muted">
                <span class="flex-1 min-w-48">{{ t('uploads.hint') }}</span>
                <button v-if="finished" type="button" class="link-accent text-muted! min-h-8" @click="clearAll">
                    {{ t('uploads.clear') }}
                </button>
            </div>
        </template>
    </section>
</template>
