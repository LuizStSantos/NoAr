# NoAr image: builds the web interface and the server, then ships them on a
# slim Debian with only the FFmpeg libraries the server links against.
#
#   docker compose up -d
#
# The first visit to http://<server>:8787 opens the setup page.

ARG NODE_IMAGE=node:24-trixie-slim
ARG RUST_IMAGE=rust:1-slim-trixie
ARG RUNTIME_IMAGE=debian:trixie-slim

# --- Web interface -----------------------------------------------------------
FROM ${NODE_IMAGE} AS frontend
WORKDIR /src

COPY package.json package-lock.json ./
RUN npm ci --no-audit --no-fund

COPY tsconfig.json vite.config.ts ./
COPY frontend ./frontend
RUN npm run build

# --- Server ------------------------------------------------------------------
FROM ${RUST_IMAGE} AS backend
ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        clang \
        libavcodec-dev \
        libavdevice-dev \
        libavfilter-dev \
        libavformat-dev \
        libavutil-dev \
        libclang-dev \
        libswresample-dev \
        libswscale-dev \
        pkg-config && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY .cargo ./.cargo
COPY backend ./backend
COPY migrations ./migrations
COPY --from=frontend /src/frontend/dist ./frontend/dist

ENV NOAR_FRONTEND_PREBUILT=1
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked --package ffplayout --no-default-features --features embed_frontend && \
    install -m 755 target/release/noar /usr/local/bin/noar

# Record which Debian packages provide the shared libraries the binary needs,
# so the runtime image installs exactly those.
RUN ldd /usr/local/bin/noar \
        | awk '/=> \// { print $3 }' \
        | xargs -r realpath \
        | xargs -r dpkg -S 2>/dev/null \
        | cut -d: -f1 \
        | sort -u > /runtime-packages.txt && \
    cat /runtime-packages.txt

# --- Runtime -----------------------------------------------------------------
FROM ${RUNTIME_IMAGE}
ENV DEBIAN_FRONTEND=noninteractive

COPY --from=backend /runtime-packages.txt /tmp/runtime-packages.txt
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        ca-certificates \
        fonts-dejavu-core \
        tini \
        tzdata \
        $(cat /tmp/runtime-packages.txt) && \
    rm -rf /var/lib/apt/lists/* /tmp/runtime-packages.txt

COPY --from=backend /usr/local/bin/noar /usr/local/bin/noar

# NoAr reads the system time zone from /etc/localtime, not from TZ: point it
# at TZ on every start, so the TZ of the compose file sets the time zone.
COPY <<'SCRIPT' /usr/local/bin/noar-entrypoint
#!/bin/sh
set -e

if [ -n "$TZ" ] && [ -f "/usr/share/zoneinfo/$TZ" ]; then
    ln -sf "/usr/share/zoneinfo/$TZ" /etc/localtime
    echo "$TZ" > /etc/timezone
fi

exec noar "$@"
SCRIPT
RUN chmod 755 /usr/local/bin/noar-entrypoint

# Default overlay logo and WebVTT placeholder copied into new storages.
COPY assets/dummy.vtt assets/logo.png /usr/share/ffplayout/

# Everything NoAr keeps lives under /data: mount it as a volume. The
# NOAR_SETUP_* paths only fill the setup page; each channel keeps the paths
# saved for it.
ENV DB=/data/db/noar.db \
    NOAR_SETUP_STORAGE=/data/media \
    NOAR_SETUP_PLAYLISTS=/data/playlists \
    NOAR_SETUP_PUBLIC=/data/public \
    NOAR_SETUP_LOGS=/data/logs \
    TZ=UTC

RUN mkdir -p /data/db /data/media /data/playlists /data/public /data/logs
VOLUME ["/data"]
WORKDIR /data

EXPOSE 8787

ENTRYPOINT ["tini", "--", "noar-entrypoint"]
CMD ["--listen", "0.0.0.0:8787"]
