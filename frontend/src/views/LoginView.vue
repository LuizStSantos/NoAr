<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useHead } from '@unhead/vue'
import { useRouter } from 'vue-router'

import AuthToolbar from '@/components/auth/AuthToolbar.vue'
import { useAuth } from '@/stores/auth'

const { t } = useI18n()
const authStore = useAuth()
const router = useRouter()

const password = ref('')
const showPassword = ref(false)
const error = ref('')
const busy = ref(false)

const features = [
    { icon: 'bi-list-ul', key: 'login.featurePlaylist' },
    { icon: 'bi-calendar-event', key: 'login.featureSchedule' },
    { icon: 'bi-broadcast', key: 'login.featureLive' },
    { icon: 'bi-card-text', key: 'login.featureGuide' },
]

useHead({ title: () => t('login.title') })

async function login() {
    busy.value = true
    error.value = ''

    try {
        const result = await authStore.obtainVerificationCode(password.value)

        if (result.status === 403 || result.status === 401) {
            error.value = t('login.wrong')
            return
        }

        if (result.status !== 200) {
            error.value = t('login.failed')
            return
        }

        password.value = ''

        if (result.verificationRequired) {
            authStore.beginVerification()
            await router.push({ name: 'verification' })
            return
        }

        await router.replace({ name: 'home' })
    } finally {
        busy.value = false
    }
}
</script>
<template>
    <div class="min-h-screen flex bg-base-200 text-base-content">
        <section
            :aria-label="t('login.brand')"
            class="hidden lg:flex flex-[1_1_520px] flex-col justify-between gap-10 bg-base-100 border-e border-line px-14 py-12"
        >
            <div class="flex items-center gap-3 text-xl font-semibold">
                <img src="@/assets/images/noar-logo.svg" alt="" class="w-10 h-10" />NoAr
            </div>

            <div class="flex flex-col gap-7 max-w-[520px]">
                <span
                    class="inline-flex self-start items-center gap-2 bg-on-air-soft text-on-air-text rounded-full px-3.5 py-1.5 text-xs font-semibold tracking-[0.06em]"
                >
                    <span class="dot bg-on-air" aria-hidden="true" />{{ t('login.tagline') }}
                </span>
                <h1 class="m-0 text-[44px] leading-[1.1] font-semibold tracking-[-0.01em]">
                    {{ t('login.headline') }}
                </h1>
                <ul class="m-0 p-0 list-none flex flex-col gap-3.5 text-[15px] text-base-content/80">
                    <li v-for="feature in features" :key="feature.key" class="flex items-center gap-3">
                        <span
                            class="w-8 h-8 rounded-lg bg-raised text-accent-text flex items-center justify-center"
                            aria-hidden="true"
                        >
                            <i class="bi" :class="feature.icon" />
                        </span>
                        {{ t(feature.key) }}
                    </li>
                </ul>
            </div>

            <div class="flex flex-wrap gap-x-4.5 gap-y-2 text-[13px] text-subtle">
                <span>NoAr</span>
                <a
                    href="https://github.com/LuizStSantos/NoAr"
                    target="_blank"
                    rel="noopener"
                    class="text-muted hover:text-base-content"
                >
                    {{ t('login.source') }}
                </a>
            </div>
        </section>

        <section
            :aria-label="t('login.title')"
            class="flex-[1_1_480px] flex flex-col px-4 xs:px-8 lg:px-14 py-6 lg:py-8"
        >
            <div class="flex items-center gap-2">
                <div class="flex lg:hidden items-center gap-2.5 text-base font-semibold flex-1">
                    <img src="@/assets/images/noar-logo.svg" alt="" class="w-8 h-8" />NoAr
                </div>
                <AuthToolbar class="ms-auto" />
            </div>

            <div class="flex-1 flex items-center justify-center py-10">
                <form class="w-full max-w-[380px] flex flex-col gap-4.5" @submit.prevent="login">
                    <div>
                        <h2 class="m-0 text-[26px] font-semibold">{{ t('login.title') }}</h2>
                        <p class="mt-1.5 mb-0 text-muted">{{ t('login.intro') }}</p>
                    </div>

                    <label class="flex flex-col gap-1.5 text-[13px] text-base-content/80">
                        {{ t('login.username') }}
                        <input
                            v-model="authStore.username"
                            name="username"
                            autocomplete="username"
                            autocapitalize="none"
                            spellcheck="false"
                            required
                            class="field-input min-h-12 rounded-[10px] px-3.5 text-[15px] bg-base-100"
                        />
                    </label>

                    <label class="flex flex-col gap-1.5 text-[13px] text-base-content/80">
                        {{ t('login.password') }}
                        <span
                            class="flex items-center bg-base-100 border border-line-strong rounded-[10px] pe-1 focus-within:border-accent focus-within:ring-3 focus-within:ring-accent-soft"
                        >
                            <input
                                v-model="password"
                                name="password"
                                :type="showPassword ? 'text' : 'password'"
                                autocomplete="current-password"
                                required
                                class="flex-1 min-w-0 bg-transparent border-0 px-3.5 min-h-[46px] text-[15px] text-base-content outline-none"
                            />
                            <button
                                type="button"
                                class="btn-icon min-w-10 min-h-10 text-muted"
                                :aria-label="showPassword ? t('login.hidePassword') : t('login.showPassword')"
                                :aria-pressed="showPassword"
                                @click="showPassword = !showPassword"
                            >
                                <i class="bi" :class="showPassword ? 'bi-eye-slash' : 'bi-eye'" aria-hidden="true" />
                            </button>
                        </span>
                    </label>

                    <p
                        v-if="error"
                        role="alert"
                        class="m-0 flex gap-2 items-start bg-on-air-soft text-on-air-text border border-on-air/40 rounded-[10px] px-3 py-2.5 text-[13px]"
                    >
                        <i class="bi bi-exclamation-circle mt-px" aria-hidden="true" />{{ error }}
                    </p>

                    <button type="submit" class="btn-main min-h-12 rounded-[10px] text-[15px]" :disabled="busy">
                        <span v-if="busy" class="loading loading-spinner loading-xs" />{{ t('login.submit') }}
                    </button>

                    <p class="m-0 text-[13px] text-subtle">{{ t('login.forgot') }}</p>
                </form>
            </div>
        </section>
    </div>
</template>
