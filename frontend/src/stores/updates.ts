import { computed, ref } from 'vue'
import { defineStore } from 'pinia'

import { authFetch } from '@/composables/authFetch'
import { useAuth } from '@/stores/auth'

/** How often an open page asks the server for news. */
const REFRESH_MS = 30 * 60 * 1000
const DISMISSED_KEY = 'noar.update.dismissed'

/** The update check, for global admins: the settings page, the nav dot and the home notice. */
export const useUpdates = defineStore('updates', () => {
    const authStore = useAuth()

    const status = ref<UpdateStatus | null>(null)
    const checking = ref(false)
    const dismissed = ref(readDismissed())
    let timer: ReturnType<typeof setInterval> | undefined

    const allowed = computed(() => authStore.role === 'global_admin')
    const available = computed(() => allowed.value && !!status.value?.available)
    /** The home notice hides once dismissed, until another version comes out. */
    const showNotice = computed(() => available.value && dismissed.value !== status.value?.latest?.commit)

    async function load() {
        if (!allowed.value) {
            return
        }

        try {
            status.value = await authFetch<UpdateStatus>('/api/system/update')
        } catch {
            // The nav dot and the notice wait for the next try.
        }
    }

    /** Load now and keep the status fresh while the page stays open. */
    function watchStatus() {
        load()
        clearInterval(timer)
        timer = setInterval(load, REFRESH_MS)
    }

    async function check() {
        checking.value = true

        try {
            status.value = await authFetch<UpdateStatus>('/api/system/update/check', { method: 'POST' })
        } finally {
            checking.value = false
        }
    }

    /** Change some of the settings; the rest keep their value. */
    async function saveSettings(settings: { auto_check?: boolean; auto_update?: boolean; update_time?: string }) {
        status.value = await authFetch<UpdateStatus>('/api/system/update/settings', {
            method: 'PUT',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(settings),
        })
    }

    /** Ask the updater to install the new version now. */
    async function install() {
        status.value = await authFetch<UpdateStatus>('/api/system/update/install', { method: 'POST' })
    }

    /** Install at the next `time` (`HH:MM`) of the server clock. */
    async function schedule(time: string) {
        status.value = await authFetch<UpdateStatus>('/api/system/update/schedule', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ time }),
        })
    }

    async function cancelSchedule() {
        status.value = await authFetch<UpdateStatus>('/api/system/update/schedule', { method: 'DELETE' })
    }

    function dismiss() {
        const commit = status.value?.latest?.commit

        if (!commit) {
            return
        }

        dismissed.value = commit

        try {
            localStorage.setItem(DISMISSED_KEY, commit)
        } catch {
            // Hidden for this visit only.
        }
    }

    return {
        status,
        checking,
        available,
        showNotice,
        load,
        watchStatus,
        check,
        saveSettings,
        install,
        schedule,
        cancelSchedule,
        dismiss,
    }
})

function readDismissed(): string | null {
    try {
        return localStorage.getItem(DISMISSED_KEY)
    } catch {
        return null
    }
}
