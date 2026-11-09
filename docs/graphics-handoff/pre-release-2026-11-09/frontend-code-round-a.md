# Frontend code review — round A

Reviewer: `noar-frontend-reviewer`. Independent read-only review of `f925f608671c17c32f4788993b84d044a6621907` against `origin/main` (`dd4529e2935b07d7a5bf8deb764421e4d201fc41`), including the current display-localization patch and `graphicsDisplayNames.ts`.

Read repository instructions, frontend reviewer definition, repository map, and both audit quality-gate documents. Inspected graphics routes, Composer and child ownership, library and on-air state, sources, fonts and image-upload clients, supporting server contracts, and extracted playlist/storage/report/schedule responsibilities. This is static evidence only. Browser interaction, lint/type check and builds: NOT_RUN in this reviewer task; staging evidence is owned by the lead. No source changes, Git mutations, servers or builds. No ballot or release authority.

## FC-01 — HIGH — Channel initialization can discard Composer edits before its update guard runs

- Evidence: VERIFIED_STATIC; browser reproduction NOT_VERIFIED.
- References: `frontend/src/components/graphics/GraphicsComposer.vue:592`, `frontend/src/components/graphics/GraphicsComposer.vue:600`, `frontend/src/components/graphics/GraphicsComposer.vue:605`, `frontend/src/views/GraphicsView.vue:175`, `frontend/src/router/index.ts:310`, `frontend/src/stores/config.ts:51`.
- The header channel selector pushes a query-only route update. The global `beforeEach` initializes the destination channel before the reused component's `onBeforeRouteUpdate` guard. Initialization changes the reactive selected channel, while the RouterView is keyed by that value and Composer also watches it to reload. Consequently the dirty document can be remounted/reloaded before the discard decision. The reload sets `busy`, and the update guard explicitly skips confirmation while busy. Returning false from a later guard also does not roll back the already-mutated global channel.
- Reproduction for staging: edit an existing Composer layer without saving, change the channel using the global header selector, and observe whether the discard dialog appears and whether cancelling preserves the exact draft, route and selected channel. Test with two configured channels and delayed channel configuration responses.
- Impact: unsaved operator work can disappear during an ordinary channel switch; route and API channel ownership may diverge after cancellation.
- Fix direction: resolve the discard decision before committing selected-channel state; do not let speculative router initialization remount or reload the active editor.

## FC-02 — HIGH — A completed save can take ownership of a different document or channel

- Evidence: VERIFIED_STATIC; browser reproduction NOT_VERIFIED.
- References: `frontend/src/components/graphics/GraphicsComposer.vue:347`, `frontend/src/components/graphics/GraphicsComposer.vue:351`, `frontend/src/components/graphics/GraphicsComposer.vue:366`, `frontend/src/components/graphics/GraphicsComposer.vue:600`, `frontend/src/components/graphics/GraphicsComposer.vue:624`, `frontend/src/composables/graphicsTemplates.ts:208`.
- Loading uses `loadVersion`, but saving captures neither that generation nor its initiating channel/document. After the save awaits, it writes the current draft/baseline, reads the current assignment, sends an optional assignment through an API whose base is the current global channel, and replaces the route using the current channel. Navigation is allowed during saving; a same-component route update expressly skips discard confirmation while busy. Unmount increments `loadVersion`, but the save never checks it.
- Reproduction for staging: delay the save response for template A; navigate to another template or channel; release the response. The stale continuation can replace the new document and navigate back, or call A's assignment endpoint under the newly selected channel. Even after leaving Composer, it can route the operator back into it.
- Impact: new draft state/navigation can be overwritten by an older save, and the follow-up request may address the wrong channel. Server channel ownership should reject cross-channel template IDs, but that does not protect client state.
- Fix direction: snapshot the input, assignment, channel and generation; bind every request to that channel and ignore obsolete continuations before state changes, follow-up mutations or navigation.

## FC-03 — MEDIUM — Save overwrites changes made through controls that stay enabled

- Evidence: VERIFIED_STATIC; browser reproduction NOT_VERIFIED.
- References: `frontend/src/components/graphics/GraphicsComposer.vue:354`, `frontend/src/components/graphics/GraphicsComposer.vue:364`, `frontend/src/components/graphics/GraphicsComposer.vue:633`, `frontend/src/components/graphics/GraphicsComposer.vue:679`, `frontend/src/components/graphics/GraphicsComposer.vue:683`, `frontend/src/components/graphics/GraphicsComposer.vue:697`, `frontend/src/components/graphics/GraphicsComposer.vue:722`, `frontend/src/components/graphics/GraphicsComposer.vue:732`.
- Only the Composer grid receives pointer blocking while busy. The name/header, save-form group, assignment and field mappings remain editable, and keyboard focus in the grid is not disabled by `pointer-events-none`. The request serializes the original draft immediately; the successful response then replaces the entire draft and marks that replacement as the clean baseline. Any name/group/mapping edits entered while awaiting it are silently removed. Assignment also reads live state across its separate awaited request.
- Reproduction for staging: delay an existing template's PUT response, change its name/group or field mapping, then release the response. The new edit disappears and the document is reported saved.
- Impact: slow saves silently lose edits or leave the displayed assignment inconsistent with what was submitted.
- Fix direction: make document and assignment controls actually inert for the save, or preserve edits by revision and mark only the submitted snapshot as saved.

## FC-04 — MEDIUM — A delayed channel-editor lookup redirects after the operator leaves

- Evidence: VERIFIED_STATIC; browser reproduction NOT_VERIFIED.
- References: `frontend/src/components/graphics/GraphicsChannelEditor.vue:29`, `frontend/src/components/graphics/GraphicsChannelEditor.vue:31`, `frontend/src/components/graphics/GraphicsChannelEditor.vue:42`, `frontend/src/components/graphics/GraphicsChannelEditor.vue:51`.
- `openModel` checks a request version, but that version changes only when its watcher or retry starts another lookup. Unmount does not invalidate it. A request started when opening Logo/Ticker/L-shape therefore still passes the check after the operator navigates to On air or Templates, and executes `router.replace`. It also derives destination channel/kind from live state rather than the original request.
- Reproduction for staging: delay `/api/graphics/<id>/channel`, open a channel editor, navigate to On air while waiting, and release the lookup. The abandoned editor redirects to Composer.
- Impact: an obsolete lookup unexpectedly interrupts the operator's current screen.
- Fix direction: invalidate/cancel pending lookups on unmount and confirm route/channel ownership before redirecting.

## FC-05 — MEDIUM — Library on-air badges never follow takes or automatic expiry

- Evidence: VERIFIED_STATIC; browser reproduction NOT_VERIFIED.
- References: `frontend/src/components/graphics/GraphicsLibrary.vue:145`, `frontend/src/components/graphics/GraphicsLibrary.vue:151`, `frontend/src/components/graphics/GraphicsLibrary.vue:165`, `frontend/src/components/graphics/GraphicsLibrary.vue:173`, `frontend/src/components/graphics/GraphicsLibrary.vue:301`.
- The initial library load stores `onAir`; its two-second polling refresh fetches only assigned channel models. The temporary on-air badge reads the untouched initial array. No timer or other update refreshes temporary takes, so a short-duration take remains marked on air indefinitely after expiring, and takes started by another operator never acquire a badge until the page reloads.
- Reproduction for staging: take a short-duration ordinary template, open Templates before it expires, wait past expiry, and inspect its badge; also start a take from another session while the library remains open.
- Impact: the library gives operators persistently incorrect on-air status despite polling the server.
- Fix direction: refresh temporary takes with the polled status and guard both result sets with the same generation.

No additional actionable regression was established in the display-name projection helper or the extracted playlist/storage/report responsibilities. Existing or unproven concerns were not promoted to findings. Round B peer challenge and a later independent ballot remain pending.
