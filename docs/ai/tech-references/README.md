# NoAr technology reference skills

The NoAr repository includes scoped reference skills. These are **not agents** and **do not join the five-member audit councils**. GStreamer is the primary reference for the current media engine.

| Skill | References | Version evidence |
| --- | --- | --- |
| `/noar-rust` | Rust, Axum, Tokio, SQLx, SQLite | Cargo manifests, Cargo.lock |
| `/noar-vue` | Vue, TypeScript, Pinia, Vue Router, Vite | package.json, package-lock.json |
| `/noar-docker` | Docker, Compose, BuildKit | Dockerfile, Compose, workflows |
| `/noar-gstreamer` | Native pipeline, states, clocks, buffers, caps and transport | Cargo.lock, native GStreamer, gst-inspect-1.0; [navigation](gstreamer.md) |
| `/noar-video-processing` | GStreamer pipeline, A/V sync, SCTE-35 | engine code and integrated stream evidence |
| `/noar-ffmpeg` (existing) | Remaining frame-memory interfaces and fixture CLI/API | installed native libs + bindings; not a playout backend |
| `/noar-intelligent-development` (existing) | Main development, audits and integration QA | repository source + evidence |

Examples:

```text
/noar-rust review Tokio cancellation
/noar-vue reference Pinia store reactivity
/noar-docker review multi-stage build cache
/noar-gstreamer review source switching and persistent SRT output
/noar-video-processing review A/V timestamp continuity
/noar-intelligent-development implement improve playout synchronization
```

The Orchestrator selects the smallest relevant playbook **only when needed** and may directly read the skill file; this does not spawn an agent. Use official documentation on demand, keep the map/token savings, and maintain independent runtime test gates.
