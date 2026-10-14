<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import SaveBar from '@/components/settings/SaveBar.vue'
import SectionCard from '@/components/settings/SectionCard.vue'
import UnitInput from '@/components/settings/UnitInput.vue'
import { authFetch } from '@/composables/authFetch'
import { usePlayoutPage } from '@/composables/settingsPage'
import { useAuth } from '@/stores/auth'
import { useIndex } from '@/stores/index'

const { t } = useI18n()
const authStore = useAuth()
const indexStore = useIndex()
const { ready, changes, alsoSaves, saveLabel, save, discard, settings, configStore } = usePlayoutPage('alerts')

const MAIL_LEVELS = ['INFO', 'WARNING', 'ERROR']
const PUSH_LEVELS = ['INFO', 'WARNING', 'ERROR', 'FATAL']

const testing = ref<'mail' | 'push' | null>(null)

const mail = computed(() => configStore.playout.mail)
const notification = computed(() => configStore.playout.notification)
const interval = computed({
    get: () => Number(mail.value.interval),
    set: (value: number) => ((mail.value as unknown as { interval: number }).interval = Number(value)),
})
const channelName = computed(() => configStore.channels[configStore.i]?.name ?? '')
const serverLink = computed(() => ({
    path: '/configure/server',
    query: { channel: configStore.channels[configStore.i]?.id },
}))

async function sendTest(kind: 'mail' | 'push') {
    testing.value = kind

    const body =
        kind === 'mail'
            ? { kind, recipient: mail.value.recipient, subject: mail.value.subject }
            : { kind, topic: notification.value.topic, tags: notification.value.tags }

    try {
        await authFetch(`/api/playout/config/${configStore.channels[configStore.i]?.id}/test-alert`, {
            method: 'POST',
            headers: { ...configStore.contentType, ...authStore.authHeader },
            body: JSON.stringify(body),
            responseType: 'text',
        })
        indexStore.msgAlert(
            'success',
            kind === 'mail' ? t('settings.alerts.mailSent') : t('settings.alerts.pushSent'),
            3,
        )
    } catch (e: any) {
        indexStore.msgAlert('error', e?.data ?? String(e), 6)
    } finally {
        testing.value = null
    }
}
</script>
<template>
    <template v-if="ready">
        <div>
            <h1 class="m-0 text-xl font-semibold">{{ t('settings.sections.alerts') }}</h1>
            <p class="mt-1 mb-0 text-muted">{{ t('settings.alerts.intro', { name: channelName }) }}</p>
        </div>

        <SectionCard :title="t('settings.alerts.mail')">
            <template #aside>
                <span v-if="mail.show" class="inline-flex items-center gap-1.5 text-[13px] text-ok">
                    <i class="bi bi-check-circle" aria-hidden="true" />{{ t('settings.alerts.mailReady') }}
                </span>
            </template>
            <template v-if="mail.show">
                <div class="grid grid-cols-1 md:grid-cols-2 gap-3.5">
                    <label class="field-label">
                        {{ t('settings.alerts.recipient') }}
                        <input v-model.trim="mail.recipient" class="field-input" type="text" inputmode="email" />
                        <span class="field-hint">{{ t('settings.alerts.recipientHint') }}</span>
                    </label>
                    <label class="field-label">
                        {{ t('settings.alerts.subject') }}
                        <input v-model="mail.subject" class="field-input" />
                    </label>
                    <label class="field-label">
                        {{ t('settings.alerts.level') }}
                        <select v-model="mail.mail_level" class="field-input">
                            <option v-for="level in MAIL_LEVELS" :key="level" :value="level">
                                {{ t(`settings.levels.${level}`) }}
                            </option>
                        </select>
                    </label>
                    <label class="field-label">
                        {{ t('settings.alerts.interval') }}
                        <UnitInput v-model.number="interval" unit="s" type="number" min="30" step="10" />
                        <span class="field-hint">{{ t('settings.alerts.intervalHint') }}</span>
                    </label>
                </div>
                <button
                    class="btn-quiet self-start"
                    :disabled="testing !== null || !mail.recipient"
                    @click="sendTest('mail')"
                >
                    <span v-if="testing === 'mail'" class="loading loading-spinner loading-xs" />{{
                        t('settings.alerts.testMail')
                    }}
                </button>
            </template>
            <p v-else class="m-0 text-muted">
                {{ t('settings.alerts.mailMissing') }}
                <RouterLink v-if="authStore.role === 'global_admin'" :to="serverLink" class="link-accent">{{
                    t('settings.sections.server')
                }}</RouterLink>
            </p>
        </SectionCard>

        <SectionCard :title="t('settings.alerts.push')" :description="t('settings.alerts.pushHint')">
            <template #aside>
                <span v-if="authStore.role === 'global_admin'" class="text-[13px] text-muted">
                    {{ t('settings.alerts.serverAt') }}
                    <RouterLink :to="serverLink" class="link-accent">{{ t('settings.sections.server') }}</RouterLink>
                </span>
            </template>
            <template v-if="notification.show">
                <div class="grid grid-cols-1 md:grid-cols-3 gap-3.5">
                    <label class="field-label">
                        {{ t('settings.alerts.topic') }}
                        <input v-model.trim="notification.topic" class="field-input font-mono" spellcheck="false" />
                    </label>
                    <label class="field-label">
                        {{ t('settings.alerts.level') }}
                        <select v-model="notification.level" class="field-input">
                            <option v-for="level in PUSH_LEVELS" :key="level" :value="level">
                                {{ t(`settings.levels.${level}`) }}
                            </option>
                        </select>
                    </label>
                    <label class="field-label">
                        {{ t('settings.alerts.tags') }}
                        <input v-model.trim="notification.tags" class="field-input" />
                    </label>
                </div>
                <button
                    class="btn-quiet self-start"
                    :disabled="testing !== null || !notification.topic"
                    @click="sendTest('push')"
                >
                    <span v-if="testing === 'push'" class="loading loading-spinner loading-xs" />{{
                        t('settings.alerts.testPush')
                    }}
                </button>
            </template>
            <p v-else class="m-0 text-muted">{{ t('settings.alerts.pushMissing') }}</p>
        </SectionCard>

        <SaveBar
            :changes="changes"
            :also-saves="alsoSaves"
            :saving="settings.saving"
            :save-label="saveLabel"
            @save="save"
            @discard="discard"
        />
    </template>
</template>
