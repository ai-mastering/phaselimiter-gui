#!/usr/bin/env python3
"""Package the standalone GUI and license notices, without the DSP backend."""
import pathlib
import shutil
import subprocess
import sys
import zipfile

root = pathlib.Path(__file__).resolve().parent.parent
target = "x86_64-pc-windows-msvc"
binary = root / "target" / target / "release" / "phaselimiter-gui.exe"
if not binary.is_file():
    raise SystemExit(f"Build the executable first: {binary}")
destination = root / "dist" / target
destination.mkdir(parents=True, exist_ok=True)
shutil.copy2(binary, destination / binary.name)
shutil.copy2(root / "LICENSE", destination / "LICENSE")
subprocess.run([sys.executable, str(root / "script/third-party-licenses.py"), target,
                str(destination / "THIRD_PARTY_LICENSES.txt")], cwd=root, check=True)
with zipfile.ZipFile(destination / "Phaselimiter-windows-x86_64.zip", "w", compression=zipfile.ZIP_DEFLATED) as output:
    for name in [binary.name, "LICENSE", "THIRD_PARTY_LICENSES.txt"]:
        output.write(destination / name, name)
print(destination / "Phaselimiter-windows-x86_64.zip")
