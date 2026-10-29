## Build NoAr

Use Rust 1.99.1 and Node.js 24, matching `Dockerfile` and
`.github/workflows/validate.yml`. NoAr uses one mandatory GStreamer media
runtime; no backend selector or FFmpeg playout fallback is provided.

### Native media dependencies

Use Debian trixie or another environment supplying GStreamer 1.26 and the
compatible codec libraries. On Debian install:

```bash
sudo apt install \
    clang libclang-dev pkg-config cmake ffmpeg fonts-dejavu-core \
    libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev \
    libgstreamer-plugins-bad1.0-dev \
    gstreamer1.0-tools gstreamer1.0-plugins-base gstreamer1.0-plugins-good \
    gstreamer1.0-plugins-bad gstreamer1.0-plugins-ugly gstreamer1.0-libav \
    glib-networking \
    libavcodec-dev libavdevice-dev libavfilter-dev libavformat-dev \
    libavutil-dev libswresample-dev libswscale-dev
rustup component add rustfmt clippy
```

The MPEG-TS development library is part of the bad-plugin development package.
Check native core, app, audio, video, pbutils and mpegts libraries with
`pkg-config --modversion gstreamer-1.0 gstreamer-app-1.0 gstreamer-audio-1.0
gstreamer-video-1.0 gstreamer-pbutils-1.0 gstreamer-mpegts-1.0`.

FFmpeg frame structures remain a memory interchange for the existing Rust
compositor and audio processing. GStreamer's libav plugin supplies additional
native codecs. Neither is an alternative playback engine. The FFmpeg command
line tools are used to generate and independently inspect synthetic test media.

### SRT lifecycle dependency

Docker and CI build GnuTLS SRT 1.5.5 through `docker/srt/build.sh`, preserving
`libsrt-gnutls.so.1.5`. The archive is pinned by SHA-256. Native GStreamer SRT
plugins load this corrected library, so closing an input session cannot trigger
the affected SRT 1.5.4 cleanup of other active sockets in the process.

For local builds, install `cmake`, `curl`, `g++`, `libgnutls28-dev` and `make`,
then build into a prefix you control and configure the loader to use that prefix.
Installing headers does not change the runtime library. Do not replace a live
process's libraries. Build provenance is recorded in the Docker image at
`/usr/local/share/noar/srt-build.txt`.

`docker/srt/test-lifecycle.c` checks data transfer while 20 auxiliary sessions
open and close. It is a dependency regression test; NoAr/MediaMTX/VLC transitions
still need separate observation. The engine forwards GStreamer diagnostics to
NoAr's logger. It does not install FFmpeg or libsrt process log callbacks.

NoAr reads external live URLs; input server listeners and automatic encoder
publication takeover have been removed. See [live_ingest.md](live_ingest.md).

Desktop presentation remains available through the desktop Cargo features. It
needs ALSA development files and a graphical session on Linux. The Docker server
image is built without a desktop window. Legacy `ffmpeg-filter` and
`ffmpeg-device` features only affect linked library capabilities; they do not
select a second media engine or restore input listeners.

## Processing Benchmark

The optional `processing-bench` feature prints periodic wall-clock timings for
decoded video/audio frame transfers, audio processing, scaling, overlays,
and desktop presentation stages. It is intended for local CPU
profiling and is disabled in normal builds.

```bash
cargo run -p noar --features processing-bench -- \
    --processing-bench-interval 5 \
    -l 127.0.0.1:8787 --log-to-console -o hls
```

`--processing-bench-interval` is measured in seconds and defaults to `1`.
The `NOAR_PROCESSING_BENCH_INTERVAL` environment variable provides the
same setting. The reported times measure the instrumented caller thread; use
`perf` or GStreamer tracing for work performed asynchronously by native plugins.

## Hardware Encoding

The codec list comes from the installed GStreamer registry and checks encoded
caps against each output muxer. CPU-backed frames pass through native color
conversion before encoding. Hardware encoders additionally need their matching
GStreamer plugin, driver and accessible device. Software-only validation does
not establish that a GPU is available; unsupported encoder settings are rejected
instead of falling back to FFmpeg.

## Generate types for Frontend
The frontend uses TypeScript, to generate types for the rust structs run: `cargo test`.

The generated types are written next to the frontend sources that import them.

## Crates

The workspace has two crates: `noar` (`backend/app`: the server, the API and
the embedded web interface; the binary is `noar`) and `noar-engine`
(`backend/engine`: decoding, compositing, encoding and the outputs).

Versions and releases are described in [releasing.md](releasing.md).

## Database migrations on `main`

Database migrations added on the `main` branch are considered unstable until
the next NoAr version containing them is released. Before that release,
an existing migration may still be changed, reordered, or combined with
another migration.

Running a self-built development version can therefore apply an intermediate
migration that is incompatible with a later commit. This can break the
migration chain or cause a migration checksum mismatch, and the affected
database may have to be reinitialized. Do not use builds from `main` with a
production database without first creating a restorable backup. Once a
migration has been included in a release, it must remain unchanged; subsequent
schema changes require a new migration.

## Setup Frontend

Install the dependencies:

```bash
npm ci
```

## Development Server

Start the development server on http://127.0.0.1:5757

```bash
npm run dev
```

## Production

Build the application for production:

```bash
npm run build
```

## Run NoAr in development mode

1. run backend: `cargo run -- -l 127.0.0.1:8787` (set `DB` and the
   `NOAR_SETUP_*` paths to a folder of your own, as the Dockerfile does)
2. in a second terminal:
    1. install packages: `npm ci`
    2. run frontend: `npm run dev`
3. in the browser navigate to: `http://127.0.0.1:5757`
4. Complete first-time setup in the frontend.

For an initial setup without the frontend, use `cargo run -- -i` instead.
