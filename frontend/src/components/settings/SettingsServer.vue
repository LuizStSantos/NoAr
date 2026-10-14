<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { cloneDeep } from 'es-toolkit/object'

import SaveBar from '@/components/settings/SaveBar.vue'
import SectionCard from '@/components/settings/SectionCard.vue'
import { authFetch } from '@/composables/authFetch'
import { countChanges } from '@/composables/diff'
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

onMounted(load)

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
            <label class="field-label max-w-2xl">
                {{ t('settings.server.logsFolder') }}
                <input :value="server.logs" class="field-input font-mono text-[13px]" readonly />
                <span class="field-hint">{{ t('settings.server.logsHint') }}</span>
            </label>
        </SectionCard>

        <SaveBar :changes="changes" :saving="saving" @save="save" @discard="discard" />
    </template>
</template>
