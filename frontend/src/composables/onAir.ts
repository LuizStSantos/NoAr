import dayjs from 'dayjs'
import timezone from 'dayjs/plugin/timezone.js'
import utc from 'dayjs/plugin/utc.js'

import { computed, onBeforeUnmount, onMounted, ref } from 'vue'

import { fileName } from '@/composables/player'
import { useConfig } from '@/stores/config'
import { usePlaylist } from '@/stores/playlist'
import { useProgrammes } from '@/stores/programmes'

dayjs.extend(utc)
dayjs.extend(timezone)

const now = ref(Date.now())
let ticker: ReturnType<typeof setInterval> | undefined
let users = 0

/** What is on air right now, in terms of the playlist the player shows. */
export function useOnAir() {
    const configStore = useConfig()
    const playlistStore = usePlaylist()
    const programmes = useProgrammes()

    onMounted(() => {
        users += 1

        if (!ticker) {
            now.value = Date.now()
            ticker = setInterval(() => (now.value = Date.now()), 1000)
        }
    })

    onBeforeUnmount(() => {
        users -= 1

        if (users <= 0 && ticker) {
            clearInterval(ticker)
            ticker = undefined
        }
    })

    const channelNow = computed(() => dayjs(now.value).tz(configStore.timezone || 'UTC'))
    const secondsOfDay = computed(
        () => channelNow.value.hour() * 3600 + channelNow.value.minute() * 60 + channelNow.value.second(),
    )
    const dayStart = computed(() => configStore.playout.playlist?.startInSec ?? 0)
    const beforeDayStart = computed(() => secondsOfDay.value < dayStart.value)

    /** Date of the playlist that is on air: yesterday's before the day start. */
    const onAirDate = computed(() =>
        (beforeDayStart.value ? channelNow.value.subtract(1, 'day') : channelNow.value).format('YYYY-MM-DD'),
    )

    /** Now in playlist seconds, which continue past 86400 after midnight. */
    const playlistNow = computed(() => secondsOfDay.value + (beforeDayStart.value ? 86400 : 0))

    const viewingOnAir = computed(
        () => playlistStore.listDate === onAirDate.value || Boolean(configStore.playout.playlist?.infinit),
    )

    /** Index of the clip on air in the list shown, or -1. */
    const onAirIndex = computed(() => {
        const item = playlistStore.playlist[playlistStore.currentIndex]

        return playlistStore.playoutIsRunning &&
            !playlistStore.ingestRuns &&
            viewingOnAir.value &&
            item &&
            item.source === playlistStore.current.source
            ? playlistStore.currentIndex
            : -1
    })

    const clipLength = computed(() => {
        const item = playlistStore.playlist[onAirIndex.value]

        if (item) {
            return Math.max(0, item.out - item.in)
        }

        return Math.max(0, (playlistStore.current.out ?? 0) - (playlistStore.current.in ?? 0))
    })
    const clipPosition = computed(() => Math.min(clipLength.value, Math.max(0, playlistStore.elapsedSec)))
    const clipRemaining = computed(() => Math.max(0, clipLength.value - clipPosition.value))

    const currentGroup = computed(() => (onAirIndex.value >= 0 ? programmes.groupOf(onAirIndex.value) : undefined))
    const nextGroup = computed(() => {
        if (!currentGroup.value) {
            return programmes.groups.find((group) => group.start > playlistNow.value)
        }

        const index = programmes.groups.indexOf(currentGroup.value)

        return programmes.groups[index + 1]
    })

    /** The program on air, as the guide names it. */
    const currentTitle = computed(() => {
        if (currentGroup.value) {
            return currentGroup.value.title
        }

        const source = playlistStore.current.source ?? ''
        const parts = source.split('/').filter(Boolean)

        return playlistStore.current.title || (parts.length > 1 ? parts[parts.length - 2] : '') || fileName(source)
    })

    return {
        now,
        channelNow,
        secondsOfDay,
        playlistNow,
        onAirDate,
        viewingOnAir,
        onAirIndex,
        clipLength,
        clipPosition,
        clipRemaining,
        currentGroup,
        nextGroup,
        currentTitle,
    }
}
