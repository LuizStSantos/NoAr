<script setup lang="ts">
import { computed, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { onClickOutside } from '@vueuse/core'

import { locales } from '@/i18n'
import { useAuth } from '@/stores/auth'
import { useIndex } from '@/stores/index'
import { useConfig } from '@/stores/config'
import { usePlaylist } from '@/stores/playlist'

const { t, locale } = useI18n()
const router = useRouter()
const route = useRoute()

const authStore = useAuth()
const configStore = useConfig()
const indexStore = useIndex()
const playlistStore = usePlaylist()

const menuOpen = ref(false)
const userMenu = ref<HTMLElement>()

onClickOutside(userMenu, () => (menuOpen.value = false))

const navItems = computed(() => [
    { name: 'home', label: t('nav.home'), path: '/' },
    { name: 'player', label: t('nav.player'), path: '/player' },
    { name: 'schedule', label: t('nav.schedule'), path: '/schedule' },
    { name: 'media', label: t('nav.media'), path: '/media' },
    { name: 'report', label: t('nav.report'), path: '/report' },
    { name: 'logging', label: t('nav.logging'), path: '/logging' },
    { name: 'configure', label: t('nav.configure'), path: '/configure' },
])

const channelId = computed(() => configStore.channels[configStore.i]?.id)

function isActive(path: string) {
    return path === '/' ? route.path === '/' : route.path.startsWith(path)
}

function channelLink(path: string) {
    return channelId.value ? { path, query: { channel: channelId.value } } : path
}

async function selectChannel(event: Event) {
    const id = Number((event.target as HTMLSelectElement).value)

    await router.push({ path: route.path, query: { ...route.query, channel: id } })
}

function changeLang(code: string) {
    locale.value = code
    localStorage.setItem('language', code)
}

function toggleTheme() {
    indexStore.darkMode = !indexStore.darkMode
    localStorage.setItem('theme', indexStore.darkMode ? 'dark' : 'light')
}

async function logout() {
    menuOpen.value = false
    await authStore.logout()
    await router.push({ name: 'login' })
}
</script>
<template>
    <header class="bg-base-100 border-b border-line">
        <div class="max-w-[1440px] mx-auto px-4 sm:px-6 min-h-14 py-2 flex flex-wrap items-center gap-x-5 gap-y-2">
            <RouterLink :to="channelLink('/')" class="flex items-center gap-2.5 font-semibold text-[16px]">
                <span class="w-7 h-7 rounded-[7px] bg-accent flex items-center justify-center" aria-hidden="true">
                    <svg width="12" height="12" viewBox="0 0 16 16" fill="#17181a">
                        <path d="M4 2.5v11l9.5-5.5z" />
                    </svg>
                </span>
                NoAr
            </RouterLink>

            <nav
                :aria-label="t('nav.main')"
                class="flex gap-1 min-w-0 order-last basis-full overflow-x-auto md:order-none md:basis-0 md:flex-1 md:flex-wrap md:overflow-visible"
            >
                <RouterLink
                    v-for="item in navItems"
                    :key="item.name"
                    :to="channelLink(item.path)"
                    :aria-current="isActive(item.path) ? 'page' : undefined"
                    class="px-3 py-2 rounded-field whitespace-nowrap"
                    :class="
                        isActive(item.path)
                            ? 'bg-base-300 text-base-content'
                            : 'text-muted hover:text-base-content hover:bg-raised'
                    "
                >
                    {{ item.label }}
                </RouterLink>
            </nav>

            <div class="flex items-center gap-2 ms-auto">
                <span
                    v-if="playlistStore.playoutIsRunning"
                    class="inline-flex items-center gap-2 bg-on-air-soft text-on-air-text rounded-full px-3 py-1 font-semibold text-xs tracking-wide"
                >
                    <span class="dot bg-[#ff5a5f]" aria-hidden="true" />{{ t('nav.onAir') }}
                </span>
                <span
                    v-else
                    class="inline-flex items-center gap-2 bg-raised text-muted rounded-full px-3 py-1 font-semibold text-xs tracking-wide"
                >
                    <span class="dot bg-subtle" aria-hidden="true" />{{ t('nav.stopped') }}
                </span>

                <label v-if="configStore.channels.length > 1" class="relative">
                    <span class="sr-only">{{ t('nav.channel') }}</span>
                    <select
                        class="appearance-none bg-raised text-base-content border border-line-strong rounded-field ps-3 pe-8 min-h-9 cursor-pointer"
                        :value="channelId"
                        @change="selectChannel"
                    >
                        <option v-for="channel in configStore.channels" :key="channel.id" :value="channel.id">
                            {{ channel.name }}
                        </option>
                    </select>
                    <i
                        class="bi bi-chevron-down absolute end-2.5 top-1/2 -translate-y-1/2 text-xs pointer-events-none"
                        aria-hidden="true"
                    />
                </label>
                <span
                    v-else-if="configStore.channels[0]"
                    class="bg-raised border border-line-strong rounded-field px-3 min-h-9 inline-flex items-center"
                >
                    {{ configStore.channels[0].name }}
                </span>

                <div ref="userMenu" class="relative">
                    <button
                        class="btn-icon border border-line-strong"
                        :aria-label="t('nav.userMenu')"
                        :aria-expanded="menuOpen"
                        @click="menuOpen = !menuOpen"
                    >
                        <i class="bi bi-person" aria-hidden="true" />
                    </button>
                    <div
                        v-if="menuOpen"
                        role="menu"
                        class="absolute end-0 mt-2 w-64 z-40 bg-raised border border-line-strong rounded-xl p-1.5 shadow-xl flex flex-col"
                    >
                        <div class="px-3 py-2 text-xs text-muted">
                            {{ configStore.configUser.username }}
                        </div>
                        <div class="px-3 pt-1 pb-1 text-xs text-subtle uppercase tracking-wider">
                            {{ t('nav.language') }}
                        </div>
                        <button
                            v-for="lang in locales"
                            :key="lang.code"
                            role="menuitemradio"
                            :aria-checked="locale === lang.code"
                            class="flex items-center gap-3 px-3 min-h-10 rounded-field text-left hover:bg-base-300 cursor-pointer"
                            @click="changeLang(lang.code)"
                        >
                            <i
                                class="bi bi-check2 w-4"
                                :class="locale === lang.code ? 'opacity-100' : 'opacity-0'"
                                aria-hidden="true"
                            />
                            {{ lang.name }}
                        </button>
                        <div class="h-px bg-line-strong my-1.5 mx-1" aria-hidden="true" />
                        <button
                            role="menuitem"
                            class="flex items-center gap-3 px-3 min-h-10 rounded-field text-left hover:bg-base-300 cursor-pointer"
                            @click="toggleTheme"
                        >
                            <i class="bi w-4" :class="indexStore.darkMode ? 'bi-sun' : 'bi-moon'" aria-hidden="true" />
                            {{ indexStore.darkMode ? t('nav.lightTheme') : t('nav.darkTheme') }}
                        </button>
                        <button
                            role="menuitem"
                            class="flex items-center gap-3 px-3 min-h-10 rounded-field text-left hover:bg-base-300 cursor-pointer"
                            @click="logout"
                        >
                            <i class="bi bi-box-arrow-right w-4" aria-hidden="true" />
                            {{ t('nav.logout') }}
                        </button>
                    </div>
                </div>
            </div>
        </div>
    </header>
</template>
