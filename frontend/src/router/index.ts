import { createRouter, createWebHistory } from 'vue-router'
// import { i18n } from '../i18n'
import HomeView from '@/views/HomeView.vue'
import LoginView from '@/views/LoginView.vue'

import { useAuth } from '@/stores/auth'
import { useConfig } from '@/stores/config'

const ALL = ['global_admin', 'channel_admin', 'user']
const ADMINS = ['global_admin', 'channel_admin']

const router = createRouter({
    history: createWebHistory(import.meta.env.BASE_URL),
    // A new page starts at the top, so a short page does not open half
    // scrolled after a long one. Back and forward restore the old position,
    // and query-only changes (like switching the channel) keep it.
    scrollBehavior(to, from, savedPosition) {
        if (savedPosition) {
            return savedPosition
        }

        if (to.path === from.path) {
            return false
        }

        return { top: 0 }
    },
    routes: [
        {
            path: '/login',
            name: 'login',
            component: LoginView,
            meta: { public: true, showHeader: false },
        },
        {
            path: '/',
            name: 'home',
            component: HomeView,
            meta: { showHeader: true },
        },
        {
            path: '/verification',
            name: 'verification',
            component: () => import('@/views/VerificationView.vue'),
            meta: { public: true, showHeader: false },
        },
        {
            path: '/init',
            name: 'init',
            component: () => import('@/views/InitView.vue'),
            meta: { public: true, showHeader: false },
        },
        {
            path: '/player',
            name: 'player',
            component: () => import('@/views/PlayerView.vue'),
            meta: { showHeader: true },
        },
        {
            path: '/playlist',
            name: 'playlist',
            component: () => import('@/views/PlayerView.vue'),
            props: { build: true },
            meta: { showHeader: true },
        },
        {
            path: '/graphics',
            name: 'graphics',
            component: () => import('@/views/GraphicsView.vue'),
            meta: { showHeader: true },
            redirect: (to) => ({ path: '/graphics/live', query: to.query }),
            children: [
                {
                    path: 'live',
                    name: 'graphics-live',
                    component: () => import('@/components/graphics/GraphicsLive.vue'),
                    meta: { showHeader: true, roles: ALL },
                },
                {
                    path: 'logo',
                    name: 'graphics-logo',
                    component: () => import('@/components/settings/SettingsLogo.vue'),
                    props: { part: 'logo' },
                    meta: { showHeader: true, roles: ADMINS },
                },
                {
                    path: 'lshape',
                    name: 'graphics-lshape',
                    component: () => import('@/components/settings/SettingsLogo.vue'),
                    props: { part: 'lshape' },
                    meta: { showHeader: true, roles: ADMINS },
                },
                {
                    path: 'subtitles',
                    name: 'graphics-subtitles',
                    component: () => import('@/components/settings/SettingsLogo.vue'),
                    props: { part: 'subtitles' },
                    meta: { showHeader: true, roles: ADMINS },
                },
                {
                    path: 'scenes',
                    name: 'graphics-scenes',
                    component: () => import('@/components/graphics/GraphicsScenes.vue'),
                    meta: { showHeader: true, roles: ADMINS },
                },
                {
                    path: 'api',
                    name: 'graphics-api',
                    component: () => import('@/components/graphics/GraphicsApi.vue'),
                    meta: { showHeader: true, roles: ADMINS },
                },
            ],
        },
        {
            path: '/schedule',
            name: 'schedule',
            component: () => import('@/views/ScheduleView.vue'),
            meta: { showHeader: true },
        },
        {
            path: '/media',
            name: 'media',
            component: () => import('@/views/MediaView.vue'),
            meta: { showHeader: true },
        },
        {
            path: '/report',
            name: 'report',
            component: () => import('@/views/ReportView.vue'),
            meta: { showHeader: true },
        },
        {
            path: '/logging',
            name: 'logging',
            component: () => import('@/views/LoggingView.vue'),
            meta: { showHeader: true },
        },
        {
            path: '/configure',
            name: 'configure',
            component: () => import('@/views/ConfigureView.vue'),
            meta: { showHeader: true },
            redirect: (to) => ({ path: '/configure/general', query: to.query }),
            children: [
                {
                    path: 'general',
                    name: 'settings-general',
                    component: () => import('@/components/settings/SettingsGeneral.vue'),
                    meta: { showHeader: true, roles: ALL },
                },
                {
                    path: 'output',
                    name: 'settings-output',
                    component: () => import('@/components/settings/SettingsOutput.vue'),
                    meta: { showHeader: true, roles: ADMINS },
                },
                {
                    path: 'ingest',
                    name: 'settings-ingest',
                    component: () => import('@/components/settings/SettingsIngest.vue'),
                    meta: { showHeader: true, roles: ADMINS },
                },
                {
                    path: 'playlist',
                    name: 'settings-playlist',
                    component: () => import('@/components/settings/SettingsPlaylist.vue'),
                    meta: { showHeader: true, roles: ADMINS },
                },
                {
                    // Moved to its own tab; old links still land there.
                    path: 'logo',
                    name: 'settings-logo',
                    redirect: (to) => ({ path: '/graphics/logo', query: to.query }),
                },
                {
                    path: 'audio',
                    name: 'settings-audio',
                    component: () => import('@/components/settings/SettingsAudio.vue'),
                    meta: { showHeader: true, roles: ADMINS },
                },
                {
                    path: 'recording',
                    name: 'settings-recording',
                    component: () => import('@/components/settings/SettingsRecording.vue'),
                    meta: { showHeader: true, roles: ADMINS },
                },
                {
                    path: 'breaks',
                    name: 'settings-breaks',
                    component: () => import('@/components/settings/SettingsBreaks.vue'),
                    meta: { showHeader: true, roles: ADMINS },
                },
                {
                    path: 'alerts',
                    name: 'settings-alerts',
                    component: () => import('@/components/settings/SettingsAlerts.vue'),
                    meta: { showHeader: true, roles: ADMINS },
                },
                {
                    path: 'advanced',
                    name: 'settings-advanced',
                    component: () => import('@/components/settings/SettingsAdvanced.vue'),
                    meta: { showHeader: true, roles: ADMINS },
                },
                {
                    path: 'users',
                    name: 'settings-users',
                    component: () => import('@/components/settings/SettingsUsers.vue'),
                    meta: { showHeader: true, roles: ALL },
                },
                {
                    path: 'server',
                    name: 'settings-server',
                    component: () => import('@/components/settings/SettingsServer.vue'),
                    meta: { showHeader: true, roles: ['global_admin'] },
                },
                {
                    path: 'updates',
                    name: 'settings-updates',
                    component: () => import('@/components/settings/SettingsUpdates.vue'),
                    meta: { showHeader: true, roles: ['global_admin'] },
                },
            ],
        },
    ],
})

router.beforeEach(async (to) => {
    const auth = useAuth()
    const configStore = useConfig()

    if (to.name === 'home') {
        const setupResponse = await fetch('/api/setup').catch(() => undefined)
        if (setupResponse?.ok && (await setupResponse.json()).required) {
            return { name: 'init' }
        }
    }

    await auth.inspectToken()

    const isVerificationRoute = to.name === 'verification'
    const isPublicRoute = to.meta.public === true

    if (isVerificationRoute) {
        if (auth.isLogin) {
            return { name: 'home' }
        }
        if (!auth.verificationPending) {
            return { name: 'login' }
        }
        return
    }

    if (!auth.isLogin && !isPublicRoute) {
        // const loc = i18n.locale.value === 'en-US' ? '' : `${i18n.locale.value}/`
        return { name: 'login' }
    }

    if (auth.isLogin && to.name === 'login') {
        return { name: 'home' }
    }

    if (!isPublicRoute) {
        const channelQuery = Array.isArray(to.query.channel) ? to.query.channel[0] : to.query.channel
        const channelId = channelQuery ? Number(channelQuery) : undefined
        await configStore.configInit(Number.isSafeInteger(channelId) ? channelId : undefined)
    }

    const allowedRoles = to.meta.roles as string[] | undefined

    if (allowedRoles && !allowedRoles.includes(auth.role)) {
        return { name: to.path.startsWith('/graphics') ? 'graphics-live' : 'settings-general', query: to.query }
    }

    return
})

export default router
