#!/bin/sh
# Turn on NDI for a NoAr Docker installation.
#
# Run it from the NoAr folder (the one with docker-compose.yml):
#
#   sudo sh docker/ndi/enable-ndi.sh
#
# It downloads the NDI SDK for Linux (you read and accept its license in the
# installer), copies the NDI Runtime (libndi.so.6) to /opt/ndi, turns on mDNS
# discovery (avahi-daemon) and writes docker-compose.override.yml, which
# Docker Compose loads by itself. Then: docker compose up -d
#
# Options:
#   --sdk FILE         Use an SDK installer already downloaded
#                      (Install_NDI_SDK_v6_Linux.tar.gz or .sh).
#   --runtime-dir DIR  Where the runtime goes (default /opt/ndi).
#   --compose-file F   Write the Compose settings to F instead of
#                      docker-compose.override.yml (for the isolated test
#                      project: qa/gstreamer/compose.ndi.yaml).
#   --no-compose       Install the runtime only.
#
# Nothing in the NoAr image changes: the NDI elements are already in it and
# NoAr enables NDI when it finds the runtime at start.
set -eu

SDK_URL=https://downloads.ndi.tv/SDK/NDI_SDK_Linux/Install_NDI_SDK_v6_Linux.tar.gz
runtime_dir=/opt/ndi
sdk=
compose_file=docker-compose.override.yml
write_compose=1

fail() {
    printf 'enable-ndi: %s\n' "$*" >&2
    exit 1
}

say() {
    printf '\n==> %s\n' "$*"
}

while [ $# -gt 0 ]; do
    case $1 in
        --sdk) sdk=${2:?--sdk needs a file}; shift 2 ;;
        --runtime-dir) runtime_dir=${2:?--runtime-dir needs a folder}; shift 2 ;;
        --compose-file) compose_file=${2:?--compose-file needs a file}; shift 2 ;;
        --no-compose) write_compose=0; shift ;;
        -h | --help) sed -n '2,24p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) fail "unknown option $1 (see --help)" ;;
    esac
done

[ "$(id -u)" -eq 0 ] || fail "run it with sudo: sudo sh $0"
[ "$(uname -s)" = Linux ] || fail "the NDI Runtime for NoAr is installed on Linux servers"

case $(uname -m) in
    x86_64 | amd64) arch_pattern=x86_64 ;;
    aarch64 | arm64) arch_pattern=aarch64 ;;
    *) fail "unsupported architecture $(uname -m)" ;;
esac

if [ "$write_compose" -eq 1 ] && [ "$compose_file" = docker-compose.override.yml ] &&
    [ ! -f docker-compose.yml ]; then
    fail "run it from the NoAr folder (the one with docker-compose.yml), or pass --no-compose"
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT HUP INT TERM

# 1. The SDK installer.
if [ -z "$sdk" ]; then
    say "Downloading the NDI SDK for Linux"
    command -v curl >/dev/null || fail "curl is needed to download the SDK"
    curl --fail --location --http1.1 --connect-timeout 30 --retry 6 --retry-delay 5 \
        --retry-all-errors "$SDK_URL" -o "$work/ndi-sdk.tar.gz" ||
        fail "download failed; get the NDI SDK for Linux from https://ndi.video/for-developers/ndi-sdk/ and pass it with --sdk"
    sdk=$work/ndi-sdk.tar.gz
fi
[ -f "$sdk" ] || fail "$sdk does not exist"

case $sdk in
    *.tar.gz | *.tgz)
        tar -xzf "$sdk" -C "$work"
        installer=$(find "$work" -maxdepth 2 -name 'Install_NDI_SDK*_Linux.sh' | head -n 1)
        [ -n "$installer" ] || fail "no installer in $sdk"
        ;;
    *.sh) installer=$sdk ;;
    *) fail "$sdk is neither the SDK .tar.gz nor its .sh installer" ;;
esac

# 2. The license: the NDI installer shows it and asks to accept it.
say "The NDI SDK license follows. Read it and accept it to continue."
(cd "$work" && sh "$installer") || fail "the NDI installer did not finish (license not accepted?)"

library=$(find "$work" -path "*${arch_pattern}*" -name 'libndi.so.6*' -type f | head -n 1)
[ -n "$library" ] || fail "no libndi.so.6 for $(uname -m) in the SDK"

# 3. The runtime where NoAr reads it.
say "Installing the NDI Runtime in $runtime_dir"
mkdir -p "$runtime_dir"
cp -f "$library" "$runtime_dir/"
ln -sf "$(basename "$library")" "$runtime_dir/libndi.so.6"
licenses=$(find "$work" -maxdepth 3 -iname '*licen*' -type f | head -n 5)
if [ -n "$licenses" ]; then
    mkdir -p "$runtime_dir/licenses"
    printf '%s\n' "$licenses" | while IFS= read -r file; do cp -f "$file" "$runtime_dir/licenses/"; done
fi
ls -la "$runtime_dir"

# 4. mDNS, which receivers and NoAr use to find NDI sources.
say "Turning on mDNS discovery (avahi-daemon)"
if command -v systemctl >/dev/null && systemctl is-active --quiet avahi-daemon; then
    echo "avahi-daemon is already running"
elif command -v apt-get >/dev/null; then
    apt-get install -y avahi-daemon
    systemctl enable --now avahi-daemon || echo "warning: start avahi-daemon by hand"
else
    echo "warning: install and start avahi-daemon (or set up an NDI Discovery Server)"
fi

# 5. The Compose settings: host networking for discovery, and the runtime.
if [ "$write_compose" -eq 1 ]; then
    say "Writing $compose_file"
    if [ -f "$compose_file" ] && ! grep -q 'enable-ndi.sh' "$compose_file"; then
        fail "$compose_file already exists with your own settings; add the NDI lines from docs/ndi.md to it"
    fi
    if command -v docker >/dev/null; then
        compose_version=$(docker compose version --short 2>/dev/null || echo 0)
        major=${compose_version%%.*}
        minor=${compose_version#*.}
        minor=${minor%%.*}
        if [ "${major:-0}" -lt 2 ] || { [ "${major:-0}" -eq 2 ] && [ "${minor:-0}" -lt 24 ]; }; then
            fail "Docker Compose $compose_version is too old; NDI needs 2.24 or newer"
        fi
    fi
    cat >"$compose_file" <<EOF
# Written by docker/ndi/enable-ndi.sh: NDI for NoAr (see docs/ndi.md).
# NDI finds sources through mDNS, so NoAr uses the server's network; its web
# interface stays on port 8787 of the server.
services:
  noar:
    network_mode: host
    ports: !reset []
    environment:
      NDI_RUNTIME_DIR_V6: $runtime_dir
      # On the server's network the updater's service name does not resolve.
      NOAR_UPDATER_URL: http://127.0.0.1:18080
    volumes:
      - $runtime_dir:$runtime_dir:ro
  # Settings › Updates keeps working: the updater answers on the server only.
  noar-updater:
    ports:
      - "127.0.0.1:18080:8080"
EOF
    cat "$compose_file"
fi

say "Done"
cat <<EOF
Next:
  1. Allow in the firewall: 5353/UDP (mDNS) and 5959-5999/TCP (NDI)$(
    [ "$write_compose" -eq 1 ] && printf ', and 8787/TCP (NoAr)')
  2. Restart NoAr:   docker compose up -d$(
    [ "$compose_file" != docker-compose.override.yml ] && [ "$write_compose" -eq 1 ] &&
        printf '   (add -f %s to your usual -f files)' "$compose_file")
  3. Settings > Output: NDI is available. Check from the server with
       docker compose exec noar ldd $runtime_dir/libndi.so.6 | grep 'not found'
     (no output means NoAr can load it).

To turn NDI off again: delete $compose_file and run docker compose up -d.
EOF
