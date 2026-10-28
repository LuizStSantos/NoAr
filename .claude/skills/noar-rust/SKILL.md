---
name: noar-rust
description: Official Rust, Axum, Tokio, SQLx and SQLite documentation for NoAr backend API, concurrency and database tasks.
---

# NoAr Rust Backend Reference

User request: $ARGUMENTS

Applies to `backend/app/`, `backend/engine/`, `migrations/` and Rust workspace manifests. Respect `backend/AGENTS.md`. For specific FFmpeg C API questions consult `/noar-ffmpeg` instead.

## Official sources

- Rust Book https://doc.rust-lang.org/book/
- Rust Reference https://doc.rust-lang.org/reference/
- Rust stdlib https://doc.rust-lang.org/std/
- Cargo https://doc.rust-lang.org/cargo/reference/
- Axum https://docs.rs/axum/
- Tokio https://docs.rs/tokio/
- SQLx https://docs.rs/sqlx/
- SQLite https://www.sqlite.org/docs.html

## Research procedure

Check `backend/app/Cargo.toml`, `backend/engine/Cargo.toml`, `Cargo.lock` and feature flags. The examined project uses Axum 0.8, Tokio 1.x and SQLx 0.9 but confirm versions before referencing exact API behavior. Review extractors/auth/errors for Axum, task cancellation/blocking/synchronization for Tokio and bound queries/transactions/migrations for SQLx SQLite. Follow the project module separation and preserve existing contracts. Recommend `cargo fmt --all -- --check`, focused tests and clippy as appropriate, stating which actually ran.

## Research and safety rules

1. Search the persistent NoAr repository map and inspect only relevant current code.
2. Read exact lockfile/resolved dependency versions and active features before applying advice. Online "latest" documentation may not match NoAr.
3. Consult 1–3 relevant official reference sections, cite exact URLs and NoAr file paths, and keep the reply concise.
4. Distinguish DOCUMENTED, SOURCE_VERIFIED, RUNTIME_VERIFIED and NOT_RUN. Documentation and code review do not prove on-air playback or clickable UI.
5. Read-only research never changes application code, production configuration, live services or data.
6. This skill is a reference playbook, not a new agent or auditor. Implementation belongs to the main workflow, and runtime validation belongs to authorized isolated QA.
