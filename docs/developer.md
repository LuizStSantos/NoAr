## Build NoAr

NoAr ships as a Docker image (see [docker.md](docker.md)); build from source
only to work on it. Use the latest stable Rust version from
[rustup](https://rustup.rs/) on Linux.

### FFmpeg libraries

NoAr links against FFmpeg through `ffmpeg-next`, so local builds need the FFmpeg runtime tools and development libraries. FFmpeg 7.0+ is required.

On Debian/Ubuntu based systems install:

```bash
sudo apt install \
    ffmpeg \
    libavcodec-dev \
    libavformat-dev \
    libavutil-dev \
    libswresample-dev \
    libswscale-dev \
    pkg-config
```

When compiling against a manually installed FFmpeg, make sure `pkg-config` can find the `.pc` files, for example:

```bash
export PKG_CONFIG_PATH=/usr/local/lib/pkgconfig:$PKG_CONFIG_PATH
```

### Optional SRT logging integration

The Docker image and CI build native SRT 1.5.5 with GnuTLS using
`docker/srt/build.sh`. FFmpeg opens SRT inputs and outputs in the same
process; SRT 1.5.4 can clean up other active sockets when one session closes.
The build pins the upstream 1.5.5 archive by SHA-256 and preserves the
`libsrt-gnutls.so.1.5` ABI. See the corrected lifecycle in
[SRT 1.5.5](https://github.com/Haivision/srt/blob/v1.5.5/srtcore/api.cpp).

For a local Debian build with the affected library, install `cmake`, `curl`,
`g++`, `libgnutls28-dev` and `make`, then run the build script with an
installation prefix you control. Configure the loader to use that prefix;
installing headers alone does not change the library FFmpeg loads. CI installs
under `/usr/local` and runs `ldconfig` before testing. Never replace the
library of an on-air process.

`docker/srt/test-lifecycle.c` sends real data over a loopback SRT connection
while opening and closing 20 auxiliary sessions. It must fail with the
affected stock library and pass with the corrected release. The test runs
during the Docker dependency build and CI; it does not prove MediaMTX/VLC recovery.

Run only the isolated dependency check with:

```sh
docker build --target srt-build -t noar-srt-check .
docker run --rm --network none noar-srt-check timeout 30 /tmp/test-srt-lifecycle
```

If a development environment uses a TLS proxy, the Docker build accepts an
optional `proxy_ca` BuildKit secret. The certificate is mounted only during
networked build steps; TLS verification remains enabled.

The engine's build script uses `pkg-config` to check whether libsrt is linkable
for the target build. If `pkg-config --modversion srt` succeeds, NoAr links
libsrt directly and routes its native messages through its own logger,
including rate limiting for repeated receive-buffer warnings. Use the
development files for the same libsrt installation as your FFmpeg build.

If libsrt or its `srt.pc` file is unavailable, NoAr still builds and SRT
ingest can still work through an FFmpeg build that supports it. In that case,
libsrt's native messages may appear directly on stderr instead of in the
NoAr logs.

`libavfilter` and `libavdevice` are optional and disabled by default to reduce
the runtime and static-build footprint. NoAr uses its own media
processing pipeline, so FFmpeg filters such as `drawtext` and FFmpeg devices
such as DeckLink are unavailable in the normal build.

Enable them independently with the `ffmpeg-filter` or `ffmpeg-device` Cargo
feature. Local builds additionally need `libavfilter-dev` or `libavdevice-dev`.

The engine can also open a local desktop window (`desktop` feature, needs
`libasound2-dev` and a graphical session). The Docker image is built without
it, and the web interface does not offer it.

## Processing Benchmark

The optional `processing-bench` feature prints periodic wall-clock timings for
video and audio decode, audio processing and encoding, scale, overlay,
subtitle, encoded-output, and desktop presentation stages. It is intended for local CPU
profiling and is disabled in normal builds.

```bash
cargo run -p noar --features processing-bench -- \
    --processing-bench-interval 5 \
    -l 127.0.0.1:8787 --log-to-console -o hls
```

`--processing-bench-interval` is measured in seconds and defaults to `1`.
The `NOAR_PROCESSING_BENCH_INTERVAL` environment variable provides the
same setting. The reported times measure the instrumented caller thread; use
`perf` for CPU time inside FFmpeg worker threads.

## Hardware Encoding

The output codec list includes hardware encoders only when they accept the
engine's CPU-backed frames. NVENC accepts `yuv420p` directly. QSV receives a
CPU-side `nv12` frame produced immediately before the encoder and performs the
surface upload internally. VAAPI keeps decoding, scaling and overlays on the
CPU, then uploads the completed `nv12` frame to `/dev/dri/renderD128`
immediately before encoding. The process therefore needs access to the render
device, normally through the `render` group, in addition to a matching GPU
driver.

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
