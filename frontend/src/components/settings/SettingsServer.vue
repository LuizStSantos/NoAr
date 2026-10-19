<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { cloneDeep } from 'es-toolkit/object'

import RestartDialog from '@/components/settings/RestartDialog.vue'
import SaveBar from '@/components/settings/SaveBar.vue'
import SectionCard from '@/components/settings/SectionCard.vue'
import { authFetch } from '@/composables/authFetch'
import { countChanges } from '@/composables/diff'
import { uptimeLabel } from '@/composables/player'
import { useAuth } from '@/stores/auth'
import { useConfig } from '@/stores/config'
import { useIndex } from '@/stores/index'

const { t } = useI18n()
const authStore = useAuth()
const configStore = useConfig()
const indexStore = useIndex()

const server = ref<GlobalSettings | null>(null)
const saved = ref<GlobalSettings | null>(null)
const smtpPassword = ref('')
const notificationToken = ref('')
const saving = ref(false)
const testing = ref(false)
const testRecipient = ref(configStore.configUser.mail ?? '')

const changes = computed(
    () => countChanges(server.value, saved.value) + (smtpPassword.value ? 1 : 0) + (notificationToken.value ? 1 : 0),
)

async function load() {
    try {
        server.value = await authFetch<GlobalSettings>('/api/global', { headers: authStore.authHeader })
        saved.value = cloneDeep(server.value)
        smtpPassword.value = ''
        notificationToken.value = ''
    } catch {
        indexStore.msgAlert('error', t('config.updateGlobalFailed'), 3)
    }
}

interface ServerInfo {
    started_at: number
    version: string
    channels: number
    channels_running: number
}

/** How long to wait for NoAr to come back after a restart. */
const RESTART_TIMEOUT_MS = 120_000

const info = ref<ServerInfo | null>(null)
const now = ref(Date.now())
const askingRestart = ref(false)
const restarting = ref<'waiting' | 'late' | null>(null)
let clock: ReturnType<typeof setInterval> | undefined
let poll: ReturnType<typeof setTimeout> | undefined

const serverLine = computed(() =>
    info.value
        ? t('settings.restart.serverInfo', {
              time: uptimeLabel(Math.max(60, now.value / 1000 - info.value.started_at)),
              running: info.value.channels_running,
              channels: info.value.channels,
          })
        : '',
)

async function loadInfo() {
    try {
        info.value = await authFetch<ServerInfo>('/api/system/server')
    } catch {
        info.value = null
    }
}

/** Ask NoAr to restart, then reload the page once a new run answers. */
async function restartServer() {
    const before = info.value?.started_at ?? 0
    const deadline = Date.now() + RESTART_TIMEOUT_MS

    askingRestart.value = false
    restarting.value = 'waiting'

    try {
        await authFetch('/api/system/restart', { method: 'POST' })
    } catch (e: any) {
        restarting.value = null
        indexStore.msgAlert('error', e?.data ?? e?.message ?? String(e), 5)
        return
    }

    const check = async () => {
        try {
            const response = await fetch('/api/system/server', { headers: authStore.authHeader })

            if (response.ok && (await response.json()).started_at !== before) {
                window.location.reload()
                return
            }
        } catch {
            // Not back yet.
        }

        if (Date.now() > deadline) {
            restarting.value = 'late'
            return
        }

        poll = setTimeout(check, 1500)
    }

    poll = setTimeout(check, 1500)
}

onMounted(() => {
    load()
    loadInfo()
    clock = setInterval(() => (now.value = Date.now()), 30_000)
})

onBeforeUnmount(() => {
    clearInterval(clock)
    clearTimeout(poll)
})

async function save() {
    if (!server.value) {
        return
    }

    saving.value = true

    try {
        server.value = await authFetch<GlobalSettings>('/api/global', {
            method: 'PUT',
            headers: { ...configStore.contentType, ...authStore.authHeader },
            body: JSON.stringify({
                smtp_server: server.value.smtp_server.trim(),
                smtp_user: server.value.smtp_user.trim(),
                smtp_password: smtpPassword.value,
                smtp_starttls: server.value.smtp_starttls,
                smtp_port: Number(server.value.smtp_port),
                notification_server: server.value.notification_server.trim(),
                notification_token: notificationToken.value,
            }),
        })
        saved.value = cloneDeep(server.value)
        smtpPassword.value = ''
        notificationToken.value = ''
        await configStore.getPlayoutConfig()
        indexStore.msgAlert('success', t('settings.saved'), 2)
    } catch (e: any) {
        indexStore.msgAlert('error', e?.data ?? t('config.updateGlobalFailed'), 4)
    } finally {
        saving.value = false
    }
}

function discard() {
    server.value = cloneDeep(saved.value)
    smtpPassword.value = ''
    notificationToken.value = ''
}

async function sendTest() {
    testing.value = true

    try {
        await authFetch('/api/global/test-mail', {
            method: 'POST',
            headers: { ...configStore.contentType, ...authStore.authHeader },
            body: JSON.stringify({ recipient: testRecipient.value.trim() }),
            responseType: 'text',
        })
        indexStore.msgAlert('success', t('settings.server.testSent', { mail: testRecipient.value }), 3)
    } catch (e: any) {
        indexStore.msgAlert('error', e?.data ?? String(e), 6)
    } finally {
        testing.value = false
    }
}
</script>
<template>
    <div>
        <h1 class="m-0 text-xl font-semibold">{{ t('settings.sections.server') }}</h1>
        <p class="mt-1 mb-0 text-muted">{{ t('settings.server.intro') }}</p>
    </div>

    <section
        :aria-labelledby="'server-restart'"
        class="card-box px-5 py-4.5 flex flex-wrap items-center gap-x-5 gap-y-3.5"
    >
        <div class="flex-[1_1_360px] flex flex-col gap-1">
            <h2 id="server-restart" class="m-0 text-base font-semibold">NoAr</h2>
            <span v-if="serverLine" class="text-base-content/85">{{ serverLine }}</span>
            <span class="text-[13px] text-muted">{{ t('settings.restart.serverHint') }}</span>
        </div>
        <button
            type="button"
            class="btn-quiet text-on-air-text border-on-air/40 hover:bg-on-air-soft!"
            :disabled="restarting !== null"
            @click="askingRestart = true"
        >
            <i class="bi bi-power" aria-hidden="true" />{{ t('settings.restart.server') }}
        </button>
    </section>

    <RestartDialog
        :show="askingRestart"
        :title="t('settings.restart.serverTitle')"
        :text="t('settings.restart.serverText')"
        :hint="t('settings.restart.serverConfirmHint')"
        @close="askingRestart = false"
        @confirm="restartServer"
    />

    <div
        v-if="restarting"
        class="z-50 fixed inset-0 flex items-center justify-center bg-black/70 px-4"
        role="status"
        aria-live="polite"
    >
        <div
            class="w-full max-w-[420px] bg-base-100 border border-line-strong rounded-[16px] px-6 py-6 flex flex-col items-center gap-3 text-center"
        >
            <template v-if="restarting === 'waiting'">
                <span class="loading loading-spinner loading-lg text-accent" />
                <h2 class="m-0 text-lg font-semibold">{{ t('settings.restart.restarting') }}</h2>
                <p class="m-0 text-muted">{{ t('settings.restart.restartingHint') }}</p>
            </template>
            <template v-else>
                <i class="bi bi-exclamation-circle text-3xl text-on-air-text" aria-hidden="true" />
                <h2 class="m-0 text-lg font-semibold">{{ t('settings.restart.late') }}</h2>
                <p class="m-0 text-muted">{{ t('settings.restart.lateHint') }}</p>
                <button type="button" class="btn-quiet" @click="restarting = null">{{ t('storage.close') }}</button>
            </template>
        </div>
    </div>

    <template v-if="server">
        <SectionCard :title="t('settings.server.smtp')" :description="t('settings.server.smtpHint')">
            <div class="grid grid-cols-1 md:grid-cols-[minmax(0,3fr)_minmax(0,1fr)] gap-3.5">
                <label class="field-label">
                    {{ t('settings.server.host') }}
                    <input
                        v-model="server.smtp_server"
                        class="field-input font-mono text-[13px]"
                        placeholder="smtp.example.com"
                        spellcheck="false"
                    />
                </label>
                <label class="field-label">
                    {{ t('settings.server.port') }}
                    <input
                        v-model.number="server.smtp_port"
                        type="number"
                        min="1"
                        max="65535"
                        class="field-input font-mono"
                    />
                </label>
                <label class="field-label">
                    {{ t('settings.server.user') }}
                    <input v-model="server.smtp_user" class="field-input" autocomplete="off" />
                </label>
                <label class="field-label">
                    {{ t('settings.server.password') }}
                    <input
                        v-model="smtpPassword"
                        type="password"
                        class="field-input"
                        autocomplete="new-password"
                        :placeholder="server.smtp_password_set ? t('settings.server.passwordSet') : ''"
                    />
                </label>
            </div>
            <div class="flex flex-wrap items-center gap-3">
                <label class="flex items-center gap-3 min-h-10 cursor-pointer">
                    <input v-model="server.smtp_starttls" type="checkbox" role="switch" class="toggle toggle-accent" />
                    {{ t('settings.server.starttls') }}
                </label>
                <div class="flex-auto" />
                <input
                    v-model="testRecipient"
                    type="email"
                    class="field-input w-auto min-w-56 flex-[0_1_260px]"
                    :aria-label="t('settings.server.testRecipient')"
                    :placeholder="t('settings.server.testRecipient')"
                />
                <button
                    class="btn-quiet"
                    :disabled="testing || changes > 0 || !testRecipient.includes('@') || !server.smtp_password_set"
                    :title="changes > 0 ? t('settings.server.saveFirst') : ''"
                    @click="sendTest"
                >
                    <span v-if="testing" class="loading loading-spinner loading-xs" />{{ t('settings.server.test') }}
                </button>
            </div>
        </SectionCard>

        <SectionCard :title="t('settings.server.push')" :description="t('settings.server.pushHint')">
            <div class="grid grid-cols-1 md:grid-cols-2 gap-3.5">
                <label class="field-label">
                    {{ t('settings.server.pushUrl') }}
                    <input
                        v-model="server.notification_server"
                        class="field-input font-mono text-[13px]"
                        placeholder="https://ntfy.sh"
                        spellcheck="false"
                    />
                </label>
                <label class="field-label">
                    {{ t('settings.server.token') }}
                    <input
                        v-model="notificationToken"
                        type="password"
                        class="field-input"
                        autocomplete="new-password"
                        :placeholder="server.notification_token_set ? t('settings.server.tokenSet') : ''"
                    />
                </label>
            </div>
        </SectionCard>

        <SectionCard :title="t('settings.server.logs')">
            <p class="m-0 text-muted">{{ t('settings.server.logsHint') }}</p>
        </SectionCard>

        <SaveBar :changes="changes" :saving="saving" @save="save" @discard="discard" />
    </template>
</template>
