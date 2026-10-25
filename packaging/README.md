# Native portable builds

`.github/workflows/build.yml` is the executable reference for the four native environments. It builds LGPL FFmpeg (GPL/nonfree/version3 disabled), static LAME/zlib, phaselimiter and its dependencies, then packages and tests the result. The backend is pinned to `feature/win_build`'s revision in `audio-sources.json`, not to its moving branch tip. Dependency source downloads and the backend's IPP downloads are checksum verified.

Use Python 3.12, CMake 3.27.9, Ninja 1.13.0, the pinned Rust toolchain, and a native C/C++ toolchain. Linux additionally needs NASM, pkg-config, patchelf and the GUI's X11/Wayland development packages. Mac uses Apple Clang and the official installed Xcode SDK, plus NASM/pkg-config. Windows uses the VS 2022 x64 developer environment for the backend, and MSYS2 MINGW64 GCC/make/NASM/pkgconf for FFmpeg. Do not build the MSVC backend from the MinGW shell.

From the GUI checkout, after cloning the exact pinned backend revision and its recursive submodules into `build/audio/backend`:

```sh
python -m pip install cmake==3.27.9 ninja==1.13.0
# Unix shell; on Windows run only this command in MSYS2 MINGW64.
bash script/build-ffmpeg.sh
# Native environment, including the VS developer environment on Windows:
python script/build-audio.py dependencies
python script/build-audio.py backend
python script/test-audio.py
cargo build --locked --release --target <rust-target>
python script/package-distribution.py --target <rust-target>
python script/test-distribution.py --archive dist/<rust-target>/Phaselimiter-<rust-target>.<zip-or-tar.gz>
```

The default work directory is `build/audio`; each script accepts `--work` (the shell script takes the directory as its first argument). `BUILD_JOBS` defaults to 2 for hosted runners. For local backend builds, set it to your available CPU count. macOS builds target 14.0. GUI-only cross-build scripts remain separate and do not produce complete audio distributions.

If your MSYS2 shell exposes the native Python only as `python`, run the FFmpeg command as `PYTHON=python bash script/build-ffmpeg.sh`, as the workflow does.

The backend test step runs CTest and downloads its pinned analysis dataset, then generates `sound_quality2_cache` using the newly built native analyzer. Dataset JSON sources are recorded in the backend's `environment/dataset.json`. The extracted-distribution test removes SDK/Conda/library search paths and gives subprocesses only system directories on PATH. On Windows it also runs the backend's own embedded-manifest and Unicode-path acceptance test. Native GUI windowing is covered by real X11 D&D on Linux; Mac/Windows GUI tests are headless.

CI separates backend checks into `test-audio.py --phase cli`, `--phase unit` and `--phase cache` so the Actions step identifies the active phase even if a runner disconnects and its logs are unavailable. CTest streams verbose output, runs one test at a time and defaults to a 15-minute timeout per test. Dataset checkout has a 10-minute timeout; cache generation uses two workers and a 15-minute timeout. Running without `--phase` still runs all phases. When changing compiler initialization flags, use a fresh backend CMake build directory (`build/audio/build/backend`); existing CMake caches retain their old flags.

Install a test-only FFmpeg with `libx264` on the host PATH (`apt install ffmpeg`, `brew install ffmpeg`, or `choco install ffmpeg`). The unchanged upstream `auxiliary-cli` test checks PPM-to-H.264 encoding and runs with this host tool. The other CTest checks and extracted-distribution tests use the self-built LGPL FFmpeg. The host test tool is never packaged; the distributed FFmpeg supports audio processing and spectrum PNGs, not H.264 encoding.

`runtime-dependencies.json` records imports, and Linux's `runtime-versions.txt` records symbol versions. The Linux GUI loads X11/Wayland desktop libraries from the OS. Runtime inspection does not enumerate arbitrary `dlopen`/`LoadLibrary` possibilities; real codec/mastering tests exercise the shipped configuration. Mac apps and relocated Mach-O files are ad-hoc signed, without a certificate, notarization or installer.

# Source and license artifacts

Publish `Phaselimiter-sources.tar.gz` alongside all four binary archives. The Linux packaging step (`--sources`) produces this shared source archive with:

- The exact backend and recursively pinned submodule sources, with `SOURCE-REVISION` identifying the snapshot.
- Unmodified, checksum-verified dependency source archives (Boost, FFmpeg, LAME, zlib, libsndfile, libpng, libjpeg-turbo, oneTBB, OpenMP and LLVM CMake helpers), plus the Armadillo header package.
- The GUI source, Cargo lockfile, native build scripts and version manifest.

To rebuild from that archive, extract it, use `gui/` as the repository, create `gui/build/audio/`, move `backend/` there and move `downloads/` there. The scripts accept the source snapshot's revision marker in place of `.git`. Run the native commands above. Rust crate sources are fetched according to Cargo.lock. IPP is a proprietary redistributable SDK: x86 builds retrieve the exact pinned Intel packages in the backend manifest; these and Apple's SDK are not placed in the source archive. Apple Silicon does not use IPP.

Binary distributions contain GUI notices, backend notices, dependency licenses, the actual downloaded IPP license when applicable, and build/source metadata. FFmpeg's libraries, LAME, libsndfile and media helpers are linked statically into command-line tools; the supplied sources and build scripts allow rebuilding those tools with modified libraries. The GUI remains MIT and invokes the tools as separate processes. Keep notices and corresponding sources available when redistributing. Linux additionally ships the toolchain's libstdc++/libgcc_s with GCC runtime license/exception notices; its system glibc is not redistributed.
