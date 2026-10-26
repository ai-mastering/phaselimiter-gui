#!/usr/bin/env python3
"""Package a previously built GUI; backend/resources can be bundled separately."""
import argparse
import pathlib
import shutil
import subprocess
import sys

from package_common import cargo_target_directory, write_macos_plist, zip_directory

root = pathlib.Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("target", nargs="?", default="x86_64-apple-darwin")
parser.add_argument("--output", type=pathlib.Path)
parser.add_argument("--no-zip", action="store_true")
args = parser.parse_args()
target = args.target
binary = cargo_target_directory() / target / "release" / "phaselimiter-gui"
if not binary.is_file():
    raise SystemExit(f"Build the executable first: {binary}")
destination = args.output or root / "dist" / target
app = destination / "Phaselimiter.app"
macos = app / "Contents" / "MacOS"
resources = app / "Contents" / "Resources"
macos.mkdir(parents=True, exist_ok=True)
resources.mkdir(parents=True, exist_ok=True)
shutil.copy2(binary, macos / "phaselimiter-gui")
(macos / "phaselimiter-gui").chmod(0o755)
shutil.copy2(root / "LICENSE", resources / "LICENSE")
subprocess.run([sys.executable, str(root / "script/third-party-licenses.py"), target,
                str(resources / "THIRD_PARTY_LICENSES.txt")], cwd=root, check=True)
write_macos_plist(app)
print(app)
if not args.no_zip:
    archive = destination / "Phaselimiter-macos.zip"
    zip_directory(archive, app)
    print(archive)
