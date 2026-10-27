<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import SceneButtons from '@/components/graphics/SceneButtons.vue'
import PositionPad, { type Shares } from '@/components/settings/PositionPad.vue'
import { authFetch } from '@/composables/authFetch'
import { LOGO_BOX, LSHAPE_CORNERS, sharesOf } from '@/composables/graphics'
import { useAuth } from '@/stores/auth'
import { useConfig } from '@/stores/config'
import { useIndex } from '@/stores/index'
import { usePlaylist } from '@/stores/playlist'

/**
 * Graphics at hand under the Player's preview: the logo on and off and where
 * it is, the L-shape in and out and its corner, and the quick scenes. A move
 * waits for a confirmation, so a slip of the mouse changes nothing on air.
 */
interface LogoStatus {
    hidden: boolean
    replacement: string | null
    position?: string | null
}

interface LShapeStatus {
    on: boolean
    ready: boolean
    replacement: string | null
    position?: string | null
    scale?: number | null
}

/** How often the state is read again, as other people or encoders change it. */
const REFRESH_MS = 5000

const { t } = useI18n()
const authStore = useAuth()
const configStore = useConfig()
const indexStore = useIndex()
const playlistStore = usePlaylist()

const logo = ref<LogoStatus | null>(null)
const lshape = ref<LShapeStatus | null>(null)
const busy = ref(false)
const open = ref<'logo' | 'lshape' | null>(null)
const logoDraft = ref<Shares | null>(null)
const cornerDraft = ref<string | null>(null)
let timer: ReturnType<typeof setInterval> | undefined

/** Like the transport, the graphics start locked while the channel is on air. */
const locked = ref(playlistStore.playoutIsRunning)
/** Nothing here can change the air: locked, or a command on its way. */
const blocked = computed(() => locked.value || busy.value)

watch(
    () => playlistStore.playoutIsRunning,
    (running) => (locked.value = running),
)
watch(locked, (on) => on && closePanel())

const channelId = computed(() => configStore.channels[configStore.i]?.id)
const processing = computed(() => configStore.playoutSaved?.processing)
const logoShares = computed(() => sharesOf(logo.value?.position ?? processing.value?.logo_position))
const lshapeCorner = computed(() => lshape.value?.position ?? processing.value?.lshape_corner ?? 'top-right')
const logoOn = computed(() => Boolean(logo.value && !logo.value.hidden))

function headers() {
    return { ...configStore.contentType, ...authStore.authHeader }
}

async function load() {
    if (!channelId.value) {
        return
    }

    const [logoState, lshapeState] = await Promise.allSettled([
        authFetch<LogoStatus>(`/api/control/${channelId.value}/logo`, { headers: headers() }),
        authFetch<LShapeStatus>(`/api/control/${channelId.value}/lshape`, { headers: headers() }),
    ])

    logo.value = logoState.status === 'fulfilled' ? logoState.value : null
    lshape.value = lshapeState.status === 'fulfilled' ? lshapeState.value : null
}

async function post<T>(route: 'logo' | 'lshape', body: Record<string, unknown>): Promise<T | null> {
    busy.value = true

    try {
        return await authFetch<T>(`/api/control/${channelId.value}/${route}`, {
            method: 'POST',
            headers: headers(),
            body: JSON.stringify(body),
        })
    } catch (error: any) {
        indexStore.msgAlert('error', error?.data ?? (error instanceof Error ? error.message : String(error)), 4)
        return null
    } finally {
        busy.value = false
    }
}

async function toggleLogo() {
    logo.value = (await post<LogoStatus>('logo', { action: logoOn.value ? 'hide' : 'show' })) ?? logo.value
}

async function toggleLShape() {
    lshape.value = (await post<LShapeStatus>('lshape', { action: lshape.value?.on ? 'off' : 'on' })) ?? lshape.value
}

async function confirmLogo() {
    if (logoDraft.value) {
        logo.value = (await post<LogoStatus>('logo', { action: 'move', ...logoDraft.value })) ?? logo.value
    }

    closePanel()
}

async function logoBack() {
    logo.value = (await post<LogoStatus>('logo', { action: 'move', position: 'default' })) ?? logo.value
    closePanel()
}

async function confirmCorner() {
    if (cornerDraft.value) {
        lshape.value =
            (await post<LShapeStatus>('lshape', {
                action: 'move',
                position: cornerDraft.value,
                scale: lshape.value?.scale ?? undefined,
            })) ?? lshape.value
    }

    closePanel()
}

function togglePanel(panel: 'logo' | 'lshape') {
    logoDraft.value = null
    cornerDraft.value = null
    open.value = open.value === panel ? null : panel
}

function closePanel() {
    open.value = null
    logoDraft.value = null
    cornerDraft.value = null
}

onMounted(() => {
    load()
    timer = setInterval(() => {
        // An open panel keeps what the person is looking at.
        if (!open.value && !busy.value) {
            load()
        }
    }, REFRESH_MS)
})
onBeforeUnmount(() => clearInterval(timer))
watch(channelId, () => {
    closePanel()
    locked.value = playlistStore.playoutIsRunning
    load()
})
</script>
<template>
    <section
        :aria-label="t('graphics.quick.label')"
        class="card-box relative px-3 py-2.5 flex flex-wrap items-center gap-2"
        @keydown.esc="closePanel"
    >
        <span class="text-xs font-semibold tracking-[0.06em] text-muted uppercase me-1">{{
            t('graphics.quick.label')
        }}</span>

        <!-- Logo: on/off and where it is. -->
        <div class="inline-flex border border-line-strong rounded-field overflow-hidden">
            <button
                type="button"
                class="min-h-11 px-3 inline-flex items-center gap-2 text-sm cursor-pointer disabled:opacity-50"
                :class="logoOn ? 'bg-ok-soft text-ok font-semibold' : 'bg-base-100'"
                :aria-pressed="logoOn"
                :disabled="blocked || !logo"
                @click="toggleLogo"
            >
                <span class="dot" :class="logoOn ? 'bg-ok' : 'bg-subtle'" aria-hidden="true" />{{
                    t('graphics.quick.logo')
                }}
            </button>
            <button
                type="button"
                class="min-h-11 w-10 border-l border-line-strong inline-flex items-center justify-center cursor-pointer disabled:opacity-50"
                :class="open === 'logo' ? 'bg-accent-soft' : 'bg-base-100'"
                :aria-label="t('graphics.quick.moveLogo')"
                :aria-expanded="open === 'logo'"
                :disabled="blocked || !logo"
                @click="togglePanel('logo')"
            >
                <i class="bi bi-arrows-move" aria-hidden="true" />
            </button>
        </div>

        <!-- L-shape: in/out and its corner. -->
        <div v-if="lshape?.ready" class="inline-flex border border-line-strong rounded-field overflow-hidden">
            <button
                type="button"
                class="min-h-11 px-3 inline-flex items-center gap-2 text-sm cursor-pointer disabled:opacity-50"
                :class="lshape.on ? 'bg-accent-soft text-accent-text font-semibold' : 'bg-base-100'"
                :aria-pressed="lshape.on"
                :disabled="blocked"
                @click="toggleLShape"
            >
                <i class="bi bi-aspect-ratio" aria-hidden="true" />{{ t('graphics.quick.lshape') }}
            </button>
            <button
                type="button"
                class="min-h-11 w-10 border-l border-line-strong inline-flex items-center justify-center cursor-pointer disabled:opacity-50"
                :class="open === 'lshape' ? 'bg-accent-soft' : 'bg-base-100'"
                :aria-label="t('graphics.quick.lshapeCorner')"
                :aria-expanded="open === 'lshape'"
                :disabled="blocked"
                @click="togglePanel('lshape')"
            >
                <i class="bi bi-chevron-down" aria-hidden="true" />
            </button>
        </div>

        <SceneButtons compact :disabled="locked" @applied="load" />

        <label
            class="ms-auto flex items-center gap-2 min-h-11 cursor-pointer text-sm"
            :class="locked ? 'text-base-content' : 'text-muted'"
        >
            <i class="bi" :class="locked ? 'bi-lock' : 'bi-unlock'" aria-hidden="true" />
            {{ t('graphics.quick.locked') }}
            <input v-model="locked" type="checkbox" role="switch" class="toggle toggle-sm toggle-accent" />
        </label>
        <RouterLink
            :to="{ name: 'graphics-live', query: { channel: channelId } }"
            class="link-accent text-sm inline-flex items-center gap-1"
        >
            {{ t('graphics.quick.open') }} <i class="bi bi-arrow-right" aria-hidden="true" />
        </RouterLink>

        <!-- Move the logo: picked on the small screen, then confirmed. -->
        <div
            v-if="open === 'logo'"
            role="dialog"
            :aria-label="t('graphics.quick.moveLogo')"
            class="absolute z-20 left-3 bottom-[calc(100%+8px)] w-[min(320px,calc(100vw-48px))] card-box shadow-lg p-3.5 flex flex-col gap-2.5"
        >
            <strong class="text-sm">{{ t('graphics.quick.moveLogo') }}</strong>
            <PositionPad
                class="max-w-none"
                :shares="logoDraft ?? logoShares"
                :ghost="logoDraft ? logoShares : null"
                :width="LOGO_BOX.width"
                :height="LOGO_BOX.height"
                :label="t('settings.logo.remote.logoPad')"
                :disabled="busy"
                @place="logoDraft = $event"
            />
            <span class="text-xs text-muted">{{ t('settings.logo.remote.padHint') }}</span>
            <div class="flex items-center justify-between gap-2 text-[13px] tabular">
                <span>
                    {{ logoShares.x }}% × {{ logoShares.y }}%
                    <template v-if="logoDraft">
                        →
                        <strong class="text-accent-text">{{ logoDraft.x }}% × {{ logoDraft.y }}%</strong>
                    </template>
                </span>
                <button
                    v-if="logo?.position"
                    class="bg-transparent border-0 p-0 text-accent-text cursor-pointer hover:underline"
                    :disabled="busy"
                    @click="logoBack"
                >
                    {{ t('settings.logo.remote.moveBack') }}
                </button>
            </div>
            <div class="flex gap-2">
                <button class="btn-main flex-1 min-h-10" :disabled="busy || !logoDraft" @click="confirmLogo">
                    <i class="bi bi-check2" aria-hidden="true" />{{ t('settings.logo.remote.confirmMove') }}
                </button>
                <button class="btn-quiet" @click="closePanel">{{ t('settings.logo.remote.cancelMove') }}</button>
            </div>
        </div>

        <!-- The L-shape corner, then confirmed. -->
        <div
            v-if="open === 'lshape' && lshape"
            role="dialog"
            :aria-label="t('graphics.quick.lshapeCorner')"
            class="absolute z-20 left-3 sm:left-48 bottom-[calc(100%+8px)] w-[min(280px,calc(100vw-48px))] card-box shadow-lg p-3.5 flex flex-col gap-2.5"
        >
            <strong class="text-sm">{{ t('graphics.quick.lshapeCorner') }}</strong>
            <div
                role="group"
                :aria-label="t('settings.logo.remote.lshapePad')"
                class="grid grid-cols-2 gap-1.5 aspect-video rounded-md border border-line-strong bg-field p-1.5"
            >
                <button
                    v-for="corner in LSHAPE_CORNERS"
                    :key="corner"
                    type="button"
                    class="rounded-sm border cursor-pointer"
                    :class="
                        (cornerDraft ?? lshapeCorner) === corner
                            ? cornerDraft
                                ? 'bg-accent/40 border-accent border-dashed border-2'
                                : 'bg-accent border-accent'
                            : cornerDraft && lshapeCorner === corner
                              ? 'bg-base-100 border-muted border-dashed border-2'
                              : 'bg-base-100 border-line-strong hover:border-accent'
                    "
                    :aria-pressed="(cornerDraft ?? lshapeCorner) === corner"
                    :aria-label="t(`lshape.corners.${corner}`)"
                    :title="t(`lshape.corners.${corner}`)"
                    @click="cornerDraft = corner === lshapeCorner ? null : corner"
                />
            </div>
            <span class="text-xs text-muted">{{ t('settings.logo.remote.lshapePadHint') }}</span>
            <div class="flex gap-2">
                <button class="btn-main flex-1 min-h-10" :disabled="busy || !cornerDraft" @click="confirmCorner">
                    <i class="bi bi-check2" aria-hidden="true" />{{
                        lshape.on ? t('settings.logo.remote.confirmMove') : t('settings.logo.remote.confirmPlace')
                    }}
                </button>
                <button class="btn-quiet" @click="closePanel">{{ t('settings.logo.remote.cancelMove') }}</button>
            </div>
        </div>
    </section>
</template>
