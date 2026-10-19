<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useHead } from '@unhead/vue'
import { useRouter } from 'vue-router'

import AuthToolbar from '@/components/auth/AuthToolbar.vue'
import { useAuth } from '@/stores/auth'

const LENGTH = 6
const RESEND_AFTER = 60

const { t } = useI18n()
const router = useRouter()
const auth = useAuth()

const digits = ref<string[]>(Array(LENGTH).fill(''))
const inputs = ref<HTMLInputElement[]>([])
const error = ref('')
const notice = ref('')
const busy = ref(false)
const wait = ref(RESEND_AFTER)
let timer: ReturnType<typeof setInterval> | undefined

const code = computed(() => digits.value.join(''))
const waitLabel = computed(() => `${Math.floor(wait.value / 60)}:${String(wait.value % 60).padStart(2, '0')}`)

useHead({ title: () => t('verification.title') })

function startTimer() {
    wait.value = RESEND_AFTER
    clearInterval(timer)
    timer = setInterval(() => {
        wait.value = Math.max(0, wait.value - 1)

        if (!wait.value) {
            clearInterval(timer)
        }
    }, 1000)
}

function focus(index: number) {
    nextTick(() => inputs.value[Math.max(0, Math.min(LENGTH - 1, index))]?.focus())
}

/** Spread typed or pasted digits from `index` on. */
function fill(index: number, value: string) {
    const typed = value.replace(/\D/g, '')

    if (!typed) {
        digits.value[index] = ''
        return
    }

    for (const [offset, digit] of [...typed].entries()) {
        if (index + offset < LENGTH) {
            digits.value[index + offset] = digit
        }
    }

    focus(index + typed.length)

    if (code.value.length === LENGTH) {
        verify()
    }
}

function onInput(index: number, event: Event) {
    const input = event.target as HTMLInputElement

    fill(index, input.value)
    input.value = digits.value[index] ?? ''
}

function onPaste(index: number, event: ClipboardEvent) {
    event.preventDefault()
    fill(index, event.clipboardData?.getData('text') ?? '')
}

function onKey(index: number, event: KeyboardEvent) {
    if (event.key === 'Backspace' && !digits.value[index] && index > 0) {
        digits.value[index - 1] = ''
        focus(index - 1)
        event.preventDefault()
    } else if (event.key === 'ArrowLeft') {
        focus(index - 1)
    } else if (event.key === 'ArrowRight') {
        focus(index + 1)
    }
}

async function verify() {
    if (busy.value || code.value.length !== LENGTH) {
        return
    }

    busy.value = true
    error.value = ''
    notice.value = ''

    try {
        const status = await auth.verifyCode(code.value)

        if (status === 200) {
            await auth.selectAuthUser()
            await router.replace({ name: 'home' })
            return
        }

        error.value = status === 400 ? t('verification.expired') : t('verification.wrongCode')
        digits.value = Array(LENGTH).fill('')
        focus(0)
    } finally {
        busy.value = false
    }
}

async function resend() {
    error.value = ''
    notice.value = ''

    const result = await auth.resendVerificationCode()

    if (result.status === 200 && result.verificationRequired) {
        notice.value = t('verification.resent')
        digits.value = Array(LENGTH).fill('')
        startTimer()
        focus(0)
    } else {
        error.value = t('verification.resendFailed')
    }
}

function back() {
    auth.cancelVerification()
    router.replace({ name: 'login' })
}

onMounted(() => {
    if (!auth.verificationPending || !auth.username) {
        back()
        return
    }

    startTimer()
    focus(0)
})

onBeforeUnmount(() => clearInterval(timer))
</script>
<template>
    <div class="min-h-screen flex flex-col bg-base-200 text-base-content px-4 xs:px-8 lg:px-14 py-6 lg:py-8">
        <div class="flex flex-wrap items-center gap-3">
            <button
                type="button"
                class="inline-flex items-center gap-2 bg-transparent border-0 p-0 min-h-9.5 text-base-content/80 hover:text-base-content cursor-pointer"
                @click="back"
            >
                <i class="bi bi-chevron-left text-xs" aria-hidden="true" />{{ t('verification.back') }}
            </button>
            <div class="flex items-center gap-2.5 font-semibold text-base ms-auto me-2">
                <img src="@/assets/images/noar-logo.svg" alt="" class="w-7 h-7" />NoAr
            </div>
            <AuthToolbar />
        </div>

        <div class="flex-1 flex items-center justify-center py-10">
            <form class="w-full max-w-[400px] flex flex-col gap-5" @submit.prevent="verify">
                <span
                    class="w-13 h-13 rounded-[14px] bg-accent-soft text-accent-text flex items-center justify-center text-2xl"
                    aria-hidden="true"
                >
                    <i class="bi bi-envelope" />
                </span>
                <div>
                    <h1 class="m-0 text-[26px] font-semibold">{{ t('verification.title') }}</h1>
                    <i18n-t keypath="verification.sent" tag="p" class="mt-1.5 mb-0 text-muted">
                        <template #user>
                            <b class="text-base-content font-semibold">{{ auth.username }}</b>
                        </template>
                    </i18n-t>
                </div>

                <fieldset class="border-0 p-0 m-0">
                    <legend class="text-[13px] text-base-content/80 mb-2">{{ t('verification.code') }}</legend>
                    <div class="grid grid-cols-6 gap-2">
                        <input
                            v-for="(digit, index) in digits"
                            :key="index"
                            :ref="(el) => (inputs[index] = el as HTMLInputElement)"
                            :value="digit"
                            :aria-label="t('verification.digit', { n: index + 1 })"
                            inputmode="numeric"
                            :autocomplete="index === 0 ? 'one-time-code' : 'off'"
                            maxlength="6"
                            class="min-w-0 min-h-14 text-center font-mono text-2xl bg-base-100 text-base-content border border-line-strong rounded-[10px] outline-none focus:border-accent focus:ring-3 focus:ring-accent-soft"
                            :disabled="busy"
                            @input="onInput(index, $event)"
                            @paste="onPaste(index, $event)"
                            @keydown="onKey(index, $event)"
                            @focus="($event.target as HTMLInputElement).select()"
                        />
                    </div>
                </fieldset>

                <p
                    v-if="error"
                    role="alert"
                    class="m-0 flex gap-2 items-start bg-on-air-soft text-on-air-text border border-on-air/40 rounded-[10px] px-3 py-2.5 text-[13px]"
                >
                    <i class="bi bi-exclamation-circle mt-px" aria-hidden="true" />{{ error }}
                </p>
                <p
                    v-else-if="notice"
                    role="status"
                    class="m-0 flex gap-2 items-start bg-ok-soft text-base-content border border-ok/40 rounded-[10px] px-3 py-2.5 text-[13px]"
                >
                    <i class="bi bi-check2 mt-px text-ok" aria-hidden="true" />{{ notice }}
                </p>

                <button
                    type="submit"
                    class="btn-main min-h-12 rounded-[10px] text-[15px]"
                    :disabled="busy || code.length !== 6"
                >
                    <span v-if="busy" class="loading loading-spinner loading-xs" />{{ t('verification.confirm') }}
                </button>

                <p class="m-0 text-[13px] text-muted">
                    {{ t('verification.notArrived') }}
                    <button
                        type="button"
                        class="bg-transparent border-0 p-0 min-h-8 text-[13px]"
                        :class="wait ? 'text-subtle' : 'link-accent'"
                        :disabled="wait > 0"
                        @click="resend"
                    >
                        {{ wait ? t('verification.resendIn', { time: waitLabel }) : t('verification.resend') }}
                    </button>
                </p>
            </form>
        </div>
    </div>
</template>
