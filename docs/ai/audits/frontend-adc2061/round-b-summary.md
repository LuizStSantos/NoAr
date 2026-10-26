# Round B relay summary (lead-relayed; no direct agent-to-agent messaging occurred)

Finding IDs are unchanged (F-001..F-031). Proposed merges are noted; vote on each ID as written.

## Lead-verified facts (static, lead is non-voting)
- F-004: frontend/src/App.vue:83-88 calls `useHead({ htmlAttrs: { lang: computed(() => locale.value) } })` and main.ts:10,20 installs `createHead` from @unhead/vue/client. So <html lang> IS updated at runtime on locale change; index.html:2 is only the pre-hydration default. Remaining gap: locale codes are 'pt-br' etc. (i18n.ts:8-29) — valid BCP-47, non-canonical casing. Runtime output NOT_VERIFIED (no browser).
- F-003: GenericModal.vue:60-64 registers a window keydown handler (Escape) while shown; AddProgramDialog and RestartDialog also handle Escape. ScheduleDialog.vue and LiveDialog.vue do not use GenericModal and have no keydown/Escape handler (grep). No component moves focus into the dialog, traps or restores it.
- F-001: confirmed AuthFetchError has only `response` and `data` (authFetch.ts:9-16); playlist.ts:83 `e.status` is always undefined; markSaved() at :91 runs unconditionally.

## Peer arguments
- F-001: all 4 commenting reviewers support HIGH. backend and functional reviewers argue it should hold readiness / lean is_blocker=true (on-air continuity: saving the falsely-empty day overwrites the real playlist).
- F-010: frontend-reviewer proposes merging into F-001 (same getPlaylist flow).
- F-002: backend-reviewer supports HIGH non-blocker.
- F-003: accessibility supports blocker. visual, functional dispute blocker (Escape partly handled; missing focus trap is AA gap, not playout blocker; suggest HIGH non-blocker or MEDIUM). frontend asks for evidence of blocker status.
- F-004: accessibility and visual support the finding (based on no document.documentElement.lang writer) — but see lead-verified useHead fact above. frontend disputes (suggest LOW/MEDIUM non-blocker); functional suggests MEDIUM non-blocker.
- F-005, F-006: functional suggests MEDIUM rather than HIGH; visual keeps HIGH; accessibility abstains on F-005.
- F-007: frontend supports MEDIUM; functional suggests HIGH (live status silently dies). backend: root cause shared with F-001 (AuthFetchError has no `status`).
- F-012/F-015: accessibility proposes moving the alert-timeout part of F-015 into F-012; F-015 keeps the missing live region.
- F-013: owner rewords to "de/ru lack ~85% of keys", MEDIUM.
- F-016: owner notes accessible name is inferred from accname rules, not observed.
- F-017, F-018: visual (owner) notes these are its own items; keep one ID each.
- F-020: support, layout NOT_VERIFIED.
- F-029: backend-reviewer verified server enforces roles (ensure_any_authority; Role::GlobalAdmin on /system/* at backend/app/src/api/routes/system.rs:77-238; User allowed on operator endpoints by design, file.rs:192-196). Client gating is UX only; reduce or dismiss as security issue.
- F-023: keep LOW; uuid and access tokens IP-bound and re-checked server-side (sse/routes.rs:94-103, file.rs:314-316).
