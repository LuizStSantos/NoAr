# syntax=docker/dockerfile:1
# Build the prototype runtime first with ./run.sh build. Mount the existing
# /workspace Rust toolchain when running this development-only image.
ARG BASE_IMAGE=noar-gstreamer-prototype:local
FROM ${BASE_IMAGE}

USER root
RUN --mount=type=secret,id=proxy_ca \
    set -eu; \
    if [ -f /run/secrets/proxy_ca ]; then \
        export APT_CONFIG=/tmp/noar-dev-apt.conf; \
        printf 'Acquire::https::CaInfo "/run/secrets/proxy_ca";\n' > "$APT_CONFIG"; \
    fi; \
    apt-get update; \
    DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
        build-essential cmake libclang1-19 libclang-common-19-dev pkg-config \
        libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev \
        libgstreamer-plugins-bad1.0-dev \
        libavcodec-dev libavdevice-dev libavfilter-dev libavformat-dev libavutil-dev \
        libswresample-dev libswscale-dev \
        libasound2-dev libudev-dev libxkbcommon-dev libwayland-dev libvulkan-dev; \
    rm -rf /var/lib/apt/lists/* /tmp/noar-dev-apt.conf

ENV CARGO_HOME=/workspace/.cargo \
    RUSTUP_HOME=/workspace/.rustup \
    CARGO_TARGET_DIR=/workspace/noar-gstreamer-dev/target \
    CARGO_INCREMENTAL=0 \
    CARGO_BUILD_JOBS=2 \
    CARGO_PROFILE_DEV_DEBUG=0 \
    CARGO_PROFILE_TEST_DEBUG=0 \
    LIBCLANG_PATH=/usr/lib/llvm-19/lib \
    PATH=/workspace/.cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
WORKDIR /workspace/NoAr
USER 1000:1000
ENTRYPOINT ["sh"]
