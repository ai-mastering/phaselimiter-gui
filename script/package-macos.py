#!/usr/bin/env python3
"""Package a previously built GUI; backend/resources can be bundled separately."""
import pathlib
import plistlib
import shutil
import subprocess
import sys
import zipfile

root = pathlib.Path(__file__).resolve().parent.parent
target = sys.argv[1] if len(sys.argv) > 1 else "x86_64-apple-darwin"
binary = root / "target" / target / "release" / "phaselimiter-gui"
if not binary.is_file():
    raise SystemExit(f"Build the executable first: {binary}")
destination = root / "dist" / target
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
with (app / "Contents" / "Info.plist").open("wb") as output:
    plistlib.dump({
        "CFBundleExecutable": "phaselimiter-gui",
        "CFBundleIdentifier": "com.aimastering.phaselimiter-gui",
        "CFBundleName": "Phaselimiter",
        "CFBundlePackageType": "APPL",
        "CFBundleShortVersionString": "0.2.0",
        "CFBundleVersion": "6",
        "LSMinimumSystemVersion": "14.0",
        "NSHighResolutionCapable": True,
    }, output)
archive = destination / "Phaselimiter-macos.zip"
with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as output:
    for path in sorted(app.rglob("*")):
        if path.is_file():
            output.write(path, path.relative_to(destination))
print(app)
print(archive)
