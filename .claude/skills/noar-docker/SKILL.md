---
name: noar-docker
description: Official Dockerfile, Docker Compose and BuildKit references for NoAr's Debian Rust/Vue multi-stage builds and images.
---

# NoAr Docker Reference

User request: $ARGUMENTS

Applies to `Dockerfile`, `docker-compose.yml`, `.dockerignore`, image release workflows and related containers.

## Official sources

- Dockerfile https://docs.docker.com/reference/dockerfile/
- Multi-stage builds https://docs.docker.com/build/building/multi-stage/
- Build cache https://docs.docker.com/build/cache/
- Compose file https://docs.docker.com/reference/compose-file/
- Build checks https://docs.docker.com/reference/build-checks/
- Docker security https://docs.docker.com/engine/security/
- GitHub Actions https://docs.github.com/en/actions

## Research procedure

Verify base images, BuildKit syntax and `COPY` paths. NoAr builds a Vue frontend with Node, a Rust/GStreamer server on Debian and a slim runtime image. Check native GStreamer development libraries, runtime plugin factories and the remaining FFmpeg frame-memory dependency against `Dockerfile` and the Validate workflow. Use `/noar-gstreamer` for media-plugin details. Check that `.dockerignore` does not exclude required build inputs. Analyze HTTP access, outbound media connections, volumes, image tags, update container, secrets, permissions, BuildKit caching and release publication. NoAr consumes external live URLs; it does not expose embedded RTMP/SRT input servers. Treat updater settings and mount paths as high-risk. Recommend `docker compose config` and disposable build tests; never build/push/deploy/prune without authorization.

## Research and safety rules

1. Search the persistent NoAr repository map and inspect only relevant current code.
2. Read exact lockfile/resolved dependency versions and active features before applying advice. Online "latest" documentation may not match NoAr.
3. Consult 1–3 relevant official reference sections, cite exact URLs and NoAr file paths, and keep the reply concise.
4. Distinguish DOCUMENTED, SOURCE_VERIFIED, RUNTIME_VERIFIED and NOT_RUN. Documentation and code review do not prove on-air playback or clickable UI.
5. Read-only research never changes application code, production configuration, live services or data.
6. This skill is a reference playbook, not a new agent or auditor. Implementation belongs to the main workflow, and runtime validation belongs to authorized isolated QA.
