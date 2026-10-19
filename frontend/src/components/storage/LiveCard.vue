<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import { baseName, imageUrl } from '@/composables/storage'

/** A live's folder in Storage: its address, signal and guide data. */
const props = defineProps<{
    channelId: number
    sheet: MediaSheet | null
    /** The live of this folder, once the list of lives has loaded. */
    live: LiveSource | null
    /** Its next time in the Scheduler, already worded. */
    next: string
    /** The last signal test of this live. */
    test: { ok: boolean; text: string; at: string } | null
    testing: boolean
}>()

const emit = defineEmits<{
    edit: []
    test: []
    artwork: [kind: ArtworkKind, file: File]
    schedule: []
}>()

const { t } = useI18n()

const posterUrl = ref('')
const posterInput = ref<HTMLInputElement>()
const backdropInput = ref<HTMLInputElement>()
const reveal = ref(false)

const fields = computed(() => props.sheet?.fields)
const title = computed(() => fields.value?.title || props.live?.title || props.live?.name || '')

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

const SECRET = /pass|key|token|streamid|secret|auth|user/i
const MASK = '••••••'

/** The address with its secrets (stream keys, passphrases) hidden. */
function masked(address: string): string {
    const [base = '', query] = address.split('?', 2)
    let shown = base

    // An RTMP stream key is the last part of the path.
    if (/^rtmps?:\/\//i.test(base)) {
        const parts = base.split('/')

        if (parts.length > 4) {
            parts[parts.length - 1] = MASK
            shown = parts.join('/')
        }
    }

    if (!query) {
        return shown
    }

    const params = query
        .split('&')
        .map((pair) => {
            const [key = '', value] = pair.split('=', 2)

            return value !== undefined && SECRET.test(key) ? `${key}=${MASK}` : pair
        })
        .join('&')

    return `${shown}?${params}`
}

const address = computed(() => {
    const url = props.live?.url ?? ''

    return reveal.value ? url : masked(url)
})

const canReveal = computed(() => Boolean(props.live) && masked(props.live?.url ?? '') !== props.live?.url)

function picked(event: Event, kind: ArtworkKind) {
    const input = event.target as HTMLInputElement
    const file = input.files?.[0]

    if (file) {
        emit('artwork', kind, file)
    }

    input.value = ''
}
</script>
<template>
    <section aria-labelledby="live-card-title" class="card-box p-4.5 flex flex-wrap gap-5">
        <button
            type="button"
            class="w-[120px] h-[180px] flex-none rounded-[10px] overflow-hidden bg-base-300 border border-line-strong flex items-end p-2.5 text-xs text-muted cursor-pointer relative"
            :aria-label="sheet?.poster ? t('storage.card.changePoster') : t('storage.card.addPoster')"
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

        <div class="flex-[1_1_420px] min-w-0 flex flex-col gap-2.5">
            <div class="flex flex-wrap items-center gap-2 text-xs text-live uppercase tracking-[0.08em]">
                {{ t('storage.live.kicker') }}
                <span v-if="live" class="normal-case tracking-normal font-mono text-subtle">{{
                    baseName(live.path)
                }}</span>
            </div>
            <h2 id="live-card-title" class="m-0 text-[22px] font-semibold">{{ title }}</h2>
            <div v-if="fields && (fields.genres.length || fields.rating)" class="flex flex-wrap gap-2 text-[13px]">
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
            </div>
            <p v-if="fields?.plot" class="m-0 text-base-content/85 max-w-3xl line-clamp-3">{{ fields.plot }}</p>

            <div class="flex flex-wrap items-center gap-x-3 gap-y-2 bg-raised rounded-[10px] px-3 py-2.5 max-w-3xl">
                <span
                    v-if="live"
                    class="text-xs font-semibold text-live border border-live/50 rounded-md px-1.5 leading-5"
                >
                    {{ live.protocol }}
                </span>
                <span class="flex-[1_1_240px] min-w-0 font-mono text-[13px] truncate" :title="reveal ? live?.url : ''">
                    {{ live ? address : t('storage.live.loading') }}
                </span>
                <button
                    v-if="canReveal"
                    type="button"
                    class="link-accent text-[13px] min-h-8"
                    :aria-pressed="reveal"
                    @click="reveal = !reveal"
                >
                    {{ reveal ? t('storage.live.hide') : t('storage.live.show') }}
                </button>
                <span v-if="testing" class="inline-flex items-center gap-1.5 text-[13px] text-muted" role="status">
                    <span class="loading loading-spinner loading-xs" />{{ t('storage.live.testing') }}
                </span>
                <span
                    v-else-if="test"
                    class="inline-flex items-center gap-1.5 text-[13px]"
                    :class="test.ok ? 'text-ok' : 'text-error'"
                    role="status"
                    :title="test.text"
                >
                    <span class="size-2 rounded-full" :class="test.ok ? 'bg-ok' : 'bg-error'" aria-hidden="true" />
                    {{
                        test.ok
                            ? t('storage.live.signalOk', { time: test.at })
                            : t('storage.live.signalFailed', { time: test.at })
                    }}
                </span>
            </div>

            <div class="flex flex-wrap gap-x-4 gap-y-2 text-[13px]">
                <span :class="sheet?.poster ? 'text-ok' : 'text-accent-text'">
                    <i
                        class="bi me-1"
                        :class="sheet?.poster ? 'bi-check-lg' : 'bi-exclamation-circle'"
                        aria-hidden="true"
                    />
                    {{ sheet?.poster ? t('storage.card.poster') : t('storage.card.noPoster') }}
                </span>
                <span :class="sheet?.backdrop ? 'text-ok' : 'text-accent-text'">
                    <i
                        class="bi me-1"
                        :class="sheet?.backdrop ? 'bi-check-lg' : 'bi-exclamation-circle'"
                        aria-hidden="true"
                    />
                    {{ sheet?.backdrop ? t('storage.card.backdrop') : t('storage.card.noBackdrop') }}
                </span>
            </div>

            <div class="flex flex-wrap gap-2 mt-0.5">
                <button
                    type="button"
                    class="btn-main bg-base-content! text-base-100!"
                    :disabled="!live"
                    @click="emit('edit')"
                >
                    <i class="bi bi-pencil" aria-hidden="true" />{{ t('storage.live.edit') }}
                </button>
                <button type="button" class="btn-quiet" :disabled="!live || testing" @click="emit('test')">
                    <i class="bi bi-activity" aria-hidden="true" />{{ t('schedule.testSignal') }}
                </button>
                <button type="button" class="btn-quiet" @click="posterInput?.click()">
                    <i class="bi bi-image" aria-hidden="true" />
                    {{ sheet?.poster ? t('storage.card.changePoster') : t('storage.card.addPoster') }}
                </button>
                <button type="button" class="btn-quiet" @click="backdropInput?.click()">
                    <i class="bi bi-upload" aria-hidden="true" />
                    {{ sheet?.backdrop ? t('storage.card.changeBackdrop') : t('storage.card.addBackdrop') }}
                </button>
                <button type="button" class="link-accent px-1.5 min-h-10" :disabled="!live" @click="emit('schedule')">
                    {{ t('storage.live.schedule') }} →
                </button>
            </div>
            <p class="m-0 text-[13px] text-muted max-w-3xl">
                {{ t('storage.live.hint') }}
                <span class="text-base-content">{{ next }}</span>
            </p>
        </div>

        <input
            ref="posterInput"
            type="file"
            class="hidden"
            accept="image/jpeg,image/png,image/webp,image/gif"
            @change="picked($event, 'poster')"
        />
        <input
            ref="backdropInput"
            type="file"
            class="hidden"
            accept="image/jpeg,image/png,image/webp,image/gif"
            @change="picked($event, 'backdrop')"
        />
    </section>
</template>
