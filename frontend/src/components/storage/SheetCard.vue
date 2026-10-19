<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import { baseName, imageUrl } from '@/composables/storage'

/** The program sheet of the open folder: a summary, or how to create one. */
const props = defineProps<{
    channelId: number
    folder: string
    sheet: MediaSheet | null
    videos: number
    videosWithSheet: number
}>()

const emit = defineEmits<{
    edit: []
    artwork: [kind: ArtworkKind, file: File]
    uploadNfo: [file: File]
    openFolder: [path: string]
}>()

const { t } = useI18n()

const posterUrl = ref('')
const posterInput = ref<HTMLInputElement>()
const backdropInput = ref<HTMLInputElement>()
const nfoInput = ref<HTMLInputElement>()

const hasSheet = computed(() => Boolean(props.sheet?.file))
const fields = computed(() => props.sheet?.fields)
const title = computed(() => fields.value?.title || baseName(props.folder))

watch(
    () => props.sheet?.poster,
    async (poster) => {
        posterUrl.value = ''

        if (poster) {
            try {
                posterUrl.value = await imageUrl(props.channelId, poster)
            } catch {
                posterUrl.value = ''
            }
        }
    },
    { immediate: true },
)

function picked(event: Event, done: (file: File) => void) {
    const input = event.target as HTMLInputElement
    const file = input.files?.[0]

    if (file) {
        done(file)
    }

    input.value = ''
}
</script>
<template>
    <section v-if="sheet && hasSheet" aria-labelledby="sheet-card-title" class="card-box p-4.5 flex flex-wrap gap-5">
        <button
            type="button"
            class="w-[120px] h-[180px] flex-none rounded-[10px] overflow-hidden bg-base-300 border border-line-strong flex items-end p-2.5 text-xs text-muted cursor-pointer relative"
            :aria-label="sheet.poster ? t('storage.card.changePoster') : t('storage.card.addPoster')"
            @click="posterInput?.click()"
        >
            <img
                v-if="posterUrl"
                :src="posterUrl"
                :alt="t('storage.artwork.poster')"
                class="absolute inset-0 w-full h-full object-cover"
            />
            <span v-else class="flex flex-col items-start gap-1.5">
                <i class="bi bi-image text-lg" aria-hidden="true" />{{ t('storage.card.noPoster') }}
            </span>
        </button>

        <div class="flex-[1_1_360px] min-w-0 flex flex-col gap-2.5">
            <div class="flex flex-wrap items-center gap-2 text-xs text-muted uppercase tracking-[0.08em]">
                {{ sheet.kind === 'movie' ? t('storage.card.movieSheet') : t('storage.card.seriesSheet') }}
                <span class="normal-case tracking-normal font-mono text-subtle">{{ baseName(sheet.file ?? '') }}</span>
            </div>
            <h2 id="sheet-card-title" class="m-0 text-[22px] font-semibold">{{ title }}</h2>
            <div v-if="fields" class="flex flex-wrap gap-2 text-[13px]">
                <span
                    v-for="genre in fields.genres"
                    :key="genre"
                    class="bg-raised border border-line rounded-full px-2.5 py-0.5"
                >
                    {{ genre }}
                </span>
                <span v-if="fields.rating" class="bg-raised border border-line rounded-full px-2.5 py-0.5">
                    {{
                        fields.rating === 'Livre'
                            ? t('storage.sheet.ratingFree')
                            : t('storage.sheet.ratingAge', { age: fields.rating })
                    }}
                </span>
                <span v-if="fields.year" class="bg-raised border border-line rounded-full px-2.5 py-0.5">
                    {{ fields.year }}
                </span>
            </div>
            <p v-if="fields?.plot" class="m-0 text-base-content/85 max-w-3xl line-clamp-3">{{ fields.plot }}</p>
            <p v-else class="m-0 text-muted">{{ t('storage.card.noPlot') }}</p>
            <p
                v-if="sheet.kind !== 'movie'"
                class="m-0 flex gap-2 items-start bg-raised rounded-[10px] px-3 py-2.5 text-[13px] max-w-3xl"
            >
                <i class="bi bi-check2 text-ok mt-px" aria-hidden="true" />{{
                    t('storage.card.seriesHint', { count: videos }, videos)
                }}
            </p>
            <div class="flex flex-wrap gap-x-4 gap-y-2 text-[13px]">
                <span :class="sheet.poster ? 'text-ok' : 'text-accent-text'">
                    <i
                        class="bi me-1"
                        :class="sheet.poster ? 'bi-check-lg' : 'bi-exclamation-circle'"
                        aria-hidden="true"
                    />
                    {{ sheet.poster ? t('storage.card.poster') : t('storage.card.noPoster') }}
                </span>
                <span :class="sheet.backdrop ? 'text-ok' : 'text-accent-text'">
                    <i
                        class="bi me-1"
                        :class="sheet.backdrop ? 'bi-check-lg' : 'bi-exclamation-circle'"
                        aria-hidden="true"
                    />
                    {{ sheet.backdrop ? t('storage.card.backdrop') : t('storage.card.noBackdrop') }}
                </span>
                <span v-if="videos" class="text-muted">
                    {{ t('storage.card.videos', { count: videos, sheets: videosWithSheet }, videos) }}
                </span>
            </div>
            <div class="flex flex-wrap gap-2 mt-0.5">
                <button type="button" class="btn-main bg-base-content! text-base-100!" @click="emit('edit')">
                    <i class="bi bi-pencil" aria-hidden="true" />{{ t('storage.card.edit') }}
                </button>
                <button type="button" class="btn-quiet" @click="posterInput?.click()">
                    <i class="bi bi-image" aria-hidden="true" />
                    {{ sheet.poster ? t('storage.card.changePoster') : t('storage.card.addPoster') }}
                </button>
                <button type="button" class="btn-quiet" @click="backdropInput?.click()">
                    <i class="bi bi-upload" aria-hidden="true" />
                    {{ sheet.backdrop ? t('storage.card.changeBackdrop') : t('storage.card.addBackdrop') }}
                </button>
            </div>
        </div>
    </section>

    <section
        v-else-if="sheet && sheet.inherited_from"
        aria-labelledby="sheet-card-title"
        class="card-box p-4.5 flex flex-wrap items-center gap-4"
    >
        <div class="flex-[1_1_280px] min-w-0 flex flex-col gap-1">
            <h2 id="sheet-card-title" class="m-0 text-base font-semibold">
                {{ t('storage.card.inherits', { folder: baseName(sheet.inherited_from) }) }}
            </h2>
            <p class="m-0 text-muted">{{ t('storage.card.inheritsHint') }}</p>
        </div>
        <button type="button" class="btn-quiet" @click="emit('openFolder', sheet.inherited_from)">
            {{ t('storage.card.openInherited', { folder: baseName(sheet.inherited_from) }) }}
        </button>
        <button type="button" class="btn-quiet" @click="emit('edit')">{{ t('storage.card.ownSheet') }}</button>
    </section>

    <section
        v-else-if="sheet"
        aria-labelledby="sheet-card-title"
        class="card-box p-4.5 flex flex-wrap items-center gap-4.5"
    >
        <div
            class="w-[72px] h-[108px] flex-none rounded-[10px] border-[1.5px] border-dashed border-line-strong flex items-center justify-center text-subtle"
            aria-hidden="true"
        >
            <i class="bi bi-image text-xl" />
        </div>
        <div class="flex-[1_1_280px] min-w-0 flex flex-col gap-1.5">
            <h2 id="sheet-card-title" class="m-0 text-base font-semibold">
                {{ t('storage.card.empty', { folder: baseName(folder) }) }}
            </h2>
            <p class="m-0 text-muted">{{ t('storage.card.emptyHint') }}</p>
            <div class="flex flex-wrap gap-2 mt-1.5">
                <button type="button" class="btn-main" @click="emit('edit')">{{ t('storage.card.create') }}</button>
                <button type="button" class="btn-quiet" @click="nfoInput?.click()">
                    {{ t('storage.card.uploadNfo') }}
                </button>
            </div>
        </div>
    </section>

    <input
        ref="posterInput"
        type="file"
        class="hidden"
        accept="image/jpeg,image/png,image/webp,image/gif"
        @change="picked($event, (file) => emit('artwork', 'poster', file))"
    />
    <input
        ref="backdropInput"
        type="file"
        class="hidden"
        accept="image/jpeg,image/png,image/webp,image/gif"
        @change="picked($event, (file) => emit('artwork', 'backdrop', file))"
    />
    <input
        ref="nfoInput"
        type="file"
        class="hidden"
        accept=".nfo,.xml"
        @change="picked($event, (file) => emit('uploadNfo', file))"
    />
</template>
