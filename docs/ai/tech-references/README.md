# NoAr technology reference skills

The NoAr repository includes four additional scoped reference skills. These are **not agents** and **do not join the five-member audit councils**.

| Skill | References | Version evidence |
| --- | --- | --- |
| `/noar-rust` | Rust, Axum, Tokio, SQLx, SQLite | Cargo manifests, Cargo.lock |
| `/noar-vue` | Vue, TypeScript, Pinia, Vue Router, Vite | package.json, package-lock.json |
| `/noar-docker` | Docker, Compose, BuildKit | Dockerfile, Compose, workflows |
| `/noar-video-processing` | Pipeline, FFmpeg, A/V sync, SCTE-35 | engine code, native FFmpeg, ffmpeg-next |
| `/noar-ffmpeg` (existing) | Scoped FFmpeg C API and CLI manuals | installed native libs + bindings |
| `/noar-intelligent-development` (existing) | Main development, audits and integration QA | repository source + evidence |

Examples:

```text
/noar-rust review Tokio cancellation
/noar-vue reference Pinia store reactivity
/noar-docker review multi-stage build cache
/noar-video-processing review A/V timestamp continuity
/noar-intelligent-development implement improve playout synchronization
```

The Orchestrator selects the smallest relevant playbook **only when needed** and may directly read the skill file; this does not spawn an agent. Use official documentation on demand, keep the map/token savings, and maintain independent runtime test gates.
