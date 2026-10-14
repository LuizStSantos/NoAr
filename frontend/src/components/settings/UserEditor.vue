<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'

import { useConfig } from '@/stores/config'

export interface UserDraft {
    id: number
    username: string
    mail: string
    password: string
    confirm: string
    role_id: number
    channel_ids: number[]
    two_factor: boolean
}

defineProps<{ self: boolean; saving: boolean; error: string }>()
const draft = defineModel<UserDraft>('draft', { required: true })
const emit = defineEmits<{ save: []; remove: [] }>()

const { t } = useI18n()
const configStore = useConfig()

const ROLES = [1, 2, 3]

const allChannels = computed(() => draft.value.role_id === 1)

function toggleChannel(id: number) {
    const ids = draft.value.channel_ids

    draft.value.channel_ids = ids.includes(id) ? ids.filter((item) => item !== id) : [...ids, id]
}
</script>
<template>
    <div class="flex flex-col gap-4">
        <div class="grid grid-cols-1 md:grid-cols-2 gap-3.5">
            <label class="field-label">
                {{ t('settings.users.username') }}
                <input v-model.trim="draft.username" class="field-input" :disabled="draft.id > 0" autocomplete="off" />
            </label>
            <label class="field-label">
                {{ t('settings.users.mail') }}
                <input v-model.trim="draft.mail" type="email" class="field-input" autocomplete="off" />
            </label>
            <label class="field-label">
                {{ draft.id ? t('settings.users.newPassword') : t('settings.users.password') }}
                <input
                    v-model="draft.password"
                    type="password"
                    class="field-input"
                    autocomplete="new-password"
                    :placeholder="draft.id ? t('settings.users.keepPassword') : ''"
                />
            </label>
            <label class="field-label">
                {{ t('settings.users.repeatPassword') }}
                <input v-model="draft.confirm" type="password" class="field-input" autocomplete="new-password" />
            </label>
            <label class="field-label md:col-span-2">
                {{ t('settings.users.role') }}
                <select v-model.number="draft.role_id" class="field-input" :disabled="self">
                    <option v-for="role in ROLES" :key="role" :value="role">
                        {{ t(`settings.users.roles.${role}`) }}: {{ t(`settings.users.roleHints.${role}`) }}
                    </option>
                </select>
                <span v-if="self" class="field-hint">{{ t('settings.users.ownRole') }}</span>
            </label>
            <fieldset class="md:col-span-2 border-0 p-0 m-0 flex flex-col gap-1.5">
                <legend class="text-xs text-muted mb-1.5">{{ t('settings.users.channels') }}</legend>
                <p v-if="allChannels" class="m-0 text-[13px] text-muted">{{ t('settings.users.allChannelsHint') }}</p>
                <div v-else class="flex flex-wrap gap-x-5 gap-y-1">
                    <label
                        v-for="channel in configStore.channels"
                        :key="channel.id"
                        class="flex items-center gap-2 min-h-10 cursor-pointer"
                    >
                        <input
                            type="checkbox"
                            class="checkbox checkbox-sm checkbox-accent"
                            :checked="draft.channel_ids.includes(channel.id)"
                            @change="toggleChannel(channel.id)"
                        />
                        {{ channel.name }}
                    </label>
                </div>
            </fieldset>
        </div>
        <p v-if="error" class="m-0 text-sm text-error" role="alert">{{ error }}</p>
        <div class="flex flex-wrap items-center gap-3">
            <label class="flex items-center gap-3 min-h-10 cursor-pointer">
                <input v-model="draft.two_factor" type="checkbox" role="switch" class="toggle toggle-accent" />
                {{ t('settings.users.ask2fa') }}
            </label>
            <div class="flex-auto" />
            <button
                v-if="draft.id && !self"
                class="btn-quiet border-[#5a2a2d]! text-[#ff9da0]!"
                @click="emit('remove')"
            >
                {{ t('settings.users.delete') }}
            </button>
            <button class="btn-main" :disabled="saving || !!error" @click="emit('save')">
                {{ draft.id ? t('settings.save') : t('settings.users.create') }}
            </button>
        </div>
    </div>
</template>
