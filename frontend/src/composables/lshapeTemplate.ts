/**
 * A base image for whoever designs an L-shape graphic: the output's size,
 * the space the picture takes (with its exact size in pixels), and the title
 * safe area. Opened in an image editor, the band is drawn around that space.
 */
export interface LShapeLayout {
    width: number
    height: number
    /** Picture size, in percent of the screen. */
    scale: number
    corner: string
}

/** The picture's rectangle, as the engine places it (even pixels). */
export function lshapeWindow({ width, height, scale, corner }: LShapeLayout) {
    const even = (value: number) => Math.max(2, Math.round(value) & ~1)
    const w = even((width * scale) / 100)
    const h = even((height * scale) / 100)

    return {
        x: corner.endsWith('left') ? 0 : (width - w) & ~1,
        y: corner.startsWith('top') ? 0 : (height - h) & ~1,
        width: w,
        height: h,
    }
}

export function lshapeTemplateName(layout: LShapeLayout): string {
    return `lshape-base-${layout.width}x${layout.height}-${layout.corner}-${layout.scale}.png`
}

/** The base as a PNG, with `labels` written on it. */
export async function lshapeTemplate(
    layout: LShapeLayout,
    labels: { band: string; video: string; safe: string },
): Promise<Blob> {
    const canvas = document.createElement('canvas')
    canvas.width = layout.width
    canvas.height = layout.height
    const context = canvas.getContext('2d')

    if (!context) {
        throw new Error('canvas')
    }

    const unit = layout.height / 1080
    const window = lshapeWindow(layout)

    // The band: where the graphic goes.
    context.fillStyle = '#d6d9de'
    context.fillRect(0, 0, layout.width, layout.height)
    context.strokeStyle = 'rgba(0,0,0,0.08)'
    context.lineWidth = 1
    for (let x = 0; x < layout.width; x += 40 * unit) {
        context.beginPath()
        context.moveTo(x, 0)
        context.lineTo(x, layout.height)
        context.stroke()
    }
    for (let y = 0; y < layout.height; y += 40 * unit) {
        context.beginPath()
        context.moveTo(0, y)
        context.lineTo(layout.width, y)
        context.stroke()
    }

    // The picture's space: the video covers it.
    context.fillStyle = '#ff00ff'
    context.fillRect(window.x, window.y, window.width, window.height)
    context.fillStyle = '#ffffff'
    context.textAlign = 'center'
    context.textBaseline = 'middle'
    context.font = `bold ${Math.round(44 * unit)}px sans-serif`
    context.fillText(labels.video, window.x + window.width / 2, window.y + window.height / 2 - 30 * unit)
    context.font = `${Math.round(30 * unit)}px sans-serif`
    context.fillText(
        `${window.width}×${window.height} px · x ${window.x}, y ${window.y}`,
        window.x + window.width / 2,
        window.y + window.height / 2 + 30 * unit,
    )

    // Title safe area (90%): text in the band stays inside it.
    const safe = 0.9
    context.setLineDash([12 * unit, 8 * unit])
    context.strokeStyle = '#1f6feb'
    context.lineWidth = 3 * unit
    context.strokeRect(
        (layout.width * (1 - safe)) / 2,
        (layout.height * (1 - safe)) / 2,
        layout.width * safe,
        layout.height * safe,
    )
    context.setLineDash([])

    // Where the band is, in words.
    const bandLabel = { x: 0, y: 0 }
    bandLabel.x = layout.corner.endsWith('left') ? (window.width + layout.width) / 2 : (layout.width - window.width) / 2
    bandLabel.y = layout.corner.startsWith('top')
        ? (window.height + layout.height) / 2
        : (layout.height - window.height) / 2
    context.fillStyle = '#3a3f47'
    context.font = `bold ${Math.round(34 * unit)}px sans-serif`
    context.fillText(labels.band, layout.width / 2, bandLabel.y)
    context.font = `${Math.round(24 * unit)}px sans-serif`
    context.fillStyle = '#1f6feb'
    context.fillText(labels.safe, layout.width / 2, bandLabel.y + 40 * unit)

    return new Promise((resolve, reject) =>
        canvas.toBlob((blob) => (blob ? resolve(blob) : reject(new Error('png'))), 'image/png'),
    )
}
