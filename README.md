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
sudo apt-get install build-essential clang libclang-dev pkg-config libx11-dev libx11-xcb-dev \
  libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev
```

The devcontainer installs these, Rust, and the macOS cross-build tools. They are build tools; users of a packaged application do not need Rust.

## Use

1. Drop audio files onto the window, or paste an absolute file path and click **Add audio**. Each import stores an independent copy, including repeated imports of the same path.
2. Select an original in **Audio & history** to listen and inspect its analysis. `audio_analyzer` is found beside the configured `phase_limiter` executable. FFmpeg converts compressed inputs to PCM WAV before analysis.
3. Click **Master this input**, review settings, and click **Master with these settings**. Further runs create additional results below the same input. New runs inherit the current settings; selecting history never changes them. **Use these settings** explicitly recalls a previous run, including its saved reference.
4. Click any original or mastering result to switch playback at the same sample position with a 10 ms crossfade. Preparation of an uncached selection leaves the current audio playing. Originals and recent results are prepared in the background. Selecting a result displays original/mastered measurements and overlaid frequency, loudness and histogram plots. Use **Original / Mastered** to compare that run's input and output without leaving its details.
5. Click the waveform to seek, or drag to loop a range. The range affects playback only; mastering always processes the whole input. **Match loudness** uses a fixed -18 LUFS target with a -1 dBTP headroom cap (and at most 24 dB gain); headroom-limited tracks may remain quieter. Matching requires completed analysis.
6. Open **Processing details** for the exact settings, limiting error, limiting-error spectrogram and backend log. **Export a copy** writes the result to a chosen filename and refuses an existing destination.

Waveform and graph data are cached, with up to 100 static drawing geometry entries per plot widget; playback-position and loop overlays update independently. History/settings snapshots are coalesced and written on a background worker; normal exit flushes pending writes.

Audio, reference analysis snapshots, results, analysis and history are saved automatically in the OS application-data directory: `$XDG_DATA_HOME/phaselimiter-gui` (default `~/.local/share/phaselimiter-gui`) on Linux, Local AppData on Windows, and Application Support on both Mac architectures. Windows uses Known Folder resolution; macOS uses Foundation's Application Support API. Restart restores history and settings with playback and the queue stopped. An unfinished run is marked interrupted; start a new mastering from its input to retry. A missing or damaged analysis report is shown as an error on that audio while the rest of the library remains usable; run analysis again to recover. The broken reference is retained when saving, and historical mastering references are never silently replaced.

External originals and exported copies are not monitored. Reimport updated audio as a new item. Source overwrites/deletions do not change stored history. Audio checksums are checked before analysis, playback decoding and processing, and results before export. Ready playback cache entries reuse the verified PCM when the stored source size and modification time are unchanged; changed metadata triggers verification; changed or missing stored audio produces an error instead of silently replacing history. Removing an input removes its processing history and owned files; exported copies are retained. Stored audio is permanent application data, not an automatically cleared cache.

The distribution includes the backend, FFmpeg and a sound-quality cache. The GUI also opens without an engine installed. Configure paths under **Settings** to use external tools. FFmpeg accepts a full path or a name on PATH. Paths are passed directly as process arguments; do not add shell quotes. Missing tools are reported in the analysis or processing details.

Mastering supports target loudness, stereo matching, intensity, bass preservation, pre-compression, true-peak ceiling, WAV/MP3/AAC, sample rate, 16/24/32-bit WAV, DE-PRMM/CMA-ES, seed, evaluation budget and worker count. Choose any analyzed library input as a custom mastering5 reference. Analysis reports are immutable shared files in `analyses/`, named by their SHA-256 checksum. `library.json` stores relationships, settings and report references, without duplicating report contents. Each run references the exact report it used; removing the reference input does not remove that report. Only the current shared-report storage format is supported; older inline reports and per-run reference copies are not migrated. Unused shared reports are not automatically deleted. Backend debug histogram mode is not enabled: it waits for interactive input. The processing image shown is a limiting-error spectrogram, not a limiter-error histogram.

Playback uses miniaudio and FFmpeg, without an external player. The private `playback/native.rs` module owns all miniaudio device access. Every platform initializes the audio context before allocating a device; device handles cannot be cloned or reconfigured by other modules. macOS links the system CoreAudio/AudioUnit/AudioToolbox frameworks directly rather than probing relative framework paths. Audio initialization errors and Rust panics are recorded in `audio-diagnostics.log` in the library directory. Decoded PCM is kept in disposable disk-backed files. Per-source workers prefetch half a second into bounded PCM queues; a mixer supplies a lock-free output queue sized to the device callback. The audio callback performs no file I/O, blocking locks, or allocation. The disk-backed PCM cache retains the 100 most recently used entries. The mixer separately retains eight sources with RAM prefetch buffers; currently fading sources stay alive until their fade ends. In-flight decoding and sources retained by the mixer can temporarily keep additional PCM files alive. This adds a C compiler and libclang requirement for developers; Linux audio systems are loaded dynamically at runtime. No audio device is opened until Play is pressed. The application does not yet provide partial mastering, automatic file tracking, library relocation, or automatic alignment of separately exported mixes.

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

## Crash boundaries

Application code denies `unsafe`; the sole exception is the small Unix process-group cleanup module (`src/backend/process_group.rs`). Audio device ownership and callback setup are isolated in `src/playback/native.rs`. File errors, unavailable backends and audio worker creation failures are reported as errors. The callback only consumes PCM and atomics, and checks the sample format before typed access.

Rust memory safety depends on the correctness of native libraries and dependency `unsafe` implementations. Native audio drivers and miniaudio still execute in the GUI process; their memory faults cannot be recovered with `Result` or `catch_unwind`. FFmpeg, the analyzer and the mastering engine run in child processes. Out-of-memory aborts and bugs in dependencies are not eliminated by these boundaries.
