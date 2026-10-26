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

1. Set **Mastering settings**, then click **Master audio…** to select one or more files, or drop audio files onto the window. To paste an absolute path, choose **File → Master audio from path…** and press Enter or click **Master**. Each file is copied independently and immediately queued with the settings and tools captured when submitted. Invalid settings block submission and display a reason.
2. **Audio & history** lists mastering results, including queued, running and failed attempts, newest first. Originals remain stored for analysis and A/B comparison but have no history rows. Select a result to listen, inspect its analysis and recall its settings. `audio_analyzer` is found beside the configured `phase_limiter` executable; FFmpeg converts compressed inputs to PCM WAV before analysis.
3. Adjust the selected result's settings and click **Create variation**. This processes its original input again, creates another result, and preserves earlier results and the playback selection. Additional jobs wait behind the active run. A collapsible change count appears immediately above **Create variation**; expanding it scrolls to the differences in the settings area.
4. Click a mastering result to switch playback at the same sample position with a 10 ms crossfade. Preparation of an uncached selection leaves the current audio playing. Originals and recent results are prepared in the background. Choose **Compare A** (original or another completed result of the same input), then use **A / B** to audition it against the selected result. The measurement table retains the original column and adds A and the saved reference when used. Spectrum, loudness and histogram graphs overlay A, B and reference; **Distribution** shows available frequency distribution images.
5. Click the waveform to seek, or drag to loop a range. The range affects playback only; mastering always processes the whole input. **Match loudness** follows app.bakuage.com: it attenuates the louder member of the current A/B pair to match the quieter one, without amplifying either. A is the original or the selected **Compare A** result; B is the selected mastering result. Matching requires loudness measurements for both sides.
6. Limiting error appears in the measurement table. Open **Processing details** for the exact settings, recorded execution command (with a copy button), limiting-error spectrogram and backend log. The log preview shows the most recent 400 lines; the complete log is retained in history and **Copy log** copies all retained lines. **Export a copy** beside the selected result opens the destination field; **Export** writes a copy. If the destination already exists, it adds a number before the extension (for example, `audio (1).wav`, `audio (2).wav`), choosing the first available filename without overwriting existing files.

The desktop workspace always shows history on the left, playback and analysis in the center, and mastering settings on the right. Resizing keeps this arrangement and the waveform visible; the center expands while the side panes retain their widths. The minimum window size is 1000 × 480 logical pixels. The Run button reserves 40 logical pixels; settings and expanded differences share one scroll area. Loudness and peak settings remain stacked. History is always visible. Scrollbars reserve their own layout space instead of overlaying controls or plots. The window-local **File / Edit / Tools / Playback** menu keeps application commands above the workspace. **Master audio…** stays at the top of history, with a persistent hint that audio files can be dropped anywhere in the main window. **Tools → Options** opens a separate modal dialog for engine paths, export location and storage information. **OK** saves the draft; **Cancel**, Esc and the dialog close button discard it. The parent workspace stays blocked while playback and background processing continue. Queued and running mastering jobs retain their captured settings. **Edit → Remove selected** removes the current history selection; right-clicking a history entry selects it and opens its context menu. F10 or Alt focuses the menu bar; Down opens it, and Alt+F/E/T/P opens a specific menu. Arrows navigate, Enter or a mnemonic activates an item, and Esc returns to the bar before leaving menu navigation. An outside click dismisses the menu. Ctrl+O selects new audio to master (Cmd+O also works on macOS). **Clear paths / use bundled tools** empties the draft engine, FFmpeg and cache overrides; press **OK** to apply. Each empty field resolves to the currently running application's bundled tool or resource at use time; a non-empty field must be an absolute path and is used exactly as specified. New settings store empty overrides rather than absolute bundle paths. Recalling a historical run preserves these application-level overrides. The bottom status bar collects errors, notices, import/analysis/playback preparation state and queue progress. The status bar is display-only; queue resume and cancellation are in **Tools**. **Tools → Diagnostics** opens an independent, nonmodal window with a JSON snapshot of current state, errors, selected/active job log tails (up to 200 lines each), and the audio log tail (up to 64 KiB). **Copy report** copies the entire snapshot; **Refresh** captures it again. Invalid inputs stay highlighted.

Application preferences (`app_settings`: tool overrides and default export directory), the one current mastering draft (`draft`), and each job's mastering parameters (`config`) and resolved execution tools (`tools`) are stored separately. The export directory is used only to suggest a destination at export time; it never blocks mastering or gets recalled from a run.

The analysis table includes loudness, loudness range, peak/true peak/15 kHz lowpass true peak, RMS, Professionality and Professionality2 (percent), YouTube loudness and estimated compensation, micro dynamics, space, acoustic entropy and dissonance. Ear Damage is omitted. Compensation follows the website's historical `min(0, -10.3 - youtube_loudness)` formula. Professionality is calculated by `audio_detector detect2` using the analyzer JSON, `resource/detection_preparation.json` and its `analysis_data` corpus. The detector runs beside `phase_limiter`, with the legacy web app's neighbor ratio 0.01 and outlier ratio 0.1. `audio_analyzer`'s v1 score is disabled; Professionality2 continues to use its `sound_quality2` output. Detector errors leave other measurements available and are shown beside the table. The detector result and parameters are saved in the analysis report. Existing reports are retained unchanged; use **Reanalyze** to populate missing scores or replace earlier analyzer-based scores with detector scores. Missing values display as a dash. Historical references load from their saved analysis snapshot, including after the reference input is removed.

Mastering history uses the queue registration date/time, displayed in local time, followed by an optional memo. Edit the memo on the selected result; it saves automatically after a short pause, on Enter, when switching results, and on normal exit. Memos do not affect processing settings, ordering, or exports. Long memos are shortened in labels; the editor retains the full text. Times are displayed through seconds, including in result details. Results within the same second may share a label; use memos to distinguish them. History omits parameter summaries and ordinary status text; processing uses a progress bar, while failed or interrupted runs have a red label and an exclamation mark. Internal identities are UUID v4 values, independent of timestamps and memos.

Waveform and graph data are cached, with up to 100 static drawing geometry entries per plot widget; playback-position and loop overlays update independently. History/settings snapshots are coalesced and written on a background worker; normal exit flushes pending writes.

Audio, reference analysis snapshots, results, analysis and history are saved automatically in the OS application-data directory: `$XDG_DATA_HOME/phaselimiter-gui` (default `~/.local/share/phaselimiter-gui`) on Linux, Local AppData on Windows, and Application Support on both Mac architectures. Windows uses Known Folder resolution; macOS uses Foundation's Application Support API. Restart restores history and settings with playback and the queue stopped. An unfinished run is marked interrupted; start a new mastering from its input to retry. A missing or damaged analysis report is shown as an error on that audio while the rest of the library remains usable; run analysis again to recover. The broken reference is retained when saving, and historical mastering references are never silently replaced.

External originals and exported copies are not monitored. Reimport updated audio as a new item. Source overwrites/deletions do not change stored history. Audio checksums are checked before analysis, playback decoding and processing, and results before export. Ready playback cache entries reuse the verified PCM when the stored source size and modification time are unchanged; changed metadata triggers verification; changed or missing stored audio produces an error instead of silently replacing history. Removing a result deletes its owned output; removing the last result for an input also removes the stored input. Exported copies are retained. Stored audio is permanent application data, not an automatically cleared cache.

The distribution includes the backend, FFmpeg and a sound-quality cache. The GUI also opens without an engine installed. Configure paths under **Tools → Options** to use external tools. Engine, FFmpeg and sound-quality cache overrides accept absolute paths only; leaving a field blank selects the bundled tool or resource. Command names and relative paths are rejected without searching PATH. Existing non-empty saved paths are retained until cleared or edited. Paths are passed directly as process arguments; do not add shell quotes. Missing tools are reported through the status bar and processing details.

Mastering supports automatic mastering on/off, a shared mastering intensity, bass preservation (on by default), pre-compression, selectable volume reference and peak control, low/high-cut frequencies, oversampling, limiter iteration limits and linear/adaptive noise updates, WAV/MP3/AAC, sample rate, 16/24/32-bit WAV, DE-PRMM/CMA-ES, seed, evaluation budget and worker count. Use **Use reference audio** to enter an external audio path or choose a file with **Browse…**. Unchecked means **Auto (no reference)** and omits the backend reference argument. Manual edits start reference analysis after 500 ms without further typing; Enter or leaving the field starts it sooner, and browsing starts it immediately. Mastering is unavailable until the enabled reference is ready. Editing the path or disabling the reference cancels its analysis; normal and reference analyses run one at a time. Reference analysis skips both quality scores and supplies the resulting feature JSON to mastering5. The draft and new history settings retain the source audio path; loading settings checks that file again. A missing or changed source is never silently replaced with an old report. Historical measurement displays retain the exact report used by the run. Removing history does not remove the current draft reference. A frequency distribution image is generated during analysis and displayed for the selected audio; reanalyze existing audio to add it. Both cut frequencies must be zero to disable filtering; high-cut zero alone is unsupported by the backend. Internal limiter oversampling stays at 1. Analysis reports are immutable shared files in `analyses/`, named by their SHA-256 checksum. `library.json` stores relationships, settings and report references, without duplicating report contents. Each run references the exact report it used; removing the reference input does not remove that report. Unused shared reports are not automatically deleted.

Analysis has a separate, rebuildable disk cache in the OS cache directory under `phaselimiter-gui/analysis`. Keys contain the source-file SHA-256, analyzer and FFmpeg executable SHA-256 values, conversion/analysis options and a processing-recipe revision. When enabled, Sound Quality 2 adds the model-cache SHA-256 and the fixed `binary` archive format; detect2 adds its executable, parameters, preparation JSON and all referenced corpus-file SHA-256 values. Disabled scores do not depend on their models. Direct executable/data dependencies are covered; operating-system and shared-library versions are not. Missing dependencies disable cache reuse while retaining the existing partial-analysis/error behavior. Failed or partial quality results are not cached as successes. **Reanalyze** bypasses this cache; existing historical reports are never rewritten. Deleting the cache does not delete reports referenced by history. Backend debug histogram mode is not enabled: it waits for interactive input. The processing image shown is a limiting-error spectrogram, not a limiter-error histogram.

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

Keep the extracted SDK outside the repository. `.local-sdk/`, `target/` and `dist/` are ignored by Git.

Build and packaging scripts honor `CARGO_TARGET_DIR` and Cargo's configured target directory. In the root-user Linux devcontainer, set `export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-/root/.cache/phaselimiter-target}"` before running Cargo or build/packaging scripts, keeping build artifacts off the shared workspace. For the executable inspection commands in this document, replace `target/` with that directory when overridden.

A local cross-build cannot verify macOS windowing behavior; run the result on your Mac. Apple Silicon GUI builds can use `MACOS_TARGET=aarch64-apple-darwin`. The complete CI distribution builds and tests the ARM backend natively.

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

Complete archive contents, native reproduction commands, CI caches and acceptance tests are documented in [packaging/README.md](packaging/README.md). Source versions and the backend revision are pinned in [packaging/audio-sources.json](packaging/audio-sources.json). Keep the corresponding-source artifact alongside binary releases.

GitHub Actions builds artifacts on pushes, pull requests and manual runs; it does not publish releases. Finder/Explorer and Wayland D&D are not automated.

## License

MIT; see [LICENSE](LICENSE). Dependencies retain their own licenses. This Rust port replaces the former Go/GTK frontend.

## Crash boundaries

Application code denies `unsafe`; the sole exception is the small Unix process-group cleanup module (`src/backend/process_group.rs`). Audio device ownership and callback setup are isolated in `src/playback/native.rs`. File errors, unavailable backends and audio worker creation failures are reported as errors. The callback only consumes PCM and atomics, and checks the sample format before typed access.

Rust memory safety depends on the correctness of native libraries and dependency `unsafe` implementations. Native audio drivers and miniaudio still execute in the GUI process; their memory faults cannot be recovered with `Result` or `catch_unwind`. FFmpeg, the analyzer and the mastering engine run in child processes. Out-of-memory aborts and bugs in dependencies are not eliminated by these boundaries.
