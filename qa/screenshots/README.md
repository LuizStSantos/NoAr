# README screenshots

`docs/images/` contains real browser captures of a disposable **Example TV**
channel, with synthetic programme media, artwork and authored graphics. The
English UI uses **dark mode** for every picture. Player and Graphics On air
show the actual advancing program preview.

## Prepare an isolated demo

Use the current backend with its GStreamer dependencies and FFmpeg/ffprobe test
tools; see [developer setup](../../docs/developer.md). The scripts require a
fresh, uninitialized loopback server and an explicit owned-directory marker.
They refuse an initialized installation, a different storage root or a remote
API host. Seeding writes files/configuration and starts this test channel.

Example Bash setup, in a directory reserved for this demo:

```sh
export DEMO_ROOT=/tmp/noar-readme
export NOAR_E2E_STAGING=1 NOAR_E2E_DISPOSABLE=1
mkdir -p "$DEMO_ROOT"/{db,media,playlists,public,logs}
touch "$DEMO_ROOT/.qa-readme-owned"
python3 - "$DEMO_ROOT/password.txt" <<'PY'
from pathlib import Path
import secrets
import sys
password = Path(sys.argv[1])
password.write_text(secrets.token_hex(24))
password.chmod(0o600)
PY

DB="$DEMO_ROOT/db/noar.db" \
NOAR_SETUP_STORAGE="$DEMO_ROOT/media" \
NOAR_SETUP_PLAYLISTS="$DEMO_ROOT/playlists" \
NOAR_SETUP_PUBLIC="$DEMO_ROOT/public" \
NOAR_SETUP_LOGS="$DEMO_ROOT/logs" \
TZ=America/Sao_Paulo \
target/debug/noar -l 127.0.0.1:8787
```

In another shell, export the same root/staging variables and seed the demo:

```sh
python3 qa/screenshots/build_demo.py \
  --url http://127.0.0.1:8787 \
  --storage "$DEMO_ROOT/media" \
  --password-file "$DEMO_ROOT/password.txt"
```

The seeder uses the real API for programme sheets, assembled programmes,
playlists, Composer templates, a CSV source and model-based graphics scheduling.
It creates the `admin` demo account, sets the channel timezone, authors logo,
clock, presenter, market and sponsor-window examples, then starts HLS and a
manual presenter take. Extra demo templates are explicitly seeded; fresh NoAr
installations still start with only the removable Logo example.
`demo-manifest.json` stays inside the owned directory and contains template IDs
and the expected storage path, without the password or session tokens.

## Capture the gallery

Install the frontend and isolated browser harness dependencies once, build the
frontend, then serve its production build:

```sh
npm ci
npm ci --prefix qa/e2e
npm run build
npm run preview -- --host 127.0.0.1 --port 5757 --strictPort
```

With the demo running and the staging variables set:

```sh
node qa/screenshots/capture.cjs \
  --base http://127.0.0.1:5757 \
  --api http://127.0.0.1:8787 \
  --manifest "$DEMO_ROOT/demo-manifest.json" \
  --password-file "$DEMO_ROOT/password.txt" \
  --chromium /usr/bin/chromium
```

Omit `--chromium` to use the Playwright-installed browser. `--base` is the
frontend URL; `--api` supplies the backend health endpoint. Both must be loopback
HTTP(S). The capture helper checks the demo channel/storage scope, fixes both
browser color preference and NoAr's saved theme, and asserts `ffp-dark` before
every picture. It also checks document width and real video advancement.

The complete set has **14 screens**: login, Player, Scheduler, Storage, Playlist,
adding a programme folder, assembled-program editor, video marks, Templates,
Composer, Graphics On air, Data sources, Report and Output settings. Viewport
size is 1440×1000; Templates uses a full-page capture to include every group.
`readme-captures.json` records hashes/dimensions, actual theme, health, page errors
and video checks. Review the pictures visually before committing them.

To inspect selected screens, keep partial evidence outside the published set:

```sh
node qa/screenshots/capture.cjs \
  --manifest "$DEMO_ROOT/demo-manifest.json" \
  --password-file "$DEMO_ROOT/password.txt" \
  --chromium /usr/bin/chromium \
  --only composer,onair \
  --evidence "$DEMO_ROOT/partial-captures.json"
```

Failed captures remain as `<name>.failed.png` inside the owned directory. Keep
password files, session state and failed attempts outside the repository.
The helper opens editing dialogs and fills preview fields without saving them.
Schedule times are placed ahead of seeding, so capture soon after preparing it.
