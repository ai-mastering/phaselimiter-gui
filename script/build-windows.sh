#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
python3 script/build-icons.py
if ! command -v cargo >/dev/null && [[ -f "$HOME/.cargo/env" ]]; then
    source "$HOME/.cargo/env"
fi
if [[ "$(uname -s)" != MINGW* && "$(uname -s)" != MSYS* ]]; then
    if ! command -v cargo-xwin >/dev/null && [[ -x /opt/phaselimiter-cross/bin/cargo-xwin ]]; then
        export PATH="/opt/phaselimiter-cross/bin:$PATH"
    fi
    export XWIN_CACHE_DIR="${XWIN_CACHE_DIR:-/tmp/phaselimiter-xwin-cache}"
    export XWIN_ARCH=x86_64
    # winresource uses llvm-rc when cross-compiling the executable icon.
    if [[ -z "${RC_PATH:-}" ]]; then
        for resource_compiler in llvm-rc llvm-rc-{22..14}; do
            if command -v "$resource_compiler" >/dev/null; then
                export RC_PATH="$(command -v "$resource_compiler")"
                break
            fi
        done
        if [[ -z "${RC_PATH:-}" ]]; then
            echo "Install LLVM's llvm-rc or set RC_PATH to its executable." >&2
            exit 1
        fi
    fi
    rustup target add x86_64-pc-windows-msvc
    cargo xwin build --locked --release --target x86_64-pc-windows-msvc
else
    cargo build --locked --release --target x86_64-pc-windows-msvc
fi
python3 script/package-windows.py
