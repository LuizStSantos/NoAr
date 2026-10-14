import { useI18n } from 'vue-i18n'

const HARDWARE: [RegExp, string][] = [
    [/nvenc/, 'NVIDIA'],
    [/qsv/, 'Intel QSV'],
    [/vaapi/, 'VAAPI'],
    [/v4l2m2m/, 'V4L2'],
]

/** Short names for encoders: "H.264 (libx264)", "H.264 · NVIDIA (h264_nvenc)". */
export function useCodecLabels() {
    const { t, te } = useI18n()

    function codecLabel(codec: CodecOption): string {
        let label = (codec.display_name || codec.name).trim()
        const first = label.split(' ')[0] ?? ''

        // "libx264 H.264 / AVC / ..." repeats the library name first.
        if (first.toLowerCase().startsWith('lib') && label.includes(' ')) {
            label = label.slice(first.length).trim()
        }

        label = (label.split(' / ')[0] ?? label).replace(/\s*\(.*\)\s*$/, '').trim() || codec.name

        const hardware = codec.hardware ? HARDWARE.find(([pattern]) => pattern.test(codec.name))?.[1] : undefined

        if (hardware && !label.includes(hardware)) {
            label = `${label} · ${hardware}`
        }

        return label === codec.name ? label : `${label} (${codec.name})`
    }

    function settingLabel(setting: EncoderSetting): string {
        const key = `settings.encoder.${setting.key}`

        return te(key) ? t(key) : setting.label
    }

    return { codecLabel, settingLabel }
}
