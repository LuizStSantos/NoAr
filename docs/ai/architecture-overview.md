# NoAr Architecture Overview

This is a navigation aid based on the repository structure reviewed on October 21, 2026, not a substitute for reading changed source code.

| Area | Primary paths | Responsibility |
| --- | --- | --- |
| HTTP API and auth | `backend/app/src/api/` | REST routing, access control and HTTP handlers |
| Database | `backend/app/src/db/` | SQLite models, migrations integration and handles |
| Playout controller | `backend/app/src/player/` | Playlist orchestration, scheduler and output coordination |
| Program guide | `backend/app/src/utils/xmltv/` | Program metadata and XMLTV generation |
| Media input | `backend/engine/src/input/` | File/live decoding, preloading and input timestamps |
| Media output | `backend/engine/src/output/` | Encoding, HLS, streaming, SCTE-35 and recording |
| Frontend | `frontend/src/` | Vue UI, components, router, state and composables |
| Schema changes | `migrations/` | Ordered SQLite migrations |
| Release and CI | `.github/workflows/` | Validation, container builds and GitHub releases |
| Runtime | `Dockerfile`, `docker-compose.yml` | Image construction, setup and optional updater |

## Project-specific guardrails

- Rust workspace: `backend/app` (`noar`) and `backend/engine` (`noar-engine`).
- Follow `backend/AGENTS.md` for Rust contributions; keep engine independent of the app crate.
- Protect broadcast continuity: verify FFmpeg behavior and scheduler timing where affected.
- Treat generated symbols as approximate; source code is authoritative.
- Do not change release or updater behavior during documentation-only work.
