import { ref, watch } from 'vue'

/** Discover server video cards lazily when the SDI output is selected. */
export function useVideoDevices(mode: () => string | undefined, discover: () => Promise<VideoDevices>) {
    /** Video cards on the server, looked up when the SDI destination is shown. */
    const videoDevices = ref<VideoDevices | null>(null)
    const devicesError = ref('')
    const loadingDevices = ref(false)

    async function loadVideoDevices() {
        loadingDevices.value = true
        devicesError.value = ''

        try {
            videoDevices.value = await discover()
        } catch (err) {
            devicesError.value = err instanceof Error ? err.message : String(err)
        } finally {
            loadingDevices.value = false
        }
    }

    watch(
        mode,
        (mode) => {
            if (mode === 'sdi' && !videoDevices.value && !loadingDevices.value) {
                loadVideoDevices()
            }
        },
        { immediate: true },
    )

    return { videoDevices, devicesError, loadingDevices, loadVideoDevices }
}
