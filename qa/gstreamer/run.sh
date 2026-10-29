#!/bin/sh
# Operate only the named disposable project; never the production Compose file.
set -eu
cd "$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
export NOAR_GST_UID="$(id -u)" NOAR_GST_GID="$(id -g)"

compose() {
    docker compose --project-name noar-gstreamer-prototype --file compose.yaml "$@"
}

build() {
    if [ -n "${CODEX_PROXY_CERT:-}" ]; then
        docker build --secret "id=proxy_ca,src=$CODEX_PROXY_CERT" \
            --tag noar-gstreamer-prototype:local .
    else
        docker build --tag noar-gstreamer-prototype:local .
    fi
}

cleanup() {
    if [ -n "${fault_pid:-}" ]; then
        kill "$fault_pid" 2>/dev/null || true
        wait "$fault_pid" 2>/dev/null || true
    fi
    compose logs --no-color > artifacts/compose.log 2>&1 || true
    compose down --remove-orphans || true
}

case "${1:-test}" in
    build) build; exit ;;
    down) compose down --remove-orphans; exit ;;
    test|recovery|watch) mode="${1:-test}" ;;
    *) echo "Usage: ./run.sh [build|test|recovery|watch|down]" >&2; exit 2 ;;
esac

mkdir -p fixtures artifacts
compose config --quiet
if ! docker image inspect noar-gstreamer-prototype:local >/dev/null 2>&1; then
    build
fi
if ! docker image inspect bluenviron/mediamtx:1.15.3 >/dev/null 2>&1; then
    docker pull bluenviron/mediamtx:1.15.3
fi
if [ ! -s fixtures/a.mp4 ] || [ ! -s fixtures/b.mp4 ]; then
    compose run --rm --no-deps fixtures
fi

export NOAR_GST_VERIFY=1 NOAR_GST_RECOVERY=0
if [ "$mode" = recovery ]; then
    command -v python3 >/dev/null
    export NOAR_GST_RECOVERY=1
    export NOAR_GST_CYCLES=3
elif [ "$mode" = watch ]; then
    export NOAR_GST_VERIFY=0
fi
compose down --remove-orphans
rm -f -- artifacts/events.jsonl artifacts/report.json artifacts/fault.json
fault_pid=""
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
compose up -d mediamtx source playout
if [ "$mode" = recovery ]; then
    python3 fault.py &
    fault_pid=$!
elif [ "$mode" = watch ]; then
    echo "Open VLC: srt://127.0.0.1:${NOAR_GST_SRT_PORT:-19890}?streamid=read:output"
    echo "This bounded demonstration ends automatically. No web UI is provided."
fi
compose wait playout || true
if [ -n "$fault_pid" ]; then
    wait "$fault_pid"
fi
container_id="$(compose ps --all --quiet playout)"
result="$(docker inspect --format '{{.State.ExitCode}}' "$container_id")"
if [ "$mode" != watch ]; then
    cat artifacts/report.json
fi
exit "$result"
