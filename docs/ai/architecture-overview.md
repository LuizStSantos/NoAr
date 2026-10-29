# NoAr Architecture — Navigation Guide

**This file is a human-written navigation aid, not a substitute for verifying current source code.**

- Rust workspace: `Cargo.toml` with `backend/app` (`noar`, HTTP API + coordination) and `backend/engine` (`noar-engine`, media pipeline).
- API handlers/auth: `backend/app/src/api/`, grouped routes under `api/routes/`.
- Persistent data: `backend/app/src/db/handles/` and SQL migrations under `migrations/`.
- Playout and scheduler: `backend/app/src/player/`; playback loop and command orchestration.
- GStreamer media inputs/outputs: `backend/engine/src/input/`, `backend/engine/src/output/`. Live URLs are clients of an external ingest service such as MediaMTX; NoAr does not host input listeners.
- SCTE-35: `backend/engine/src/output/scte35.rs`, scheduled break integration and frontend configuration.
- XMLTV and programs: `backend/app/src/utils/xmltv/`.
- Frontend Vue/TypeScript: `frontend/src/components`, `frontend/src/views`, `frontend/src/stores`, `frontend/src/composables`, `frontend/src/router`.
- Build/test/release: `.github/workflows/{validate,docker,release}.yml`, `Dockerfile`, `docker-compose.yml`, `docs/releasing.md`.

Read `docs/ai/repository-map.md` for the updated machine-generated file map and query symbols with the index script before opening whole files. Revalidate module descriptions after refactors.
