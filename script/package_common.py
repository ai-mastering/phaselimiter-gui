"""Shared metadata for standalone and complete application bundles."""
import json
import plistlib
import shutil
import subprocess
import tomllib
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def cargo_target_directory():
    """Use Cargo's configured output directory, including environment overrides."""
    metadata = subprocess.check_output(
        ['cargo', 'metadata', '--locked', '--no-deps', '--format-version=1'],
        cwd=ROOT, text=True,
    )
    return Path(json.loads(metadata)['target_directory'])


def write_macos_plist(app):
    resources = app / 'Contents/Resources'
    resources.mkdir(parents=True, exist_ok=True)
    shutil.copy2(ROOT / 'build/icons/phaselimiter.icns', resources / 'phaselimiter.icns')
    with (ROOT / 'Cargo.toml').open('rb') as source:
        package = tomllib.load(source)['package']
    with (app / 'Contents/Info.plist').open('wb') as output:
        plistlib.dump({
            'CFBundleExecutable': package['name'],
            'CFBundleIdentifier': 'com.aimastering.phaselimiter-gui',
            'CFBundleName': 'Phaselimiter',
            'CFBundleIconFile': 'phaselimiter.icns',
            'CFBundlePackageType': 'APPL',
            'CFBundleShortVersionString': package['version'],
            'CFBundleVersion': package['metadata']['macos']['bundle-version'],
            'LSMinimumSystemVersion': package['metadata']['macos']['minimum-system-version'],
            'NSHighResolutionCapable': True,
        }, output)


def zip_directory(archive, directory):
    with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as output:
        for path in sorted(directory.rglob("*")):
            if path.is_file():
                output.write(path, path.relative_to(directory.parent))
