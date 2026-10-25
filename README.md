# phaselimiter-gui

A portable desktop frontend for [phaselimiter](https://github.com/ai-mastering/phaselimiter/tree/feature/win_build), the audio mastering engine used by bakuage.com / aimastering.com.

The GUI uses Rust, iced 0.14 and the tiny-skia CPU renderer. It does not require GTK, Qt, a WebView or a GPU rendering API. Linux still uses X11/Wayland libraries. Audio processing runs locally in a separate backend process.

## Build and run

Install Rust using [rustup](https://rustup.rs/). The repository pins its toolchain in `rust-toolchain.toml`.

```sh
cargo run --locked --release
```

On Debian/Ubuntu, development packages for Linux builds are:

```sh
sudo apt-get install build-essential pkg-config libx11-dev libx11-xcb-dev \
  libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev
```

The devcontainer installs these, Rust, and the macOS cross-build tools. They are build tools; users of a packaged application do not need Rust.

## Use

1. Set the output directory and mastering settings.
2. Drop audio files onto the window, or paste an absolute file path and click **Add file**.
3. Click **Start queue**. Files are processed sequentially.
4. Select a job's filename to view its backend log. **Cancel current & pause** stops the active job and leaves the remaining queue waiting.

Settings are captured when a file is added. Change settings before adding files, or remove and re-add a queued file. The GUI chooses unused `_output` filenames and refuses an existing output at processing start. Failed and cancelled jobs retain their logs; add their input again to retry. Settings are currently kept only for the lifetime of the window.

The CI distribution includes the backend, FFmpeg and a sound-quality cache. The GUI also opens without an engine installed when built with Cargo alone. Edit paths under **Engine & resources** to use external tools. FFmpeg accepts a full path or a name on PATH. Paths are passed directly as process arguments; do not add shell quotes.

The GUI supports target loudness, independent stereo matching, mastering intensity, bass preservation, pre-compression, true-peak ceiling, WAV/MP3/AAC, output sample rate, 16/24/32-bit WAV, DE-PRMM/CMA-ES, seed, evaluation budget and worker count. CMA-ES and the seed argument require the updated backend branch. It does not yet expose every backend option.

Default portable layout (use `.exe` on Windows):

```text
phaselimiter-gui
ffmpeg
phaselimiter/
  bin/phase_limiter
  resource/sound_quality2_cache
```

`cargo build` builds the GUI only. The CI builds the complete layout, including backend libraries and licenses. The x86-64 backend requires AVX (Sandy Bridge or newer); Apple Silicon uses NEON and the system Accelerate framework.

## Windows x86-64

The GUI statically links the MSVC C runtime (`crt-static` in `.cargo/config.toml`). The CI builds the backend and its libraries with the static CRT too, and rejects unexpected Visual C++ runtime DLL imports. The complete distribution targets Windows 10 1903 or later, uses UTF-8 backend manifests, and includes required oneTBB/OpenMP DLLs. No separate Visual C++ Redistributable is intended to be necessary.

From the Linux devcontainer:

```sh
bash script/build-windows.sh
```

This uses pinned `cargo-xwin` and Clang/LLD. Windows SDK and CRT packages are downloaded from Microsoft's distribution servers into `/tmp/phaselimiter-xwin-cache`, outside the repository. First-time setup downloads and unpacks the SDK; later builds reuse the cache. On other Linux environments, install `cargo-xwin`, Clang (including a `clang-cl` driver) and LLD first.

The GUI-only archive is `dist/x86_64-pc-windows-msvc/Phaselimiter-windows-x86_64.zip`. Extract it and run `phaselimiter-gui.exe`. It contains the executable and license notices; it does not contain the DSP backend or FFmpeg.

Inspect normal and delayed DLL imports with:

```sh
python3 script/analyze-windows.py target/x86_64-pc-windows-msvc/release/phaselimiter-gui.exe
```

This inspects PE tables, not runtime `LoadLibrary` calls. Windows-native execution and D&D testing are separate from a successful Linux cross-build.

## Intel Mac / Sonoma

Native build on your Mac, using Apple Xcode or Command Line Tools:

```sh
bash script/build-macos.sh
```

The script targets `x86_64-apple-darwin`, sets the minimum macOS version to 14.0, and creates:

```text
dist/x86_64-apple-darwin/Phaselimiter.app
dist/x86_64-apple-darwin/Phaselimiter-macos.zip
```

Open `Phaselimiter.app` on the Mac. This is an unsigned local build; no installer or App Store is involved. The macOS GUI backend defaults point to `Phaselimiter.app/Contents/Resources/phaselimiter/` and the sibling `ffmpeg`. You can instead enter external paths in the GUI.

### Cross-build from the Linux devcontainer

Use an SDK installed by **Apple's official Xcode/Command Line Tools**. No SDK is downloaded by this repository's scripts. On your Mac, from this repository's directory:

```sh
sdk_path="$(xcrun --sdk macosx --show-sdk-path)"
sdk_path="$(cd "$sdk_path" && pwd -P)"
mkdir -p .local-sdk
tar -czf .local-sdk/macos-sdk.tar.gz -C "$sdk_path" .
```

The archive stays in the shared `.local-sdk/` directory. Inside the devcontainer, extract it into `/tmp`, outside the shared repository:

```sh
mkdir -p /tmp/phaselimiter-official-sdk/MacOSX.sdk
tar -xzf .local-sdk/macos-sdk.tar.gz -C /tmp/phaselimiter-official-sdk/MacOSX.sdk
export SDKROOT=/tmp/phaselimiter-official-sdk/MacOSX.sdk
bash script/build-macos.sh
```

Keep the extracted SDK outside the repository. `.local-sdk/`, `target/` and `dist/` are ignored by Git. A local cross-build cannot verify macOS windowing behavior; run the result on your Mac. Apple Silicon GUI builds can use `MACOS_TARGET=aarch64-apple-darwin`. The complete CI distribution builds and tests the ARM backend natively.

## Tests and CI

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

Tests cover configuration validation, output collisions, Unicode paths, argument handling, backend stdout/stderr, failure/cancellation, and headless GUI interactions. No backend installation or display server is needed for these tests.

For a real X11 file drop through the windowing protocol, install `xvfb xauth python3-xlib`, build the GUI, and run:

```sh
cargo build --locked --release
xvfb-run -a python3 script/test-x11-drop.py target/release/phaselimiter-gui
```

This test drops a filename containing Japanese characters and spaces through XDND and checks that it reaches the queue. It does not simulate the drop by directly calling application code. Wayland D&D, IME, and Finder/Explorer D&D still need their own platform tests.

GitHub Actions builds complete distributions on four native runners:

| Target | Runner / baseline | Archive |
| --- | --- | --- |
| Linux x86-64 | Ubuntu 22.04; actual glibc requirements in `runtime-versions.txt` | `.tar.gz` |
| Windows x86-64 | Windows 2022; Windows 10 1903+ target | `.zip` |
| Intel macOS | macOS 15 Intel; macOS 14+ target | `.app` in `.zip` |
| Apple Silicon macOS | macOS 15 ARM64; macOS 14+ target | `.app` in `.zip` |

Each archive contains the GUI, self-built LGPL FFmpeg (including LAME), all eight backend tools, a cache generated by that target's analyzer, runtime libraries and license notices. Linux bundles oneTBB, OpenMP, libstdc++ and libgcc_s; glibc and desktop X11/Wayland libraries come from the OS. Mac bundles oneTBB/OpenMP and uses the system C++ runtime. Mac builds use the runner's official Xcode SDK in place. Apps use an ad-hoc signature for relocated executable code, without a signing certificate or notarization. No SDK is published.

The workflow runs GUI tests, real Linux XDND, the backend's CTest suite, and tests the extracted archive in a relocated Japanese/emoji path with development runtime search paths removed. Acceptance tests cover codecs, resampling, spectrum PNG output and short DE/CMA-ES mastering jobs. The upstream auxiliary visualizer test alone uses a separate OS-installed test FFmpeg because it requires GPL libx264; this test tool is not packaged. Audio and distribution tests use our LGPL build. Windows also checks UTF-8 manifests and WAV/MP3/AAC processing with Unicode paths. Finder/Explorer and Wayland D&D are not automated by this workflow.

Source downloads are SHA-256 checked. The backend revision and source versions are fixed in [packaging/audio-sources.json](packaging/audio-sources.json); updating the branch alone does not change the packaged backend. A separate `phaselimiter-corresponding-sources` artifact contains the backend/submodule source snapshot, dependency source archives, GUI source and build scripts. Keep this source artifact with binary releases; see [packaging/README.md](packaging/README.md) for native reproduction commands and release contents. GitHub Actions artifacts are built on pushes, pull requests and manual runs; this workflow does not publish releases.

## License

MIT; see [LICENSE](LICENSE). Dependencies retain their own licenses. This Rust port replaces the former Go/GTK frontend.
