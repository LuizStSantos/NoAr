<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import StoragePicker from '@/components/schedule/StoragePicker.vue'
import { storageImageUrl } from '@/composables/logo'
import { useConfig } from '@/stores/config'
import { authFetch } from '@/composables/authFetch'
import { useAuth } from '@/stores/auth'

const configStore = useConfig()
const channel = computed(() => configStore.channels[configStore.i])
const channelId = computed(() => channel.value?.id)
const { t } = useI18n()
const auth = useAuth()
const path = ref('')
const baseline = ref('')
const ready = ref(false)
const busy = ref(false)
const error = ref('')
const saved = ref(false)
const fallbackPath = ref('')
const preview = ref('')
const previewFailed = ref(false)
const picking = ref(false)
let previewVersion = 0
let version = 0

async function load() {
    const request = ++version
    ready.value = false
    error.value = ''

    try {
        const config = await authFetch<PlayoutConfigExt>(`/api/playout/config/${channelId.value}`, {
            headers: auth.authHeader,
        })

        if (request !== version) {
            return
        }

        path.value = baseline.value = config.processing.guide_logo
        fallbackPath.value = config.processing.logo
        ready.value = true
    } catch (failure) {
        if (request === version) {
            error.value = String(failure)
        }
    }
}

async function save() {
    const targetChannel = channelId.value
    busy.value = true
    error.value = ''
    saved.value = false

    try {
        // Read the current configuration so this independent guide setting
        // cannot overwrite a model just saved in the Composer.
        const config = await authFetch<PlayoutConfigExt>(`/api/playout/config/${targetChannel}`, {
            headers: auth.authHeader,
        })
        config.processing.guide_logo = path.value.trim()
        await authFetch(`/api/playout/config/${targetChannel}`, {
            method: 'PUT',
            headers: { ...auth.authHeader, 'Content-Type': 'application/json' },
            body: JSON.stringify(config),
        })

        if (targetChannel === channelId.value) {
            baseline.value = config.processing.guide_logo
            path.value = baseline.value
            saved.value = true
        }
    } catch (failure) {
        if (targetChannel === channelId.value) {
            error.value = String(failure)
        }
    } finally {
        busy.value = false
    }
}

async function loadPreview() {
    const request = ++previewVersion
    previewFailed.value = false
    preview.value = ''

    try {
        const url = await storageImageUrl(channel.value, path.value || fallbackPath.value)

        if (request === previewVersion) {
            preview.value = url
        }
    } catch {
        if (request === previewVersion) {
            previewFailed.value = true
        }
    }
}

function chooseImage(image: { path: string }) {
    path.value = image.path
    picking.value = false
    saved.value = false
}

watch(channelId, load, { immediate: true })
watch([channel, path, fallbackPath], loadPreview, { immediate: true })
</script>

<template>
    <section class="flex flex-col gap-4">
        <header>
            <h1 class="m-0 text-xl font-semibold">{{ t('graphics.sections.xmltv') }}</h1>
            <p class="mt-1 mb-0 text-muted">{{ t('settings.logo.guideLogoHint') }}</p>
        </header>
        <div class="card-surface p-4 flex flex-col sm:flex-row items-start gap-5">
            <div
                class="w-36 h-36 flex-none rounded-xl border border-line bg-base-300 flex items-center justify-center p-2"
            >
                <img
                    v-if="preview && !previewFailed"
                    :src="preview"
                    :alt="t('graphics.sections.xmltv')"
                    class="max-w-full max-h-full object-contain"
                    @error="previewFailed = true"
                />
                <i v-else class="bi bi-image text-3xl text-muted" aria-hidden="true" />
            </div>
            <div class="min-w-0 flex-1 w-full flex flex-col gap-3">
                <label class="field-label">
                    {{ t('settings.logo.guideLogo') }}
                    <input
                        v-model="path"
                        class="field-input"
                        :disabled="!ready || busy"
                        placeholder="Graphics/guide.png"
                    />
                </label>
                <p class="m-0 text-xs text-muted">
                    {{ t(path ? 'settings.logo.guideLogoOwn' : 'settings.logo.guideLogoSame') }}
                </p>
                <div class="flex flex-wrap gap-2">
                    <button class="btn-quiet" :disabled="!ready || busy" @click="picking = !picking">
                        {{ t('graphics.composer.pickImage') }}
                    </button>
                    <button class="btn-main" :disabled="!ready || busy || path === baseline" @click="save">
                        {{ t('graphics.composer.saveGuideLogo') }}
                    </button>
                </div>
                <p v-if="saved" role="status" class="m-0 text-sm">{{ t('graphics.composer.guideLogoSaved') }}</p>
                <p v-if="error" role="alert" class="m-0 text-sm text-red-500">{{ error }}</p>
                <button v-if="!ready && error" class="btn-quiet" @click="load">
                    {{ t('graphics.composer.retry') }}
                </button>
            </div>
        </div>
        <div v-if="picking" class="h-96">
            <StoragePicker class="h-full" hide-lives images-only @pick="chooseImage" />
        </div>
    </section>
</template>
