#!/usr/bin/env bash
# Native Unix builds, or MinGW-w64 from the MSYS2 shell on Windows.
set -euo pipefail
cd "$(dirname "$0")/.."
repo="$PWD"
work="${1:-build/audio}"
mkdir -p "$work"
work="$(cd "$work" && pwd -P)"
jobs="${BUILD_JOBS:-2}"
python_bin="${PYTHON:-python3}"
source_path() {
    local value
    value="$("$python_bin" "$repo/script/build-audio.py" source "$1" --work "$work")" || return
    value="${value//$'\r'/}"
    if [[ -n "${MSYSTEM:-}" ]]; then cygpath -u "$value"; else echo "$value"; fi
}
lame="$(source_path lame)"
zlib="$(source_path zlib)"
ffmpeg="$(source_path ffmpeg)"
prefix="$work/ffmpeg-prefix"
mkdir -p "$prefix" "$work/build/lame" "$work/build/ffmpeg"

export PKG_CONFIG_LIBDIR="$prefix/lib/pkgconfig"
export PKG_CONFIG_PATH="$PKG_CONFIG_LIBDIR"
export CFLAGS="${CFLAGS:-} -I$prefix/include"
export LDFLAGS="${LDFLAGS:-} -L$prefix/lib"
# Guard empty array expansions below for macOS Bash 3.2 with nounset enabled.
extra=()
host=()
if [[ -n "${MSYSTEM:-}" ]]; then
    export CC=gcc AR=ar RANLIB=ranlib
    export LDFLAGS="$LDFLAGS -static -static-libgcc"
    extra+=(--target-os=mingw32 --extra-ldflags="-L$prefix/lib -static -static-libgcc")
elif [[ "$(uname -s)" == Darwin ]]; then
    export CC=/usr/bin/clang AR=/usr/bin/ar RANLIB=/usr/bin/ranlib
    export SDKROOT="$(xcrun --sdk macosx --show-sdk-path)"
    export MACOSX_DEPLOYMENT_TARGET=14.0
    export CFLAGS="$CFLAGS -mmacosx-version-min=14.0"
    extra+=(--extra-ldflags="$LDFLAGS -Wl,-headerpad_max_install_names")
    if [[ "$(uname -m)" == arm64 ]]; then
        # LAME 3.100's old config.guess predates Apple Silicon.
        host+=(--host=aarch64-apple-darwin)
    fi
fi
cd "$work/build/lame"
"$lame/configure" --prefix="$prefix" --disable-shared --enable-static \
    --with-pic --disable-frontend --disable-dependency-tracking ${host[@]+"${host[@]}"}
make -j"$jobs"
make install

# zlib's configure requires an in-source build. Use an isolated copy.
if [[ ! -d "$work/build/ffmpeg-zlib" ]]; then cp -R "$zlib" "$work/build/ffmpeg-zlib"; fi
cd "$work/build/ffmpeg-zlib"
./configure --static --prefix="$prefix"
make -j"$jobs"
make install

cd "$work/build/ffmpeg"
"$ffmpeg/configure" --prefix="$prefix" \
    --disable-autodetect --disable-gpl --disable-nonfree --disable-version3 \
    --disable-shared --enable-static --enable-libmp3lame --enable-zlib \
    --disable-network --disable-devices --disable-hwaccels --disable-ffplay \
    --disable-doc --disable-debug --disable-encoders --pkg-config-flags=--static \
    --enable-encoder=pcm_s16le,pcm_s24le,pcm_f32le,flac,alac,aac,libmp3lame,png,wrapped_avframe \
    --extra-cflags="-I$prefix/include" --extra-ldflags="-L$prefix/lib" ${extra[@]+"${extra[@]}"}
make -j"$jobs"
make install
