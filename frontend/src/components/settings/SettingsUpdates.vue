<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import SectionCard from '@/components/settings/SectionCard.vue'
import ToggleRow from '@/components/settings/ToggleRow.vue'
import UpdateProgress from '@/components/settings/UpdateProgress.vue'
import GenericModal from '@/components/utils/GenericModal.vue'
import { authFetch } from '@/composables/authFetch'
import { copyText } from '@/composables/browser'
import { useIndex } from '@/stores/index'
import { useUpdates } from '@/stores/updates'

const { t, locale } = useI18n()
const indexStore = useIndex()
const updates = useUpdates()

/** What updates the image on the server, the way the README says. */
const SERVER_COMMAND = 'docker compose pull && docker compose up -d'

const saving = ref(false)
const scheduleTime = ref('03:00')
const autoTime = ref('03:00')
const confirming = ref(false)
/** `started_at` of this run, while the updater installs the new version. */
const installingFrom = ref<number | null>(null)

const status = computed(() => updates.status)
const latest = computed(() => status.value?.latest ?? null)
const updater = computed(() => status.value?.updater ?? null)
/** Changes left out of the list, which shows 30 at most. */
const moreChanges = computed(() => Math.max(0, (latest.value?.change_count ?? 0) - (latest.value?.changes.length ?? 0)))

const setupCommands = computed(() =>
    [
        `curl -fsSLO https://raw.githubusercontent.com/${status.value?.repository ?? 'LuizStSantos/NoAr'}/main/docker-compose.yml`,
        'echo "COMPOSE_PROFILES=updater" >> .env',
        'echo "NOAR_UPDATER_TOKEN=$(openssl rand -hex 24)" >> .env',
        'docker compose up -d',
    ].join('\n'),
)

const autoCheck = computed({
    get: () => status.value?.auto_check ?? true,
    set: (value: boolean) => save({ auto_check: value }),
})

const autoUpdate = computed({
    get: () => status.value?.auto_update ?? false,
    set: (value: boolean) => save({ auto_update: value, update_time: autoTime.value }),
})

watch(
    () => status.value?.update_time,
    (time) => {
        if (time) {
            autoTime.value = time
            scheduleTime.value = time
        }
    },
    { immediate: true },
)

onMounted(() => updates.load())

function short(commit: string | null | undefined): string {
    return commit ? commit.slice(0, 7) : ''
}

/** "21/10/2026 às 14:02", or "hoje às 14:02". */
function when(millis: number | null | undefined): string {
    if (!millis) {
        return ''
    }

    const date = new Date(millis)
    const time = date.toLocaleTimeString(locale.value, { hour: '2-digit', minute: '2-digit' })

    if (isToday(date)) {
        return t('settings.updates.today', { time })
    }

    return t('settings.updates.dateTime', {
        date: date.toLocaleDateString(locale.value, { day: '2-digit', month: '2-digit', year: 'numeric' }),
        time,
    })
}

function isToday(date: Date): boolean {
    return date.toDateString() === new Date().toDateString()
}

function published(millis: number): string {
    return isToday(new Date(millis))
        ? t('settings.updates.publishedToday', { when: when(millis) })
        : t('settings.updates.publishedAt', { when: when(millis) })
}

function failed(e: any) {
    indexStore.msgAlert('error', e?.message ?? String(e), 6)
}

async function check() {
    try {
        await updates.check()

        if (!status.value?.error) {
            indexStore.msgAlert(
                'success',
                status.value?.available ? t('settings.updates.foundNew') : t('settings.updates.upToDate'),
                3,
            )
        }
    } catch (e: any) {
        failed(e)
    }
}

async function save(settings: { auto_check?: boolean; auto_update?: boolean; update_time?: string }) {
    saving.value = true

    try {
        await updates.saveSettings(settings)
    } catch (e: any) {
        failed(e)
    } finally {
        saving.value = false
    }
}

function saveAutoTime() {
    if (autoTime.value && autoTime.value !== status.value?.update_time) {
        save({ update_time: autoTime.value })
    }
}

async function schedule() {
    try {
        await updates.schedule(scheduleTime.value)
        indexStore.msgAlert('success', t('settings.updates.scheduled', { when: when(status.value?.scheduled_at) }), 4)
    } catch (e: any) {
        failed(e)
    }
}

async function cancelSchedule() {
    try {
        await updates.cancelSchedule()
    } catch (e: any) {
        failed(e)
    }
}

/** Ask the updater to install now, then follow NoAr until it is back. */
async function install(confirmed: boolean) {
    confirming.value = false

    if (!confirmed) {
        return
    }

    try {
        const { started_at } = await authFetch<{ started_at: number }>('/api/system/server')

        await updates.install()
        installingFrom.value = started_at
    } catch (e: any) {
        failed(e)
    }
}

async function copy(text: string) {
    try {
        await copyText(text)
        indexStore.msgAlert('success', t('settings.updates.copied'), 2)
    } catch {
        indexStore.msgAlert('error', t('settings.updates.copyFailed'), 3)
    }
}
</script>
<template>
    <div>
        <h1 class="m-0 text-xl font-semibold">{{ t('settings.sections.updates') }}</h1>
        <p class="mt-1 mb-0 text-muted">{{ t('settings.updates.intro') }}</p>
    </div>

    <p v-if="!status" class="m-0 text-muted">
        <span class="loading loading-spinner loading-xs me-2" aria-hidden="true" />{{ t('settings.updates.loading') }}
    </p>
    <template v-else>
        <section
            v-if="status?.available && latest"
            aria-labelledby="update-new"
            class="card-box border-accent/60! px-5 py-4.5 flex flex-col gap-3.5"
        >
            <div class="flex flex-wrap items-start gap-x-5 gap-y-3">
                <div class="flex-[1_1_360px] min-w-0 flex flex-col gap-0.5">
                    <span class="text-xs font-semibold tracking-[0.06em] uppercase text-accent-text">
                        {{ t('settings.updates.newKicker') }}
                    </span>
                    <h2 id="update-new" class="m-0 text-xl font-semibold">
                        {{ latest.published_at ? published(latest.published_at) : t('settings.updates.newVersion') }}
                    </h2>
                    <span class="text-muted">
                        <template v-if="latest.change_count !== null">
                            {{ t('settings.updates.changes', { count: latest.change_count }, latest.change_count) }} ·
                        </template>
                        <template v-else>{{ t('settings.updates.changesUnknown') }} · </template>
                        <span v-if="latest.version" class="font-mono">{{ status.version }} → {{ latest.version }}</span>
                        <span v-else class="font-mono"
                            >{{ short(status.commit) || '?' }} → {{ short(latest.commit) }}</span
                        >
                    </span>
                </div>
                <a :href="latest.url" target="_blank" rel="noopener" class="link-accent text-[13px] pt-1">
                    {{ t('settings.updates.github')
                    }}<i class="bi bi-box-arrow-up-right ms-1.5 text-xs" aria-hidden="true" />
                </a>
            </div>

            <ul v-if="latest.changes.length" class="m-0 p-0 list-none flex flex-col gap-2 border-t border-line pt-3">
                <li v-for="change in latest.changes" :key="change.commit" class="flex items-start gap-2.5">
                    <span class="w-1.5 h-1.5 rounded-full bg-subtle mt-2 flex-none" aria-hidden="true" />
                    <span class="min-w-0">{{ change.title }}</span>
                </li>
                <li v-if="moreChanges" class="text-muted text-[13px] ps-4">
                    {{ t('settings.updates.moreChanges', { count: moreChanges }, moreChanges) }}
                </li>
            </ul>

            <div class="flex items-start gap-2.5 bg-raised rounded-[10px] px-3 py-2.5 text-[13px] text-base-content/85">
                <i class="bi bi-exclamation-triangle text-accent-text mt-px" aria-hidden="true" />
                <span>{{ t('settings.updates.downtime') }}</span>
            </div>

            <template v-if="updater?.connected">
                <div v-if="status.scheduled_at" class="flex flex-wrap items-center gap-x-3 gap-y-2">
                    <i class="bi bi-clock text-accent-text" aria-hidden="true" />
                    <span class="flex-[1_1_260px]">
                        {{ t('settings.updates.scheduledFor', { when: when(status.scheduled_at) }) }}
                    </span>
                    <button type="button" class="btn-quiet" @click="cancelSchedule">
                        {{ t('settings.updates.cancelSchedule') }}
                    </button>
                </div>
                <div v-else class="flex flex-wrap items-center gap-x-3 gap-y-2.5">
                    <button type="button" class="btn-main" @click="confirming = true">
                        <i class="bi bi-download" aria-hidden="true" />{{ t('settings.updates.installNow') }}
                    </button>
                    <span class="text-muted">{{ t('settings.updates.or') }}</span>
                    <label class="inline-flex items-center gap-2 text-base-content/85">
                        {{ t('settings.updates.at') }}
                        <input
                            v-model="scheduleTime"
                            type="time"
                            class="field-input font-mono w-28"
                            :aria-label="t('settings.updates.scheduleTime')"
                        />
                    </label>
                    <button type="button" class="btn-quiet" :disabled="!scheduleTime" @click="schedule">
                        {{ t('settings.updates.schedule') }}
                    </button>
                </div>
            </template>
            <div v-else class="flex flex-col gap-2 border-t border-line pt-3">
                <span>{{ t('settings.updates.howNow') }}</span>
                <div class="flex flex-wrap items-center gap-2">
                    <code
                        class="flex-[1_1_320px] min-w-0 font-mono text-[13px] bg-base-200 border border-line-strong rounded-field px-3 py-2 break-all"
                    >
                        {{ SERVER_COMMAND }}
                    </code>
                    <button type="button" class="btn-quiet min-h-10" @click="copy(SERVER_COMMAND)">
                        <i class="bi bi-clipboard" aria-hidden="true" />{{ t('settings.updates.copy') }}
                    </button>
                </div>
                <span class="text-[13px] text-muted">{{ t('settings.updates.withUpdater') }}</span>
            </div>
        </section>

        <section
            aria-labelledby="update-installed"
            class="card-box px-5 py-4.5 flex flex-wrap items-center gap-x-5 gap-y-3"
        >
            <div class="flex-[1_1_360px] min-w-0 flex flex-col gap-0.5">
                <h2 id="update-installed" class="m-0 text-base font-semibold">{{ t('settings.updates.installed') }}</h2>
                <span v-if="status" class="text-base-content/85">
                    NoAr {{ status.version }} ·
                    <template v-if="status.commit">
                        <span class="font-mono">{{ short(status.commit) }}</span>
                        <template v-if="status.commit_time"> · {{ when(status.commit_time) }} </template>
                    </template>
                    <template v-else>{{ t('settings.updates.localBuild') }}</template>
                </span>
                <span v-if="status?.checked_at" class="text-[13px] text-muted">
                    {{ t('settings.updates.lastCheck', { when: when(status.checked_at) }) }}
                    <template v-if="!status.available && latest && !status.error">
                        ·
                        {{
                            status.commit
                                ? t('settings.updates.newest')
                                : t('settings.updates.latestIs', { commit: latest.version ?? short(latest.commit) })
                        }}
                    </template>
                </span>
                <span v-else class="text-[13px] text-muted">{{ t('settings.updates.neverChecked') }}</span>
                <span v-if="status?.error" class="text-[13px] text-on-air-text" role="status">
                    {{ t('settings.updates.checkFailed', { error: status.error }) }}
                </span>
            </div>
            <button type="button" class="btn-quiet" :disabled="updates.checking" @click="check">
                <span v-if="updates.checking" class="loading loading-spinner loading-xs" aria-hidden="true" />
                <i v-else class="bi bi-arrow-repeat" aria-hidden="true" />{{ t('settings.updates.checkNow') }}
            </button>
        </section>

        <SectionCard :title="t('settings.updates.howTitle')">
            <ToggleRow
                v-model="autoCheck"
                :label="t('settings.updates.autoCheck')"
                :hint="t('settings.updates.autoCheckHint')"
                :disabled="saving || !status"
            />

            <div class="flex flex-wrap items-center gap-x-4 gap-y-2 border-t border-line pt-3">
                <ToggleRow
                    v-model="autoUpdate"
                    class="flex-[1_1_320px]"
                    :label="t('settings.updates.autoUpdate')"
                    :hint="t('settings.updates.autoUpdateHint')"
                    :disabled="saving || !updater?.connected"
                />
                <label class="inline-flex items-center gap-2 text-base-content/85">
                    {{ t('settings.updates.at') }}
                    <input
                        v-model="autoTime"
                        type="time"
                        class="field-input font-mono w-28"
                        :aria-label="t('settings.updates.autoUpdateTime')"
                        :disabled="saving || !updater?.connected"
                        @change="saveAutoTime"
                    />
                </label>
            </div>

            <div
                v-if="updater?.connected"
                class="flex items-center gap-2 border-t border-line pt-3 text-[13px] text-muted"
                role="status"
            >
                <span class="dot bg-ok" aria-hidden="true" />
                {{ t('settings.updates.updaterConnected', { address: updater.address ?? 'noar-updater' }) }}
            </div>
            <div
                v-else-if="updater?.configured"
                class="flex items-start gap-2 border-t border-line pt-3 text-[13px] text-on-air-text"
                role="status"
            >
                <span class="dot bg-on-air mt-1.5" aria-hidden="true" />
                <span>
                    {{ t('settings.updates.updaterDown', { address: updater.address ?? '' }) }}
                    <span v-if="updater.error" class="text-muted">({{ updater.error }})</span>
                </span>
            </div>
            <div v-else class="flex flex-col gap-2.5 border-t border-line pt-3">
                <h3 class="m-0 text-[15px] font-semibold">{{ t('settings.updates.setupTitle') }}</h3>
                <p class="m-0 text-[13px] text-base-content/85">{{ t('settings.updates.setupText') }}</p>
                <p class="m-0 text-[13px] text-base-content/85">{{ t('settings.updates.setupSteps') }}</p>
                <div class="flex flex-wrap items-start gap-2">
                    <pre
                        class="m-0 flex-[1_1_320px] min-w-0 font-mono text-[13px] bg-base-200 border border-line-strong rounded-field px-3 py-2 whitespace-pre-wrap break-all"
                        >{{ setupCommands }}</pre>
                    <button type="button" class="btn-quiet min-h-10" @click="copy(setupCommands)">
                        <i class="bi bi-clipboard" aria-hidden="true" />{{ t('settings.updates.copy') }}
                    </button>
                </div>
                <p class="m-0 text-[13px] text-muted">{{ t('settings.updates.setupDone') }}</p>
                <template v-if="!status?.available">
                    <span class="text-[13px] text-base-content/85">{{ t('settings.updates.onServer') }}</span>
                    <div class="flex flex-wrap items-center gap-2">
                        <code
                            class="flex-[1_1_320px] min-w-0 font-mono text-[13px] bg-base-200 border border-line-strong rounded-field px-3 py-2 break-all"
                        >
                            {{ SERVER_COMMAND }}
                        </code>
                        <button type="button" class="btn-quiet min-h-10" @click="copy(SERVER_COMMAND)">
                            <i class="bi bi-clipboard" aria-hidden="true" />{{ t('settings.updates.copy') }}
                        </button>
                    </div>
                </template>
            </div>
        </SectionCard>
    </template>

    <GenericModal
        :show="confirming"
        :title="t('settings.updates.confirmTitle')"
        :ok-label="t('settings.updates.installNow')"
        :modal-action="install"
    >
        <p class="m-0 max-w-md">{{ t('settings.updates.confirmText') }}</p>
    </GenericModal>

    <UpdateProgress v-if="installingFrom !== null" :started-at="installingFrom" @close="installingFrom = null" />
</template>
