/**
 * Browser features that only exist on secure pages (HTTPS or localhost).
 * NoAr is usually opened over plain HTTP at the server's address, where they
 * are missing, so each helper falls back to what works there.
 */

/** A random v4 UUID; `crypto.randomUUID` is missing over plain HTTP. */
export function randomId(): string {
    if (typeof crypto.randomUUID === 'function') {
        return crypto.randomUUID()
    }

    const bytes = crypto.getRandomValues(new Uint8Array(16))

    bytes[6] = ((bytes[6] ?? 0) & 0x0f) | 0x40
    bytes[8] = ((bytes[8] ?? 0) & 0x3f) | 0x80

    const hex = Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('')

    return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`
}

/** Put text on the clipboard; `navigator.clipboard` is missing over plain HTTP. */
export async function copyText(text: string): Promise<void> {
    if (window.isSecureContext && navigator.clipboard) {
        await navigator.clipboard.writeText(text)
        return
    }

    const area = document.createElement('textarea')

    area.value = text
    area.setAttribute('readonly', '')
    area.style.position = 'fixed'
    area.style.opacity = '0'
    document.body.appendChild(area)
    area.select()

    try {
        // The old copy command is the only one left on insecure pages.
        if (!document.execCommand('copy')) {
            throw new Error('Copy refused')
        }
    } finally {
        area.remove()
    }
}
