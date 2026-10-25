#!/usr/bin/env python3
"""Stage the GUI, built audio tools, resources, licenses and portable archives."""
import argparse
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import zipfile

from audio_common import Build, PINS, ROOT, digest, download, run
from bundle_runtime import TOOLS, audit, collect
from package_common import write_macos_plist


def notices(build, stage):
    licenses = stage / "phaselimiter/licenses"
    shutil.copytree(build.backend / "packaging/licenses", licenses, dirs_exist_ok=True)
    for key in ["ffmpeg", "lame", "zlib", "png", "jpeg", "sndfile", "boost", "tbb", "openmp"]:
        source = build.source(key)
        destination = licenses / key
        destination.mkdir(exist_ok=True)
        for path in source.iterdir():
            if path.is_file() and (path.name.upper().startswith(("LICENSE", "COPYING", "NOTICE", "THIRD-PARTY"))
                                   or path.name in ("README.ijg", "README")):
                shutil.copy2(path, destination / path.name)
    # Include the actual proprietary SDK license alongside the upstream notice.
    for path in sorted((build.work / "sdk").rglob("*")):
        if path.is_file() and path.name.lower().startswith(("license", "eula")):
            relative = path.relative_to(build.work / "sdk")
            destination = licenses / "intel-ipp" / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(path, destination)
    if sys.platform == "linux":
        for package in ["libstdc++6", "libgcc-s1"]:
            source = Path("/usr/share/doc") / package / "copyright"
            if source.is_file():
                shutil.copy2(source, licenses / (package + "-copyright"))
        shutil.copy2("/usr/share/common-licenses/GPL-3", licenses / "GCC-GPL-3.txt")


def source_archive(build, destination):
    """One Linux job publishes sources/build scripts for all four native builds."""
    with tarfile.open(destination, "w:gz") as archive:
        for asset in PINS["sources"].values():
            # A binary-cache hit skips the dependency build that normally
            # downloads these archives. Still publish all corresponding sources.
            path = download(asset, build.downloads)
            archive.add(path, arcname="downloads/" + path.name)
        files = subprocess.check_output(["git", "ls-files", "--recurse-submodules", "-z"],
                                        cwd=build.backend).decode().split("\0")
        for name in files:
            if name:
                archive.add(build.backend / name, arcname="backend/" + name, recursive=False)
        revision = (PINS["backend"]["revision"] + "\n").encode()
        marker = tarfile.TarInfo("backend/SOURCE-REVISION")
        marker.size = len(revision)
        archive.addfile(marker, io.BytesIO(revision))
        for name in ["src", "script", "packaging", ".cargo", ".github", "Cargo.toml", "Cargo.lock",
                     "rust-toolchain.toml", "build.rs", "LICENSE", "README.md"]:
            archive.add(ROOT / name, arcname="gui/" + name,
                        filter=lambda item: None if "__pycache__" in item.name else item)


def package(build, target, sources=False):
    stage = build.work / "stage"
    if stage.exists():
        shutil.rmtree(stage)
    backend_stage = stage / "phaselimiter"
    bindir = backend_stage / "bin"
    bindir.mkdir(parents=True)
    suffix = ".exe" if build.is_windows else ""
    for tool in TOOLS:
        shutil.copy2(build.work / "build/backend/bin" / (tool + suffix), bindir / (tool + suffix))
    for name in ["LICENSE", "README.md"]:
        shutil.copy2(build.backend / name, backend_stage / name)
    shutil.copytree(build.backend / "resource", backend_stage / "resource",
                    ignore=shutil.ignore_patterns(".DS_Store", "sound_quality2_cache"))
    shutil.copy2(build.work / "data/sound_quality2_cache", backend_stage / "resource/sound_quality2_cache")
    shutil.copy2(build.work / "ffmpeg-prefix/bin" / ("ffmpeg" + suffix), stage / ("ffmpeg" + suffix))
    notices(build, stage)
    collect(stage, build.prefix)
    audit(stage)
    metadata = {
        "target": target, "backend": PINS["backend"], "sources": PINS["sources"],
        "backend_dependency_manifest": json.loads((build.backend / "environment/dependencies.json").read_text()),
        "dataset": json.loads((build.backend / "environment/dataset.json").read_text()),
        "cpu": "ARM64/NEON/Accelerate" if target.startswith("aarch64") else "x86-64/AVX/IPP",
        "minimum_os": "macOS 14" if build.is_mac else "Windows 10 1903" if build.is_windows else
                      "Linux x86-64 (glibc floor recorded in runtime-versions.txt)",
        "source_archive": "Phaselimiter-sources.tar.gz",
        "toolchain": {key: os.environ.get(key, "") for key in ["ImageVersion", "VCToolsVersion", "MACOSX_DEPLOYMENT_TARGET"]},
    }
    metadata["binary_sha256"] = {str(p.relative_to(stage)): digest(p)
                                 for p in [*bindir.iterdir(), stage / ("ffmpeg" + suffix)]}
    (stage / "BUILD-INFO.json").write_text(json.dumps(metadata, indent=2) + "\n")
    (stage / "ffmpeg-build.txt").write_text(subprocess.check_output(
        [str(stage / ("ffmpeg" + suffix)), "-version"], text=True))
    if sys.platform == "linux":
        with (stage / "runtime-versions.txt").open("w") as output:
            for path in [*bindir.iterdir(), *(backend_stage / "lib").iterdir(), stage / "ffmpeg"]:
                output.write(f"\n{path.relative_to(stage)}\n")
                output.flush()
                run(["readelf", "--version-info", path], stdout=output)
    # Existing standalone GUI packagers remain available for development builds.
    gui = ROOT / "target" / target / "release" / ("phaselimiter-gui" + suffix)
    destination = ROOT / "dist" / target
    destination.mkdir(parents=True, exist_ok=True)
    payload = destination / ("Phaselimiter.app" if build.is_mac else "Phaselimiter")
    shutil.rmtree(payload, ignore_errors=True)
    resource = payload / "Contents/Resources" if build.is_mac else payload
    shutil.copytree(stage, resource)
    executable_dir = payload / "Contents/MacOS" if build.is_mac else payload
    executable_dir.mkdir(parents=True, exist_ok=True)
    shutil.copy2(gui, executable_dir / gui.name)
    shutil.copy2(ROOT / "LICENSE", resource / "LICENSE")
    run([sys.executable, ROOT / "script/third-party-licenses.py", target,
         resource / "THIRD_PARTY_LICENSES.txt"], cwd=ROOT)
    if build.is_mac:
        write_macos_plist(payload)
        run(["codesign", "--force", "--sign", "-", payload])
        run(["codesign", "--verify", "--deep", "--strict", payload])
    elif not build.is_windows:
        # The GUI's libgcc dependency also resolves to the bundled copy.
        run(["patchelf", "--set-rpath", "$ORIGIN/phaselimiter/lib", executable_dir / gui.name])
        with (resource / "runtime-versions.txt").open("a") as output:
            output.write("\nphaselimiter-gui\n")
            output.flush()
            run(["readelf", "--version-info", executable_dir / gui.name], stdout=output)
    archive = destination / ("Phaselimiter-" + target + (".zip" if build.is_mac or build.is_windows else ".tar.gz"))
    if archive.suffix == ".zip":
        with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as output:
            for path in sorted(payload.rglob("*")):
                if path.is_file():
                    output.write(path, path.relative_to(destination))
    else:
        with tarfile.open(archive, "w:gz") as output:
            output.add(payload, arcname=payload.name)
    if sources:
        source_archive(build, destination / "Phaselimiter-sources.tar.gz")
    print(archive)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", required=True)
    parser.add_argument("--work", default="build/audio")
    parser.add_argument("--sources", action="store_true")
    args = parser.parse_args()
    package(Build(args.work, 2), args.target, args.sources)


if __name__ == "__main__":
    main()
