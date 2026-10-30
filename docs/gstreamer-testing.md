# Test the GStreamer branch from GitHub

This runs the actual NoAr application, including its web UI and Scheduler.
It is separate from the earlier independent Python prototype. The branch is
**work in progress**, not a published release or production-ready image.
See [current validation and known failures](gstreamer-integration.md).

Use a separate checkout, Compose project, image, ports and data directory.
These commands do not update the existing `~/NoAr` production installation.
Run them on the test server that has Docker, not on the PC that only opens VLC.
Git, Docker Engine, Compose v2, network access to GitHub/Debian/container and
package registries, and sufficient free disk for a complete Rust image build
are required. Rust/Node/GStreamer are built or installed inside the image.

## Download and build

On the Docker server:

```sh
cd ~
git clone --single-branch --branch prototype/gstreamer-continuity \
  https://github.com/LuizStSantos/NoAr.git NoAr-gstreamer-test
cd ~/NoAr-gstreamer-test
git branch --show-current
git rev-parse --short HEAD
df -h .
docker system df
mkdir -p qa/gstreamer/artifacts/server-data
export NOAR_COMMIT="$(git rev-parse HEAD)"
docker compose -f qa/gstreamer/compose.integrated.yaml config --quiet
docker compose -f qa/gstreamer/compose.integrated.yaml build --pull noar
docker compose -f qa/gstreamer/compose.integrated.yaml up -d --no-build
docker compose -f qa/gstreamer/compose.integrated.yaml ps
```

The first build can take considerable time and disk space. Wait for it to
finish successfully before `up`. A failed build is not a usable test image.
The full image build is **NOT_AVAILABLE in the current Codex executor** because
its 32 GB disk and Docker `vfs` storage exhausted space. Native local compilation
and Dockerfile checks do not establish that the complete image was built.
Do not prune the server's unrelated Docker images or production volumes.

Always include `-f qa/gstreamer/compose.integrated.yaml` in commands for this
test. The root Compose file belongs to the normal installation architecture.
The test uses `noar-gstreamer-staging:local`, never `ghcr.io/...:latest`, has no
updater service, and excludes both test containers from Watchtower updates.
Its data is under this checkout's `qa/gstreamer/artifacts/server-data`.
Complete the setup with these new paths; do not reuse a production database
or production media directories. Default container paths under `/data` are
already suitable for this isolated installation.

## Open the web interface

The default binds all test ports to the server's loopback address. On a PC
with SSH access to that server, keep this command running in a terminal:

```sh
ssh -N -L 18787:127.0.0.1:18787 YOUR_USER@YOUR_SERVER
```

Then open `http://127.0.0.1:18787` on that PC. Complete the fresh NoAr setup,
create test credentials and add synthetic/test media. The SSH tunnel forwards
HTTP only; it does not forward SRT or UDP.

For direct access from an authorized test network, bind the dedicated test
ports to the server interfaces and recreate only this Compose project:

```sh
cd ~/NoAr-gstreamer-test
export NOAR_GST_BIND_IP=0.0.0.0
docker compose -f qa/gstreamer/compose.integrated.yaml up -d --no-build
```

Open `http://SERVER_IP:18787` and finish setup before exposing access more
broadly. Network/firewall/NAT rules must already permit the chosen test ports.
This guide does not change firewall rules. Only the test paths in this
dedicated MediaMTX instance accept unauthenticated publication/reading; do not
use it as a production media server.

To preserve this binding across new SSH sessions, create `.env` in the root
of the fresh **test checkout**, which is ignored by Git:

```sh
cd ~/NoAr-gstreamer-test
printf '%s\n' 'NOAR_GST_BIND_IP=0.0.0.0' > .env
```

Use this creation command only in a fresh checkout without an existing `.env`.
If one already contains custom test settings, edit its binding instead of
overwriting it. Compose reads this file when commands run from the checkout
root. Before starting, inspect `docker ps --format 'table {{.Names}}\t{{.Ports}}'`;
if a test port is already occupied, choose a free `NOAR_GST_*_PORT` value in
this file and use that same port in the browser, encoder and VLC URLs.

The default ports below can be changed using the variables in
`compose.integrated.yaml`. Keep the same variable values when recreating the
project, and replace the port in the browser/VLC/encoder URL accordingly.

| Purpose | Server port | Transport |
| --- | --- | --- |
| NoAr UI/API/HLS | 18787 | TCP |
| Test MediaMTX SRT | 19890 | UDP |
| Test MediaMTX RTMP | 11935 | TCP |
| Test MediaMTX HLS | 19888 | TCP |
| NoAr UDP live receiver | 19912 | UDP |

## Publish a test live and use it in the Scheduler

NoAr pulls RTMP/SRT/HLS from the dedicated MediaMTX service. UDP reception is
direct. Inside a container, `localhost` refers to that container; use the
`mediamtx` Compose service name for the media server.

| Live input | Encoder/test source destination | Address saved in NoAr |
| --- | --- | --- |
| SRT | `srt://SERVER_IP:19890?streamid=publish:input` | `srt://mediamtx:8890?streamid=read:input` |
| RTMP | `rtmp://SERVER_IP:11935/live/input` | `rtmp://mediamtx:1935/live/input` |
| HLS | Publish SRT to the `input` path above | `http://mediamtx:8888/input/index.m3u8` |
| UDP | `udp://SERVER_IP:19912?pkt_size=1316` carrying MPEG-TS/H.264/AAC | `udp://0.0.0.0:15000` |

In **Scheduler → New live**, select the protocol and save the corresponding
read address. Use **Test signal**, then schedule a short future event. Test
both **Clock** and **Continue** return policies. Observe the actual output,
audio, scheduled admission, return to files and behavior after a source loss.
The full input matrix remains under validation; a saved URL alone is not a
playback PASS. Use one synthetic publisher per path and change protocols
between separate test cases.

For a simple visual output test, choose HLS in NoAr Output settings and use
playlist name `stream`. For channel 1, open this on the test network:

```text
http://SERVER_IP:18787/public/1/live/master.m3u8
```

For SRT output, send NoAr to the **test** MediaMTX address:

```text
srt://mediamtx:8890?streamid=publish:output
```

Then open in VLC on the test network:

```text
srt://SERVER_IP:19890?streamid=read:output
```

For RTMP output, the NoAr destination is
`rtmp://mediamtx:1935/live/output`; the corresponding external viewer address
is `rtmp://SERVER_IP:11935/live/output`. The application and stream components
(`live/output`) are both required.

SCTE-35 needs independent validation. A stock MediaMTX does not preserve the
SCTE-35 track, so VLC through this relay cannot prove cue preservation. Use
the [original-byte capture and manifest smoke tests](../qa/gstreamer/INTEGRATED.md)
for UDP/SRT/HLS with SCTE-35. RTMP has no SCTE-35 implementation. A smooth VLC
picture alone does not verify cue CRCs, timing or downstream ad insertion.

## Update this test checkout

After new commits are pushed to the branch:

```sh
cd ~/NoAr-gstreamer-test
git status --short
git fetch origin prototype/gstreamer-continuity
git pull --ff-only origin prototype/gstreamer-continuity
git rev-parse --short HEAD
export NOAR_COMMIT="$(git rev-parse HEAD)"
docker compose -f qa/gstreamer/compose.integrated.yaml build noar
docker compose -f qa/gstreamer/compose.integrated.yaml up -d --no-build
docker compose -f qa/gstreamer/compose.integrated.yaml ps
```

Keep `NOAR_GST_BIND_IP` and any custom port variables consistent with the first
start. Preserve local edits if Git reports a conflict; do not reset them to
force an update. Pulling a published `latest` image does not install this branch.

Read logs and check the native runtime inside the actual test service:

```sh
docker compose -f qa/gstreamer/compose.integrated.yaml logs --tail 100 noar mediamtx
docker compose -f qa/gstreamer/compose.integrated.yaml exec -T noar \
  sh -c 'noar --version; gst-inspect-1.0 --version; cat /usr/local/share/noar/srt-build.txt'
```

Record the commit ID, input/output protocols, SCTE-35 setting, event times and
observed failures. Remove credentials/private URLs before sharing logs.

## Stop only the test

```sh
cd ~/NoAr-gstreamer-test
docker compose -f qa/gstreamer/compose.integrated.yaml down
```

This stops only the `noar-gstreamer-staging` project and retains its data for
another test. The existing `~/NoAr` project and its database are unaffected.
