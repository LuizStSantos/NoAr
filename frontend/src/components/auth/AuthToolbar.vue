<script setup lang="ts">
import { useI18n } from 'vue-i18n'

import { locales } from '@/i18n'
import { useIndex } from '@/stores/index'

/** Language and theme switches of the pages shown before signing in. */
const { t, locale } = useI18n()
const indexStore = useIndex()

function changeLang(event: Event) {
    const code = (event.target as HTMLSelectElement).value

    locale.value = code

    try {
        localStorage.setItem('language', code)
    } catch {
        // The choice only lasts for this visit.
    }
}

function toggleTheme() {
    indexStore.darkMode = !indexStore.darkMode

    try {
        localStorage.setItem('theme', indexStore.darkMode ? 'dark' : 'light')
    } catch {
        // The choice only lasts for this visit.
    }
}
</script>
<template>
    <div class="flex items-center gap-2">
        <label class="inline-flex items-center gap-2 text-[13px] text-muted">
            <i class="bi bi-globe2" aria-hidden="true" />
            <select
                :aria-label="t('nav.language')"
                :value="locale"
                class="bg-base-100 text-base-content border border-line-strong rounded-lg px-2.5 min-h-9.5 text-[13px] cursor-pointer"
                @change="changeLang"
            >
                <option v-for="lang in locales" :key="lang.code" :value="lang.code">{{ lang.name }}</option>
            </select>
        </label>
        <button
            type="button"
            class="btn-icon min-w-9.5 min-h-9.5 bg-base-100 border border-line-strong text-base-content/80"
            :aria-label="indexStore.darkMode ? t('nav.lightTheme') : t('nav.darkTheme')"
            @click="toggleTheme"
        >
            <i class="bi" :class="indexStore.darkMode ? 'bi-sun' : 'bi-moon'" aria-hidden="true" />
        </button>
    </div>
</template>
