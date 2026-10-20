<script setup lang="ts">
import { computed, nextTick, reactive, ref, toRaw, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { onClickOutside } from '@vueuse/core'

import { CREDIT_ROLES, imageUrl, photoKey, uploadPhoto } from '@/composables/storage'
import { useIndex } from '@/stores/index'

/**
 * The credits of a sheet: each person with their job and, if wanted, a photo.
 * Photos are stored at once, in `.actors/` of the program folder; the people
 * are saved with the sheet.
 */
const props = defineProps<{
    channelId: number
    /** Storage path of the folder or clip whose sheet this is. */
    path: string
    /** The sheet of an episode: its own people, next to the program's. */
    episode: boolean
    photos: Record<string, string>
    /** The people of the program sheet, shown on an episode. */
    programCredits: Credit[]
    /** Storage path of the program folder, for an episode. */
    program: string | null
}>()

const credits = defineModel<Credit[]>({ required: true })

const emit = defineEmits<{ photos: [photos: Record<string, string>]; openProgram: [path: string] }>()

const { t } = useI18n()
const indexStore = useIndex()

const menuOpen = ref(false)
const menu = ref<HTMLElement>()
const photoInput = ref<HTMLInputElement>()
/** The person a photo is being picked for. */
const photoFor = ref<Credit | null>(null)
/** Upload progress by photo key, 0 to 1. */
const uploading = reactive<Record<string, number>>({})
/** Image addresses by storage path. */
const urls = reactive<Record<string, string>>({})

const keys = new WeakMap<Credit, number>()
let nextKey = 0

onClickOutside(menu, () => (menuOpen.value = false))

/** A stable key for a row, whatever is typed in it. */
function keyOf(credit: Credit): number {
    const raw = toRaw(credit)
    let key = keys.get(raw)

    if (key === undefined) {
        key = ++nextKey
        keys.set(raw, key)
    }

    return key
}

const groups = computed(() =>
    CREDIT_ROLES.map((role) => ({
        role,
        people: credits.value.filter((credit) => credit.role === role),
    })).filter((group) => group.people.length),
)

function photoPath(credit: Credit): string | undefined {
    const key = photoKey(credit.name)

    return key ? props.photos[key] : undefined
}

function photoUrl(credit: Credit): string | undefined {
    const path = photoPath(credit)

    if (path) {
        return urls[path]
    }

    return credit.thumb?.startsWith('http') ? credit.thumb : undefined
}

function progressOf(credit: Credit): number | undefined {
    const key = photoKey(credit.name)

    return key ? uploading[key] : undefined
}

watch(
    () => Object.values(props.photos),
    async (paths) => {
        for (const path of paths) {
            if (urls[path] !== undefined) {
                continue
            }

            urls[path] = ''

            try {
                urls[path] = await imageUrl(props.channelId, path)
            } catch {
                // The person icon stays.
            }
        }
    },
    { immediate: true },
)

async function add(role: CreditRole) {
    menuOpen.value = false
    credits.value.push({ name: '', role, character: null, guest: false, thumb: null })

    await nextTick()

    const inputs = document.querySelectorAll<HTMLInputElement>(`[data-credit-name="${role}"]`)

    inputs[inputs.length - 1]?.focus()
}

function remove(credit: Credit) {
    credits.value = credits.value.filter((other) => toRaw(other) !== toRaw(credit))
}

function changeRole(credit: Credit) {
    if (credit.role !== 'actor') {
        credit.character = null
        credit.guest = false
    }
}

function choosePhoto(credit: Credit) {
    photoFor.value = credit
    photoInput.value?.click()
}

async function onPhoto(event: Event) {
    const input = event.target as HTMLInputElement
    const file = input.files?.[0]
    const credit = photoFor.value
    const name = credit?.name.trim() ?? ''
    const key = photoKey(name)

    input.value = ''

    if (!file || !key) {
        return
    }

    uploading[key] = 0

    try {
        const sheet = await uploadPhoto(props.channelId, props.path, name, file, (share) => (uploading[key] = share))

        // A new file under the same name needs a new address.
        const path = sheet.photos[key]

        if (path) {
            delete urls[path]
        }

        emit('photos', sheet.photos)
    } catch (e) {
        indexStore.msgAlert('error', e instanceof Error ? e.message : String(e), 6)
    } finally {
        delete uploading[key]
    }
}
</script>
<template>
    <section class="flex flex-col gap-3.5" aria-labelledby="credits-title">
        <div class="flex flex-wrap items-end gap-3">
            <div class="flex-[1_1_300px] min-w-0">
                <h3 id="credits-title" class="m-0 text-[15px] font-semibold">
                    {{ episode ? t('storage.credits.episodeTitle') : t('storage.credits.title') }}
                </h3>
                <p class="m-0 mt-0.5 text-[13px] text-muted">
                    {{ episode ? t('storage.credits.episodeHint') : t('storage.credits.hint') }}
                </p>
            </div>
            <div ref="menu" class="relative" @keydown.esc.stop="menuOpen = false">
                <button
                    type="button"
                    class="inline-flex items-center gap-2 rounded-field border px-3.5 min-h-10 font-medium bg-accent-soft text-accent-text cursor-pointer"
                    :class="menuOpen ? 'border-accent' : 'border-accent/40'"
                    aria-haspopup="menu"
                    :aria-expanded="menuOpen"
                    @click="menuOpen = !menuOpen"
                >
                    <i class="bi bi-plus-lg" aria-hidden="true" />{{ t('storage.credits.add') }}
                </button>
                <div
                    v-if="menuOpen"
                    role="menu"
                    :aria-label="t('storage.credits.roleMenu')"
                    class="absolute end-0 mt-1.5 w-[280px] max-w-[calc(100vw-32px)] z-20 bg-raised border border-line-strong rounded-xl p-1.5 shadow-xl flex flex-col"
                >
                    <button
                        v-for="role in CREDIT_ROLES"
                        :key="role"
                        type="button"
                        role="menuitem"
                        class="flex items-center justify-between gap-3 px-2.5 min-h-9 rounded-field text-left bg-transparent border-0 cursor-pointer hover:bg-base-300 focus-visible:bg-base-300"
                        @click="add(role)"
                    >
                        {{ t(`storage.credits.roles.${role}`) }}
                        <span class="font-mono text-xs text-muted">{{ role }}</span>
                    </button>
                </div>
            </div>
        </div>

        <p v-if="!credits.length" class="m-0 text-[13px] text-muted">{{ t('storage.credits.empty') }}</p>

        <div v-for="group in groups" :key="group.role" class="flex flex-col gap-1">
            <div class="text-[11px] tracking-[0.08em] uppercase text-muted py-1">
                {{ t(`storage.credits.groups.${group.role}`) }}
            </div>
            <div
                v-for="credit in group.people"
                :key="keyOf(credit)"
                class="grid grid-cols-[48px_minmax(0,1fr)_36px] sm:grid-cols-[48px_minmax(0,1.4fr)_150px_minmax(0,1fr)_36px] gap-x-2.5 gap-y-2 items-center bg-base-200 border border-line rounded-[10px] px-2.5 py-2"
            >
                <div class="row-span-3 sm:row-span-1 self-start sm:self-center">
                    <div
                        v-if="progressOf(credit) !== undefined"
                        role="img"
                        :aria-label="
                            t('storage.credits.uploading', {
                                name: credit.name,
                                percent: Math.round((progressOf(credit) ?? 0) * 100),
                            })
                        "
                        class="w-12 h-12 rounded-full flex items-center justify-center"
                        :style="{
                            background: `conic-gradient(var(--color-accent) 0 ${(progressOf(credit) ?? 0) * 100}%, var(--color-line) 0 100%)`,
                        }"
                    >
                        <span
                            class="w-10 h-10 rounded-full bg-base-200 flex items-center justify-center font-mono text-[11px]"
                        >
                            {{ Math.round((progressOf(credit) ?? 0) * 100) }}%
                        </span>
                    </div>
                    <button
                        v-else-if="photoPath(credit) || photoUrl(credit)"
                        type="button"
                        class="w-12 h-12 rounded-full overflow-hidden border-0 p-0 bg-base-300 text-muted flex items-center justify-center cursor-pointer"
                        :aria-label="t('storage.credits.changePhoto', { name: credit.name })"
                        :title="t('storage.credits.changePhoto', { name: credit.name })"
                        @click="choosePhoto(credit)"
                    >
                        <img
                            v-if="photoUrl(credit)"
                            :src="photoUrl(credit)"
                            alt=""
                            class="w-full h-full object-cover"
                        />
                        <i v-else class="bi bi-person text-2xl" aria-hidden="true" />
                    </button>
                    <button
                        v-else
                        type="button"
                        class="w-12 h-12 rounded-full border-[1.5px] border-dashed border-line-strong bg-transparent p-0 text-muted flex flex-col items-center justify-center text-[10px] leading-tight cursor-pointer disabled:cursor-not-allowed disabled:opacity-50"
                        :disabled="!photoKey(credit.name)"
                        :aria-label="
                            credit.name.trim()
                                ? t('storage.credits.addPhoto', { name: credit.name })
                                : t('storage.credits.nameFirst')
                        "
                        :title="credit.name.trim() ? undefined : t('storage.credits.nameFirst')"
                        @click="choosePhoto(credit)"
                    >
                        <i class="bi bi-camera text-base" aria-hidden="true" />{{ t('storage.credits.photo') }}
                    </button>
                </div>

                <label class="field-label text-[11px]">
                    {{ t('storage.credits.name') }}
                    <input
                        v-model="credit.name"
                        class="field-input text-sm"
                        :data-credit-name="credit.role"
                        autocomplete="off"
                    />
                </label>

                <label class="field-label text-[11px] col-start-2 sm:col-start-3 sm:row-start-1">
                    {{ t('storage.credits.role') }}
                    <select v-model="credit.role" class="field-input text-sm" @change="changeRole(credit)">
                        <option v-for="role in CREDIT_ROLES" :key="role" :value="role">
                            {{ t(`storage.credits.roles.${role}`) }}
                        </option>
                    </select>
                </label>

                <div
                    v-if="credit.role === 'actor'"
                    class="col-start-2 sm:col-start-4 sm:row-start-1 flex flex-col gap-1"
                >
                    <label class="field-label text-[11px]">
                        {{ t('storage.credits.character') }}
                        <input v-model="credit.character" class="field-input text-sm" autocomplete="off" />
                    </label>
                </div>
                <div v-else class="hidden sm:block sm:col-start-4 sm:row-start-1" />

                <label
                    v-if="credit.role === 'actor'"
                    class="col-start-2 sm:col-start-4 flex items-center gap-1.5 text-[13px] text-base-content/85 -mt-1"
                >
                    <input v-model="credit.guest" type="checkbox" class="checkbox checkbox-xs checkbox-accent" />
                    {{ t('storage.credits.guest') }}
                </label>

                <button
                    type="button"
                    class="btn-quiet min-w-9 min-h-9 px-0 text-error col-start-3 row-start-1 sm:col-start-5 self-center"
                    :aria-label="t('storage.credits.remove', { name: credit.name || t('storage.credits.person') })"
                    @click="remove(credit)"
                >
                    <i class="bi bi-trash" aria-hidden="true" />
                </button>
            </div>
        </div>

        <div
            v-if="episode && programCredits.length"
            class="flex flex-wrap items-center gap-2 bg-base-200 border border-line rounded-[10px] px-3 py-2.5 text-[13px] text-muted"
        >
            <span class="text-base-content/85">{{ t('storage.credits.fromProgram') }}</span>
            <span
                v-for="credit in programCredits"
                :key="`${credit.role}-${credit.name}`"
                class="bg-base-300 rounded-full px-2.5 py-0.5 text-base-content"
            >
                {{ credit.name }} · {{ t(`storage.credits.roles.${credit.role}`).toLocaleLowerCase() }}
            </span>
            <button
                v-if="program"
                type="button"
                class="link-accent ms-auto min-h-8"
                @click="emit('openProgram', program)"
            >
                {{ t('storage.credits.editProgram') }}
            </button>
        </div>

        <p class="m-0 text-xs text-muted">
            {{ t('storage.credits.photoHint') }}
        </p>

        <input ref="photoInput" type="file" class="hidden" accept="image/jpeg,image/png,image/webp" @change="onPhoto" />
    </section>
</template>
