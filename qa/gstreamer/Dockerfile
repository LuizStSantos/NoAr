# syntax=docker/dockerfile:1
FROM debian:trixie-slim

RUN --mount=type=secret,id=proxy_ca \
    if [ -f /run/secrets/proxy_ca ]; then \
        export APT_CONFIG=/tmp/prototype-apt.conf; \
        printf 'Acquire::https::CaInfo "/run/secrets/proxy_ca";\n' > "$APT_CONFIG"; \
    fi; \
    apt-get update && apt-get install -y --no-install-recommends \
        python3 python3-gi python3-gst-1.0 gir1.2-gstreamer-1.0 \
        gir1.2-gst-plugins-base-1.0 gstreamer1.0-tools \
        gstreamer1.0-plugins-base gstreamer1.0-plugins-good \
        gstreamer1.0-plugins-bad gstreamer1.0-plugins-ugly \
        gstreamer1.0-libav ffmpeg ca-certificates \
    && rm -rf /var/lib/apt/lists/* /tmp/prototype-apt.conf

ENV PYTHONUNBUFFERED=1 XDG_CACHE_HOME=/tmp/cache GST_REGISTRY=/tmp/gst-registry.bin
WORKDIR /prototype
USER 1000:1000
ENTRYPOINT ["python3"]
