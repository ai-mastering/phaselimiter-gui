#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
python3 script/build-icons.py

if ! command -v cargo >/dev/null && [[ -f "$HOME/.cargo/env" ]]; then
    source "$HOME/.cargo/env"
fi

# Supply an SDK installed by Apple Xcode/Command Line Tools. This script never
# downloads SDKs or uses third-party SDK archives.
if [[ "$(uname -s)" == Darwin ]]; then
    export SDKROOT="${SDKROOT:-$(xcrun --sdk macosx --show-sdk-path)}"
else
    : "${SDKROOT:?Set SDKROOT to your locally supplied official MacOSX.sdk directory}"
fi
if [[ ! -f "$SDKROOT/SDKSettings.json" && ! -f "$SDKROOT/SDKSettings.plist" ]]; then
    echo "SDKROOT is not a macOS SDK directory: $SDKROOT" >&2
    exit 1
fi
export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-14.0}"
target="${MACOS_TARGET:-x86_64-apple-darwin}"
rustup target add "$target"
if [[ "$(uname -s)" == Darwin ]]; then
    cargo build --locked --release --target "$target"
else
    # Use the supplied SDK's stubs and record both the minimum OS and SDK
    # version explicitly. Clang and LLD are provided by the Linux distribution.
    sdk_version="$(python3 - "$SDKROOT" <<'PY'
import json, pathlib, plistlib, sys
sdk = pathlib.Path(sys.argv[1])
if (sdk / "SDKSettings.json").is_file():
    settings = json.loads((sdk / "SDKSettings.json").read_text())
else:
    with (sdk / "SDKSettings.plist").open("rb") as source:
        settings = plistlib.load(source)
print(settings["Version"])
PY
)"
    case "$target" in
        x86_64-apple-darwin) clang_arch=x86_64 ;;
        aarch64-apple-darwin) clang_arch=arm64 ;;
        *) echo "Unsupported macOS target: $target" >&2; exit 1 ;;
    esac
    # The GUI's miniaudio dependency compiles C and generates target bindings.
    export CC="${CC:-$(command -v clang-19 || command -v clang)}"
    export AR="${AR:-llvm-ar}"
    export CC_SHELL_ESCAPED_FLAGS=1
    # Target-specific variables take precedence over CFLAGS in cc-rs.
    # Keep the no-runtime-linking definition used by native Mac builds too.
    task_cflags_var="CFLAGS_${target//-/_}"
    task_bindgen_var="BINDGEN_EXTRA_CLANG_ARGS_${target//-/_}"
    export "$task_cflags_var=${!task_cflags_var:-} ${CFLAGS:-} -DMA_NO_RUNTIME_LINKING --target=$clang_arch-apple-macos$MACOSX_DEPLOYMENT_TARGET -isysroot \"$SDKROOT\""
    export "$task_bindgen_var=${!task_bindgen_var:-} ${BINDGEN_EXTRA_CLANG_ARGS:-} -DMA_NO_RUNTIME_LINKING -isysroot \"$SDKROOT\""
    cargo rustc --locked --release --target "$target" -- \
        -C linker=clang \
        -C "link-arg=--target=$clang_arch-apple-macos$MACOSX_DEPLOYMENT_TARGET" \
        -C link-arg=-isysroot -C "link-arg=$SDKROOT" \
        -C link-arg=-fuse-ld=lld \
        -C "link-arg=-Wl,-platform_version,macos,$MACOSX_DEPLOYMENT_TARGET,$sdk_version"
fi
python3 script/package-macos.py "$target" "$@"
