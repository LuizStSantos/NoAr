<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useHead } from '@unhead/vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'

import AuthToolbar from '@/components/auth/AuthToolbar.vue'
import { useAuth } from '@/stores/auth'

/**
 * First run: create the administrator, confirm the folders and optionally
 * set up email, then sign in straight away.
 */
const { t } = useI18n()
const router = useRouter()
const authStore = useAuth()

const loading = ref(true)
const submitting = ref(false)
const error = ref('')
const confirmPassword = ref('')
const showPaths = ref(false)
const useMail = ref(false)
const form = ref({
    logs: '',
    playlists: '',
    public: '',
    storage: '',
    shared: false,
    smtp_server: '',
    smtp_user: '',
    smtp_password: '',
    smtp_starttls: false,
    smtp_port: 465,
    username: 'admin',
    mail: '',
    password: '',
    two_factor: false,
})

const paths = computed(
    () =>
        [
            { key: 'storage', label: t('setup.pathStorage') },
            { key: 'playlists', label: t('setup.pathPlaylists') },
            { key: 'public', label: t('setup.pathPublic') },
            { key: 'logs', label: t('setup.pathLogs') },
        ] as const,
)

/** The Docker image fills every path under its /data volume. */
const dockerPaths = computed(() => paths.value.every((path) => form.value[path.key].startsWith('/data/')))

const strength = computed(() => {
    const password = form.value.password
    let score = 0

    if (password.length >= 8) score++
    if (password.length >= 12) score++
    if (/[a-z]/.test(password) && /[A-Z]/.test(password)) score++
    if (/\d/.test(password)) score++
    if (/[^A-Za-z0-9]/.test(password)) score++

    score = Math.min(4, score)

    if (!password) {
        return { score: 0, label: '', tone: '' }
    }

    if (password.length < 8 || score <= 1) {
        return { score: Math.max(1, score), label: t('setup.weak'), tone: 'bg-on-air text-on-air-text' }
    }

    if (score === 2) {
        return { score, label: t('setup.fair'), tone: 'bg-accent text-accent-text' }
    }

    return { score, label: score === 3 ? t('setup.good') : t('setup.strong'), tone: 'bg-ok text-ok' }
})

const passwordsMatch = computed(() => Boolean(confirmPassword.value) && confirmPassword.value === form.value.password)

const canSubmit = computed(
    () =>
        !submitting.value &&
        form.value.username.trim() !== '' &&
        form.value.mail.trim() !== '' &&
        form.value.password.length >= 8 &&
        passwordsMatch.value &&
        (!useMail.value || (form.value.smtp_server.trim() !== '' && form.value.smtp_user.trim() !== '')),
)

useHead({ title: () => t('setup.title') })

onMounted(loadSetup)

/** Example values of the stored defaults are hints, not settings. */
function realValue(value: string | undefined): string {
    return value && !value.includes('example.org') ? value : ''
}

async function loadSetup() {
    try {
        const response = await fetch('/api/setup')
        const data = await response.json()

        if (!response.ok || !data.required) {
            await router.replace({ name: 'login' })
            return
        }

        const settings = data.settings ?? {}

        Object.assign(form.value, {
            logs: settings.logs ?? '',
            playlists: settings.playlists ?? '',
            public: settings.public ?? '',
            storage: settings.storage ?? '',
            shared: Boolean(settings.shared),
            smtp_server: realValue(settings.smtp_server),
            smtp_user: realValue(settings.smtp_user),
            smtp_password: settings.smtp_password ?? '',
            smtp_starttls: Boolean(settings.smtp_starttls),
            smtp_port: settings.smtp_port || 465,
        })
        useMail.value = form.value.smtp_server !== ''
        showPaths.value = paths.value.some((path) => !form.value[path.key])
    } catch {
        error.value = t('setup.loadFailed')
    } finally {
        loading.value = false
    }
}

async function submit() {
    if (!canSubmit.value) {
        return
    }

    submitting.value = true
    error.value = ''

    const body = { ...form.value, username: form.value.username.trim(), mail: form.value.mail.trim() }

    if (!useMail.value) {
        Object.assign(body, { smtp_server: '', smtp_user: '', smtp_password: '', two_factor: false })
    }

    try {
        const response = await fetch('/api/setup', {
            method: 'POST',
            headers: { 'content-type': 'application/json;charset=UTF-8' },
            body: JSON.stringify(body),
        })

        if (!response.ok) {
            throw new Error((await response.text()).replace(/^"|"$/g, '') || t('setup.saveFailed'))
        }
    } catch (cause) {
        error.value = cause instanceof Error ? cause.message : t('setup.saveFailed')
        submitting.value = false
        return
    }

    // Sign in with the new account; fall back to the sign-in page.
    authStore.username = body.username

    const login = await authStore.obtainVerificationCode(body.password)

    form.value.password = ''
    confirmPassword.value = ''
    submitting.value = false

    if (login.status === 200 && login.verificationRequired) {
        authStore.beginVerification()
        await router.replace({ name: 'verification' })
    } else if (login.status === 200) {
        await router.replace({ name: 'home' })
    } else {
        await router.replace({ name: 'login' })
    }
}
</script>

<template>
    <div class="min-h-screen bg-base-200 text-base-content">
        <header class="max-w-[1440px] mx-auto px-4 xs:px-6 py-5 flex flex-wrap items-center gap-3">
            <div class="flex items-center gap-2.5 text-lg font-semibold flex-1">
                <img src="@/assets/images/noar-logo.svg" alt="" class="w-8 h-8" />NoAr
            </div>
            <AuthToolbar />
        </header>

        <main class="max-w-[760px] mx-auto px-4 xs:px-6 pt-4 pb-12">
            <div v-if="loading" class="flex justify-center py-20">
                <span class="loading loading-spinner" />
            </div>

            <form v-else class="flex flex-col gap-4.5" novalidate @submit.prevent="submit">
                <div>
                    <h1 class="m-0 text-[28px] xs:text-[32px] font-semibold tracking-[-0.01em] leading-tight">
                        {{ t('setup.title') }}
                    </h1>
                    <p class="mt-1.5 mb-0 text-muted text-[15px]">{{ t('setup.intro') }}</p>
                </div>

                <section aria-labelledby="setup-admin" class="card-box px-5 py-4.5 flex flex-col gap-3.5">
                    <div class="flex items-center gap-3">
                        <span
                            class="w-7 h-7 rounded-full bg-accent text-accent-content font-semibold flex items-center justify-center"
                            aria-hidden="true"
                        >
                            1
                        </span>
                        <h2 id="setup-admin" class="m-0 text-[17px] font-semibold">{{ t('setup.admin') }}</h2>
                    </div>
                    <div class="grid grid-cols-1 sm:grid-cols-2 gap-3.5">
                        <label class="field-label">
                            {{ t('setup.username') }}
                            <input
                                v-model="form.username"
                                name="username"
                                autocomplete="username"
                                autocapitalize="none"
                                spellcheck="false"
                                required
                                class="field-input min-h-11 text-[15px]"
                            />
                        </label>
                        <label class="field-label">
                            {{ t('setup.mail') }}
                            <input
                                v-model="form.mail"
                                name="mail"
                                type="email"
                                autocomplete="email"
                                required
                                class="field-input min-h-11 text-[15px]"
                            />
                        </label>
                        <label class="field-label">
                            {{ t('setup.password') }}
                            <input
                                v-model="form.password"
                                type="password"
                                autocomplete="new-password"
                                required
                                minlength="8"
                                aria-describedby="setup-strength"
                                class="field-input min-h-11 text-[15px]"
                            />
                            <span class="flex gap-1 mt-0.5" aria-hidden="true">
                                <span
                                    v-for="bar in 4"
                                    :key="bar"
                                    class="flex-1 h-1 rounded-sm"
                                    :class="bar <= strength.score ? strength.tone.split(' ')[0] : 'bg-line-strong'"
                                />
                            </span>
                            <span id="setup-strength" :class="strength.tone.split(' ')[1] ?? 'text-subtle'">
                                {{ strength.label || t('setup.passwordHint') }}
                            </span>
                        </label>
                        <label class="field-label">
                            {{ t('setup.confirmPassword') }}
                            <input
                                v-model="confirmPassword"
                                type="password"
                                autocomplete="new-password"
                                required
                                aria-describedby="setup-match"
                                class="field-input min-h-11 text-[15px]"
                            />
                            <span id="setup-match" class="inline-flex items-center gap-1.5 min-h-5" aria-live="polite">
                                <template v-if="passwordsMatch">
                                    <i class="bi bi-check2 text-ok" aria-hidden="true" />
                                    <span class="text-ok">{{ t('setup.passwordsMatch') }}</span>
                                </template>
                                <template v-else-if="confirmPassword">
                                    <i class="bi bi-x text-on-air-text" aria-hidden="true" />
                                    <span class="text-on-air-text">{{ t('setup.passwordMismatch') }}</span>
                                </template>
                            </span>
                        </label>
                    </div>
                </section>

                <section aria-labelledby="setup-paths" class="card-box px-5 py-4.5 flex flex-col gap-3.5">
                    <div class="flex flex-wrap items-center gap-3">
                        <span
                            class="w-7 h-7 rounded-full bg-base-300 font-semibold flex items-center justify-center"
                            aria-hidden="true"
                        >
                            2
                        </span>
                        <h2 id="setup-paths" class="m-0 text-[17px] font-semibold flex-1">{{ t('setup.folders') }}</h2>
                        <span
                            v-if="dockerPaths"
                            class="inline-flex items-center gap-1.5 text-xs text-ok bg-ok-soft rounded-full px-2.5 py-0.5"
                        >
                            <i class="bi bi-check2" aria-hidden="true" />{{ t('setup.dockerReady') }}
                        </span>
                    </div>
                    <p class="m-0 text-muted">
                        <i18n-t v-if="dockerPaths" keypath="setup.dockerHint" tag="span">
                            <template #folder>
                                <span class="font-mono text-base-content/85">./data</span>
                            </template>
                        </i18n-t>
                        <template v-else>{{ t('setup.foldersHint') }}</template>
                    </p>

                    <dl
                        v-if="!showPaths"
                        class="m-0 grid grid-cols-1 xs:grid-cols-[180px_minmax(0,1fr)] border border-line rounded-[10px] overflow-hidden"
                    >
                        <template v-for="(path, index) in paths" :key="path.key">
                            <dt
                                class="px-3.5 pt-2.5 xs:py-2.5 text-muted bg-base-200"
                                :class="index < paths.length - 1 ? 'xs:border-b border-line' : ''"
                            >
                                {{ path.label }}
                            </dt>
                            <dd
                                class="m-0 px-3.5 pb-2.5 xs:py-2.5 font-mono break-all bg-base-200 xs:bg-transparent"
                                :class="index < paths.length - 1 ? 'border-b border-line' : ''"
                            >
                                {{ form[path.key] }}
                            </dd>
                        </template>
                    </dl>

                    <div v-else class="grid grid-cols-1 sm:grid-cols-2 gap-3.5">
                        <label v-for="path in paths" :key="path.key" class="field-label">
                            {{ path.label }}
                            <input v-model="form[path.key]" required class="field-input font-mono" />
                        </label>
                        <label class="flex items-start gap-2.5 sm:col-span-2 cursor-pointer">
                            <input
                                v-model="form.shared"
                                type="checkbox"
                                class="checkbox checkbox-sm checkbox-accent rounded-[4px] mt-0.5"
                            />
                            <span>
                                {{ t('setup.shared') }}
                                <span class="block text-xs text-subtle">{{ t('setup.sharedHint') }}</span>
                            </span>
                        </label>
                    </div>

                    <button
                        type="button"
                        class="self-start inline-flex items-center gap-1.5 bg-transparent border-0 p-0 min-h-9 text-base-content/80 hover:text-base-content cursor-pointer"
                        :aria-expanded="showPaths"
                        @click="showPaths = !showPaths"
                    >
                        <i
                            class="bi bi-chevron-right text-xs transition-transform"
                            :class="showPaths ? 'rotate-90' : ''"
                            aria-hidden="true"
                        />{{ t('setup.otherFolders') }}
                    </button>
                </section>

                <section aria-labelledby="setup-mail" class="card-box px-5 py-4.5 flex flex-col gap-3">
                    <div class="flex flex-wrap items-center gap-3">
                        <span
                            class="w-7 h-7 rounded-full bg-base-300 font-semibold flex items-center justify-center"
                            aria-hidden="true"
                        >
                            3
                        </span>
                        <h2 id="setup-mail" class="m-0 text-[17px] font-semibold flex-1">
                            {{ t('setup.mailTitle') }}
                            <span class="text-subtle font-normal text-sm">· {{ t('setup.optional') }}</span>
                        </h2>
                        <label class="flex items-center gap-2.5 min-h-10 cursor-pointer">
                            <input
                                v-model="useMail"
                                type="checkbox"
                                role="switch"
                                class="toggle toggle-sm toggle-accent"
                            />
                            {{ t('setup.mailNow') }}
                        </label>
                    </div>
                    <p class="m-0 text-muted">{{ t('setup.mailHint') }}</p>

                    <div v-if="useMail" class="grid grid-cols-1 sm:grid-cols-6 gap-3.5 pt-1">
                        <label class="field-label sm:col-span-5">
                            {{ t('setup.smtpServer') }}
                            <input
                                v-model="form.smtp_server"
                                placeholder="smtp.example.com"
                                autocapitalize="none"
                                spellcheck="false"
                                class="field-input"
                            />
                        </label>
                        <label class="field-label">
                            {{ t('setup.smtpPort') }}
                            <input
                                v-model.number="form.smtp_port"
                                type="number"
                                min="1"
                                max="65535"
                                class="field-input font-mono"
                            />
                        </label>
                        <label class="field-label sm:col-span-3">
                            {{ t('setup.smtpUser') }}
                            <input
                                v-model="form.smtp_user"
                                autocomplete="off"
                                autocapitalize="none"
                                spellcheck="false"
                                class="field-input"
                            />
                        </label>
                        <label class="field-label sm:col-span-3">
                            {{ t('setup.smtpPassword') }}
                            <input
                                v-model="form.smtp_password"
                                type="password"
                                autocomplete="off"
                                class="field-input"
                            />
                        </label>
                        <label class="flex items-center gap-2.5 sm:col-span-6 cursor-pointer">
                            <input
                                v-model="form.smtp_starttls"
                                type="checkbox"
                                class="checkbox checkbox-sm checkbox-accent rounded-[4px]"
                            />
                            {{ t('setup.starttls') }}
                        </label>
                        <label class="flex items-start gap-2.5 sm:col-span-6 cursor-pointer">
                            <input
                                v-model="form.two_factor"
                                type="checkbox"
                                class="checkbox checkbox-sm checkbox-accent rounded-[4px] mt-0.5"
                            />
                            <span>
                                {{ t('setup.twoFactor') }}
                                <span class="block text-xs text-subtle">{{ t('setup.twoFactorHint') }}</span>
                            </span>
                        </label>
                    </div>
                </section>

                <p
                    v-if="error"
                    role="alert"
                    class="m-0 flex gap-2 items-start bg-on-air-soft text-on-air-text border border-on-air/40 rounded-[10px] px-3 py-2.5 text-[13px]"
                >
                    <i class="bi bi-exclamation-circle mt-px" aria-hidden="true" />{{ error }}
                </p>

                <div class="flex flex-wrap items-center gap-3 pt-1">
                    <span class="text-muted text-[13px] flex-[1_1_260px]">{{ t('setup.afterHint') }}</span>
                    <button
                        type="submit"
                        class="btn-main min-h-12 rounded-[10px] px-5.5 text-[15px]"
                        :disabled="!canSubmit"
                    >
                        <span v-if="submitting" class="loading loading-spinner loading-xs" />{{ t('setup.complete') }}
                    </button>
                </div>
            </form>
        </main>
    </div>
</template>
