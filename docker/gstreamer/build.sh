#!/bin/sh
# Build GStreamer core, gst-plugins-base and gst-libav 1.26.11 to install over
# Debian's 1.26.2. In 1.26.2, decodebin3 aborts the whole process ("Stream
# switch requested for future collection") when a live MPEG-TS source changes
# its streams, for example an encoder that drops its video; 1.26.11 handles
# the smaller collection. gst-libav 1.26.2's avdec_h264 aborts the process
# ("video meta uses 864x480 instead of 854x480") on video whose width is not
# a multiple of its alignment; 1.26.11 sizes its buffer pool for the padding.
# The other plugin sets stay Debian's 1.26.2 builds, which run on a newer core
# and base of the same series. gst-libav uses Debian's FFmpeg libraries, which
# the runtime image already installs for gstreamer1.0-libav.
#
#   sh build.sh /opt/noar-gst   # files land in /opt/noar-gst/usr/...
set -eu

destdir=${1:-/opt/noar-gst}
build_dir=$(mktemp -d)
trap 'rm -rf "$build_dir"' EXIT HUP INT TERM

version=1.26.11
core_sha256=2e0bd192d0438ea606a6f76a95c8e16542167656ffec2c2bc3aaf6ee0837fbf6
base_sha256=fc50f885d41f5d0407ce0876ec7235d9e7b82d48db2f4bc72c5f244a4ac79263
libav_sha256=9bb3d2681ef0de92d1b1a9d955186236ee22e64f372db9bfc0d22e2d0df19865
multiarch=$(gcc -dumpmachine)
install_options="--prefix=/usr --libdir=lib/$multiarch --buildtype=release
    -Dtests=disabled -Ddoc=disabled"
options="$install_options -Dexamples=disabled -Dintrospection=disabled -Dnls=disabled"

fetch() {
    curl --fail --location --http1.1 --connect-timeout 30 \
        --retry 6 --retry-delay 5 --retry-all-errors \
        "https://gstreamer.freedesktop.org/src/$1/$1-$version.tar.xz" \
        -o "$build_dir/$1.tar.xz"
    printf '%s  %s\n' "$2" "$build_dir/$1.tar.xz" | sha256sum --check -
    tar -xJf "$build_dir/$1.tar.xz" -C "$build_dir"
}

fetch gstreamer "$core_sha256"
fetch gst-plugins-base "$base_sha256"
fetch gst-libav "$libav_sha256"

# Stack traces on errors (libunwind, libdw) stay off: Debian's runtime
# packages do not bring those libraries into the image.
# Core first, installed into the build image too, so plugins-base builds
# against it; the copy for the runtime image goes to the destination.
# shellcheck disable=SC2086
meson setup "$build_dir/core" "$build_dir/gstreamer-$version" $options \
    -Dbenchmarks=disabled -Dptp-helper=disabled -Dlibunwind=disabled -Dlibdw=disabled
ninja -C "$build_dir/core"
meson install -C "$build_dir/core" --quiet
meson install -C "$build_dir/core" --quiet --destdir "$destdir"

# Only what Debian's gst-plugins-base package carries: OpenGL and the X11
# sinks are separate Debian packages (and libraries the image does not have).
# shellcheck disable=SC2086
meson setup "$build_dir/base" "$build_dir/gst-plugins-base-$version" $options \
    -Dgl=disabled -Dx11=disabled -Dxvideo=disabled -Dxshm=disabled -Dxi=disabled
ninja -C "$build_dir/base"
meson install -C "$build_dir/base" --quiet
meson install -C "$build_dir/base" --quiet --destdir "$destdir"

# The libav plugin, over Debian's gstreamer1.0-libav file of the same name
# (gst-libav has no examples, introspection or translations).
# shellcheck disable=SC2086
meson setup "$build_dir/libav" "$build_dir/gst-libav-$version" $install_options
ninja -C "$build_dir/libav"
meson install -C "$build_dir/libav" --quiet --destdir "$destdir"

# Only what runs: no headers, pkg-config files or GIR data.
rm -rf "$destdir/usr/include" "$destdir/usr/share/gir-1.0" \
    "$destdir/usr/lib/$multiarch/pkgconfig" "$destdir/usr/share/gstreamer-1.0/gdb"

mkdir -p "$destdir/usr/local/share/noar" "$destdir/usr/local/share/licenses/noar-gstreamer"
printf 'GStreamer core, gst-plugins-base and gst-libav %s over Debian 1.26.2 (decodebin3 collection and avdec_h264 alignment fixes)\nSource SHA256: %s (core), %s (base), %s (libav)\n' \
    "$version" "$core_sha256" "$base_sha256" "$libav_sha256" > "$destdir/usr/local/share/noar/gstreamer-build.txt"
# Each part's own license (all LGPL-2.1-or-later), next to its sources' hashes.
for part in gstreamer gst-plugins-base gst-libav; do
    cp "$build_dir/$part-$version/COPYING" "$destdir/usr/local/share/licenses/noar-gstreamer/COPYING.$part"
done
