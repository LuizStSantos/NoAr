import { nextTick, onBeforeUnmount, onMounted, watch, type Ref } from 'vue'

const FOCUSABLE = [
    'a[href]',
    'button:not([disabled])',
    'input:not([disabled]):not([type="hidden"])',
    'select:not([disabled])',
    'textarea:not([disabled])',
    '[tabindex]:not([tabindex="-1"])',
    '[contenteditable="true"]',
].join(',')

/** Open dialogs, innermost last. Only the innermost one handles keys. */
const openDialogs: symbol[] = []

type DialogFocusOptions = {
    /** Whether the dialog is shown; leave out for dialogs that exist only while shown. */
    open?: () => boolean
    /** Called on Escape; leave out for dialogs that must not be dismissed. */
    onEscape?: () => void
}

/**
 * Keeps keyboard focus inside a modal dialog: it moves into the dialog when it
 * opens, Tab and Shift+Tab cycle within it, Escape closes it, and focus goes
 * back to the control that opened it when it closes. The dialog element needs
 * `tabindex="-1"` so it can take focus itself when it has nothing focusable.
 */
export function useDialogFocus(dialog: Ref<HTMLElement | null | undefined>, options: DialogFocusOptions = {}) {
    const id = Symbol('dialog')
    let opener: HTMLElement | null = null
    let active = false

    function focusables(): HTMLElement[] {
        if (!dialog.value) {
            return []
        }

        return [...dialog.value.querySelectorAll<HTMLElement>(FOCUSABLE)].filter(
            (element) => element.getClientRects().length > 0,
        )
    }

    function onKey(event: KeyboardEvent) {
        if (openDialogs[openDialogs.length - 1] !== id || !dialog.value) {
            return
        }

        if (event.key === 'Escape') {
            if (options.onEscape) {
                event.preventDefault()
                options.onEscape()
            }
            return
        }

        if (event.key !== 'Tab') {
            return
        }

        const items = focusables()

        if (!items.length) {
            event.preventDefault()
            dialog.value.focus()
            return
        }

        const index = items.indexOf(document.activeElement as HTMLElement)

        if (event.shiftKey && index <= 0) {
            event.preventDefault()
            items[items.length - 1]?.focus()
        } else if (!event.shiftKey && (index === -1 || index === items.length - 1)) {
            event.preventDefault()
            items[0]?.focus()
        }
    }

    async function activate() {
        if (active) {
            return
        }

        active = true
        opener = document.activeElement instanceof HTMLElement ? document.activeElement : null
        openDialogs.push(id)
        window.addEventListener('keydown', onKey)

        await nextTick()

        const element = dialog.value

        // The dialog may have focused one of its own fields already.
        if (!active || !element || element.contains(document.activeElement)) {
            return
        }

        ;(element.querySelector<HTMLElement>('[autofocus]') ?? element).focus()
    }

    function deactivate() {
        if (!active) {
            return
        }

        active = false
        openDialogs.splice(openDialogs.indexOf(id), 1)
        window.removeEventListener('keydown', onKey)

        if (opener?.isConnected) {
            opener.focus()
        }
        opener = null
    }

    if (options.open) {
        watch(options.open, (open) => (open ? activate() : deactivate()), { immediate: true })
    } else {
        onMounted(activate)
    }

    onBeforeUnmount(deactivate)
}
