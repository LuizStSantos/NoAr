#!/bin/sh
# Build the corrected native SRT lifecycle without changing the SRT ABI that
# Debian's packages (and GStreamer's SRT plugin) expect.
set -eu

prefix=${1:-/usr/local}
build_dir=$(mktemp -d)
trap 'rm -rf "$build_dir"' EXIT HUP INT TERM

version=1.5.5
checksum=c3518bc43a71b5289032395b2db4c3e09e73d78b54247d56c14553a503b491cf
# Slow or unstable links to GitHub cut HTTP/2 streams (curl error 92), which
# plain --retry does not cover; the checksum below still verifies the file.
curl --fail --location --http1.1 --connect-timeout 30 \
    --retry 6 --retry-delay 5 --retry-all-errors \
    "https://codeload.github.com/Haivision/srt/tar.gz/refs/tags/v${version}" \
    -o "$build_dir/srt.tar.gz"
printf '%s  %s\n' "$checksum" "$build_dir/srt.tar.gz" | sha256sum --check -
tar -xzf "$build_dir/srt.tar.gz" -C "$build_dir"

cmake -S "$build_dir/srt-$version" -B "$build_dir/build" \
    -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$prefix" \
    -DCMAKE_INSTALL_LIBDIR=lib -DTARGET_srt=srt-gnutls \
    -DUSE_ENCLIB=gnutls -DENABLE_SHARED=ON -DENABLE_STATIC=OFF \
    -DENABLE_APPS=OFF -DENABLE_TESTING=OFF
cmake --build "$build_dir/build" --parallel 2
cmake --install "$build_dir/build"

mkdir -p "$prefix/share/noar" "$prefix/share/licenses/noar-srt"
printf 'SRT %s (GnuTLS), corrected native session lifecycle\nSource SHA256: %s\n' \
    "$version" "$checksum" > "$prefix/share/noar/srt-build.txt"
cp "$build_dir/srt-$version/LICENSE" "$prefix/share/licenses/noar-srt/LICENSE"
