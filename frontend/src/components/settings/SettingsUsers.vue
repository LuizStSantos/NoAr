<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import SectionCard from '@/components/settings/SectionCard.vue'
import UserEditor, { type UserDraft } from '@/components/settings/UserEditor.vue'
import GenericModal from '@/components/utils/GenericModal.vue'
import { authFetch } from '@/composables/authFetch'
import { useAuth } from '@/stores/auth'
import { useConfig } from '@/stores/config'
import { useIndex } from '@/stores/index'

const { t } = useI18n()
const authStore = useAuth()
const configStore = useConfig()
const indexStore = useIndex()

type Draft = UserDraft

const users = ref<User[]>([])
const editing = ref<Draft | null>(null)
const saving = ref(false)
const deleting = ref<User | null>(null)

const isGlobal = computed(() => authStore.role === 'global_admin')
const me = computed(() => configStore.currentUser)

function draftOf(user?: User): Draft {
    return {
        id: user?.id ?? 0,
        username: user?.username ?? '',
        mail: user?.mail ?? '',
        password: '',
        confirm: '',
        role_id: user?.role_id ?? 3,
        channel_ids: (user?.channel_ids ?? [configStore.channels[configStore.i]?.id ?? 1]).filter((id) => id > 0),
        two_factor: user?.two_factor ?? false,
    }
}

async function loadUsers() {
    if (!isGlobal.value) {
        editing.value = draftOf(configStore.configUser)
        return
    }

    users.value = await authFetch<User[]>('/api/users', { headers: authStore.authHeader })
}

onMounted(loadUsers)

function channelNames(user: User): string {
    if (user.role_id === 1) {
        return t('settings.users.allChannels')
    }

    const names = configStore.channels
        .filter((channel) => user.channel_ids?.includes(channel.id))
        .map((channel) => channel.name)

    return names.length ? names.join(', ') : '—'
}

function toggleEdit(user?: User) {
    if (editing.value && editing.value.id === (user?.id ?? 0)) {
        editing.value = null
        return
    }

    editing.value = draftOf(user)
}

const error = computed(() => {
    const draft = editing.value

    if (!draft) {
        return ''
    }

    if (!draft.id && !draft.username.trim()) {
        return t('settings.users.errorName')
    }

    if (!draft.id && !draft.password) {
        return t('settings.users.errorPassword')
    }

    if (draft.password && draft.password !== draft.confirm) {
        return t('user.mismatch')
    }

    return ''
})

async function saveUser() {
    const draft = editing.value

    if (!draft || error.value) {
        return
    }

    saving.value = true

    const body = {
        username: draft.username.trim(),
        mail: draft.mail.trim(),
        password: draft.password,
        role_id: draft.role_id,
        channel_ids: draft.channel_ids,
        two_factor: draft.two_factor,
    }

    try {
        await authStore.inspectToken()

        if (draft.id) {
            await authFetch(`/api/user/${draft.id}`, {
                method: 'PUT',
                headers: { ...configStore.contentType, ...authStore.authHeader },
                body: JSON.stringify(body),
                responseType: 'text',
            })
            indexStore.msgAlert('success', t('user.updateSuccess'), 2)
        } else {
            await authFetch('/api/user', {
                method: 'POST',
                headers: { ...configStore.contentType, ...authStore.authHeader },
                body: JSON.stringify(body),
                responseType: 'text',
            })
            indexStore.msgAlert('success', t('user.addSuccess'), 2)
        }

        if (isGlobal.value) {
            editing.value = null
            await loadUsers()
        } else {
            draft.password = ''
            draft.confirm = ''
        }

        await configStore.getUserConfig()
    } catch (e: any) {
        indexStore.msgAlert('error', e?.data ?? (draft.id ? t('user.updateFailed') : t('user.addFailed')), 4)
    } finally {
        saving.value = false
    }
}

async function deleteUser(confirm: boolean) {
    const user = deleting.value
    deleting.value = null

    if (!confirm || !user || user.id === me.value) {
        return
    }

    try {
        await authFetch(`/api/user/${user.id}`, {
            method: 'DELETE',
            headers: authStore.authHeader,
            responseType: 'text',
        })
        editing.value = null
        await loadUsers()
        indexStore.msgAlert('success', t('user.deleteSuccess'), 2)
    } catch (e) {
        indexStore.msgAlert('error', `${t('user.deleteError')}: ${e}`, 3)
    }
}
</script>
<template>
    <div class="flex flex-wrap items-end justify-between gap-3">
        <div>
            <h1 class="m-0 text-xl font-semibold">{{ t('settings.sections.users') }}</h1>
            <p class="mt-1 mb-0 text-muted">
                {{ isGlobal ? t('settings.users.intro') : t('settings.users.introSelf') }}
            </p>
        </div>
        <button v-if="isGlobal" class="btn-main" @click="toggleEdit()">+ {{ t('settings.users.add') }}</button>
    </div>

    <template v-if="!isGlobal">
        <SectionCard v-if="editing" :title="editing.username">
            <div class="grid grid-cols-1 md:grid-cols-2 gap-3.5 max-w-3xl">
                <label class="field-label md:col-span-2">
                    {{ t('settings.users.mail') }}
                    <input v-model.trim="editing.mail" type="email" class="field-input" autocomplete="email" />
                </label>
                <label class="field-label">
                    {{ t('settings.users.newPassword') }}
                    <input
                        v-model="editing.password"
                        type="password"
                        class="field-input"
                        autocomplete="new-password"
                        :placeholder="t('settings.users.keepPassword')"
                    />
                </label>
                <label class="field-label">
                    {{ t('settings.users.repeatPassword') }}
                    <input v-model="editing.confirm" type="password" class="field-input" autocomplete="new-password" />
                </label>
            </div>
            <p v-if="error" class="m-0 text-sm text-error" role="alert">{{ error }}</p>
            <button class="btn-main self-start" :disabled="saving || !!error" @click="saveUser">
                {{ t('settings.save') }}
            </button>
        </SectionCard>
    </template>

    <section v-else :aria-label="t('settings.users.list')" class="card-box relative overflow-x-auto">
        <table class="w-full min-w-[720px] border-collapse text-[14px]">
            <thead>
                <tr class="text-muted text-left text-xs">
                    <th class="px-4.5 py-3 font-medium">{{ t('settings.users.user') }}</th>
                    <th class="px-2 py-3 font-medium">{{ t('settings.users.role') }}</th>
                    <th class="px-2 py-3 font-medium">{{ t('settings.users.channels') }}</th>
                    <th class="px-2 py-3 font-medium">{{ t('settings.users.twoFactor') }}</th>
                    <th class="px-4.5 py-3 w-[110px]">
                        <span class="sr-only">{{ t('settings.actions') }}</span>
                    </th>
                </tr>
            </thead>
            <tbody>
                <tr v-if="editing && !editing.id" class="border-t border-line bg-raised">
                    <td colspan="5" class="px-4.5 py-4">
                        <div class="font-semibold mb-3">{{ t('settings.users.newUser') }}</div>
                        <UserEditor
                            v-model:draft="editing"
                            :self="false"
                            :saving="saving"
                            :error="error"
                            @save="saveUser"
                        />
                    </td>
                </tr>
                <template v-for="user in users" :key="user.id">
                    <tr class="border-t border-line" :class="editing?.id === user.id ? 'bg-raised' : ''">
                        <td class="px-4.5 py-2.5">
                            <div class="font-semibold">
                                {{ user.username }}
                                <span
                                    v-if="user.id === me"
                                    class="text-xs font-normal bg-accent-soft text-accent-text rounded-md px-1.5 py-0.5 ms-1"
                                    >{{ t('settings.users.you') }}</span
                                >
                            </div>
                            <div class="text-[13px] text-muted">{{ user.mail || '—' }}</div>
                        </td>
                        <td class="px-2 py-2.5">{{ t(`settings.users.roles.${user.role_id ?? 3}`) }}</td>
                        <td class="px-2 py-2.5 text-muted">{{ channelNames(user) }}</td>
                        <td class="px-2 py-2.5" :class="user.two_factor ? 'text-ok' : 'text-muted'">
                            {{ user.two_factor ? t('settings.users.on') : t('settings.users.off') }}
                        </td>
                        <td class="px-4.5 py-2.5 text-right">
                            <button
                                class="btn-quiet min-h-9"
                                :aria-expanded="editing?.id === user.id"
                                @click="toggleEdit(user)"
                            >
                                {{ editing?.id === user.id ? t('settings.close') : t('settings.edit') }}
                            </button>
                        </td>
                    </tr>
                    <tr v-if="editing && editing.id === user.id" class="bg-raised">
                        <td colspan="5" class="px-4.5 pb-4 pt-1">
                            <UserEditor
                                v-model:draft="editing"
                                :self="user.id === me"
                                :saving="saving"
                                :error="error"
                                @save="saveUser"
                                @remove="deleting = user"
                            />
                        </td>
                    </tr>
                </template>
            </tbody>
        </table>
    </section>

    <p v-if="isGlobal" class="m-0 text-[13px] text-muted">{{ t('settings.users.selfHint') }}</p>

    <GenericModal
        :show="deleting !== null"
        :title="t('settings.users.deleteTitle')"
        :ok-label="t('settings.users.delete')"
        danger
        :modal-action="deleteUser"
    >
        <p class="m-0 max-w-sm">{{ t('settings.users.deleteQuestion', { name: deleting?.username }) }}</p>
    </GenericModal>
</template>
