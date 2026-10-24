# phaselimiter-gui

A portable desktop frontend for [phaselimiter](https://github.com/ai-mastering/phaselimiter/tree/feature/codex), the audio mastering engine used by bakuage.com / aimastering.com.

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

The GUI opens without an engine installed. To process audio, supply the `feature/codex` backend, FFmpeg and `sound_quality2_cache`. Edit their paths under **Engine & resources**. FFmpeg accepts a full path or a name on PATH. Paths are passed directly as process arguments; do not add shell quotes.

The GUI supports target loudness, independent stereo matching, mastering intensity, bass preservation, pre-compression, true-peak ceiling, WAV/MP3/AAC, output sample rate, 16/24/32-bit WAV, DE-PRMM/CMA-ES, seed, evaluation budget and worker count. CMA-ES and the seed argument require the updated backend branch. It does not yet expose every backend option.

Default portable layout (use `.exe` on Windows):

```text
phaselimiter-gui
ffmpeg
phaselimiter/
  bin/phase_limiter
  resource/sound_quality2_cache
```

The backend's dynamic libraries and any codec dependencies must also be packaged. Building this repository builds the GUI only, not the DSP backend. The backend has its own CPU and platform requirements.

## Windows x86-64

The GUI statically links the MSVC C runtime (`crt-static` in `.cargo/config.toml`). Its aim is to run on Windows 10/11 without a separate Visual C++ Redistributable installation. This does not cover the separately distributed DSP backend.

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

Keep the extracted SDK outside the repository. `.local-sdk/`, `target/` and `dist/` are ignored by Git. A local cross-build cannot verify macOS windowing behavior; run the result on your Mac. Future Apple Silicon GUI builds can use `MACOS_TARGET=aarch64-apple-darwin`; the DSP backend needs separate ARM support.

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

GitHub Actions builds and tests on Linux, Windows and Intel macOS. The Mac job uses the runner's installed Xcode/official SDK in place and uploads the GUI application, not the SDK. The build artifacts currently contain the GUI only; they are not complete audio-processing distributions.

## License

MIT; see [LICENSE](LICENSE). Dependencies retain their own licenses. This Rust port replaces the former Go/GTK frontend.
