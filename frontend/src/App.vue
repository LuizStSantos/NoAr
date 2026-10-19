<script setup lang="ts">
import { computed, ref, onBeforeMount } from 'vue'
import { RouterView, useRoute } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { useHead } from '@unhead/vue'

import { usePlayoutStatusStream } from '@/composables/playoutStatus'
import { locales } from '@/i18n'
import { useIndex } from '@/stores/index'

import AlertMsg from '@/components/utils/AlertMsg.vue'
import UploadTray from '@/components/utils/UploadTray.vue'
import { useUploads } from '@/stores/uploads'
import HeaderMenu from '@/components/HeaderMenu.vue'

const { locale } = useI18n()
const indexStore = useIndex()
const uploads = useUploads()
const route = useRoute()

// Closing or reloading the tab would stop the uploads: ask first.
window.addEventListener('beforeunload', (event) => {
    if (uploads.busy) {
        event.preventDefault()
        event.returnValue = ''
    }
})

usePlayoutStatusStream()

/** The saved language, or the first browser language NoAr has. */
function startLanguage(): string {
    let saved: string | null = null

    try {
        saved = localStorage.getItem('language')
    } catch {
        // Storage can be unavailable (private mode); use the browser's.
    }

    if (saved) {
        return saved
    }

    for (const language of navigator.languages ?? [navigator.language]) {
        const code = language.toLowerCase()
        const match =
            locales.find((item) => item.code === code) ??
            locales.find((item) => item.code.split('-')[0] === code.split('-')[0])

        if (match) {
            return match.code
        }
    }

    return 'en'
}

locale.value = startLanguage()

const darkThemeMq = window.matchMedia('(prefers-color-scheme: dark)')
const theme = ref(localStorage.getItem('theme'))

const preferDark = computed(() => {
    return theme.value === 'dark' || (!theme.value && darkThemeMq.matches)
})

onBeforeMount(() => {
    indexStore.darkMode = preferDark.value

    darkThemeMq.addEventListener('change', (e) => {
        indexStore.darkMode = e.matches
    })

    window.addEventListener('storage', (e) => {
        if (e.key === 'theme') {
            theme.value = e.newValue
            indexStore.darkMode = preferDark.value
        }
    })
})

useHead({
    htmlAttrs: {
        lang: computed(() => locale.value),
        'data-theme': computed(() => (indexStore.darkMode ? 'ffp-dark' : 'ffp-light')),
    },
})

const showHeader = computed(() => route.meta.showHeader === true)
</script>
<template>
    <div class="min-h-screen bg-base-200">
        <div v-if="showHeader" class="sticky top-0 z-30">
            <HeaderMenu />
        </div>

        <main>
            <RouterView />
        </main>

        <UploadTray v-if="showHeader" />
        <AlertMsg />
    </div>
</template>
