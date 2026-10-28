---
name: noar-vue
description: Official Vue 3, TypeScript, Pinia, Vue Router, Vite and Tailwind references for NoAr frontend implementation and review.
---

# NoAr Vue Frontend Reference

User request: $ARGUMENTS

Applies to `frontend/`, root `package.json`, `package-lock.json`, `vite.config.ts` and TypeScript configuration.

## Official sources

- Vue 3 https://vuejs.org/guide/introduction.html
- Vue TypeScript https://vuejs.org/guide/typescript/overview.html
- Vue Composition API https://vuejs.org/guide/extras/composition-api-faq.html
- TypeScript https://www.typescriptlang.org/docs/handbook/intro.html
- Pinia https://pinia.vuejs.org/
- Vue Router https://router.vuejs.org/
- Vite https://vite.dev/guide/
- Tailwind CSS https://tailwindcss.com/docs/
- WAI-ARIA patterns https://www.w3.org/WAI/ARIA/apg/

## Research procedure

Check actual installed dependency versions in `package-lock.json`. Trace only necessary SFCs, stores, composables, routes and relevant API calls. Examine reactivity, watchers, lifecycle cleanup, fetch errors, router guards, responsive layout and accessible keyboard/focus behavior. Propose type-check/build/lint checks; avoid auto-fixing during read-only review. Actual clicking or responsiveness requires a browser test in isolated staging via existing Integration QA.

## Research and safety rules

1. Search the persistent NoAr repository map and inspect only relevant current code.
2. Read exact lockfile/resolved dependency versions and active features before applying advice. Online "latest" documentation may not match NoAr.
3. Consult 1–3 relevant official reference sections, cite exact URLs and NoAr file paths, and keep the reply concise.
4. Distinguish DOCUMENTED, SOURCE_VERIFIED, RUNTIME_VERIFIED and NOT_RUN. Documentation and code review do not prove on-air playback or clickable UI.
5. Read-only research never changes application code, production configuration, live services or data.
6. This skill is a reference playbook, not a new agent or auditor. Implementation belongs to the main workflow, and runtime validation belongs to authorized isolated QA.
