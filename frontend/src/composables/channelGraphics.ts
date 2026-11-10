import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import { AuthFetchError } from '@/composables/authFetch'
import { type ChannelGraphic, type GraphicOnAir, useGraphicsApi } from '@/composables/graphicsTemplates'
import { useConfig } from '@/stores/config'
import { useIndex } from '@/stores/index'

/** Shared model status for the operator screen and the Player's read-only line. */
export function useChannelGraphics() {
    const config = useConfig()
    const index = useIndex()
    const { t } = useI18n()
    const api = useGraphicsApi()
    const channelId = computed(() => config.channels[config.i]?.id)
    const models = ref<ChannelGraphic[]>([])
    const onAir = ref<GraphicOnAir[]>([])
    const busy = ref<number | null>(null)
    const unavailable = ref(false)
    let version = 0
    let timer: ReturnType<typeof setInterval> | undefined

    async function refresh() {
        if (!channelId.value || busy.value !== null) {
            return
        }

        const request = ++version

        try {
            const [assigned, temporary] = await Promise.all([api.channel(), api.onAir()])

            if (request === version) {
                models.value = assigned
                onAir.value = temporary
                unavailable.value = false
            }
        } catch {
            if (request === version) {
                unavailable.value = true
            }
        }
    }

    async function control(model: ChannelGraphic, on: boolean) {
        if (busy.value !== null) {
            return
        }

        const targetChannel = channelId.value
        const operation = ++version
        busy.value = model.template.id

        try {
            const updated = await api.controlChannel(model.template.id, on)

            if (targetChannel === channelId.value && operation === version) {
                models.value = updated
                unavailable.value = false
            }
        } catch (failure) {
            index.msgAlert(
                'error',
                failure instanceof AuthFetchError && typeof failure.data === 'string'
                    ? failure.data
                    : t('graphics.templates.takeFailed'),
                4,
            )
        } finally {
            if (targetChannel === channelId.value && operation === version) {
                busy.value = null
            }
        }
    }

    watch(channelId, () => {
        version += 1
        models.value = []
        onAir.value = []
        busy.value = null
        unavailable.value = false
        void refresh()
    })
    onMounted(() => {
        void refresh()
        timer = setInterval(refresh, 2000)
    })
    onBeforeUnmount(() => {
        clearInterval(timer)
        version += 1
    })

    return { models, onAir, busy, unavailable, refresh, control }
}
