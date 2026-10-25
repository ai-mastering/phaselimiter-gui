"""Shared metadata for standalone and complete application bundles."""
import plistlib
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def write_macos_plist(app):
    with (ROOT / 'Cargo.toml').open('rb') as source:
        package = tomllib.load(source)['package']
    with (app / 'Contents/Info.plist').open('wb') as output:
        plistlib.dump({
            'CFBundleExecutable': package['name'],
            'CFBundleIdentifier': 'com.aimastering.phaselimiter-gui',
            'CFBundleName': 'Phaselimiter',
            'CFBundlePackageType': 'APPL',
            'CFBundleShortVersionString': package['version'],
            'CFBundleVersion': package['metadata']['macos']['bundle-version'],
            'LSMinimumSystemVersion': package['metadata']['macos']['minimum-system-version'],
            'NSHighResolutionCapable': True,
        }, output)
