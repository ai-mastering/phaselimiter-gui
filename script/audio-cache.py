#!/usr/bin/env python3
"""Fingerprint native build caches independently of the backend revision."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess

from audio_common import PINS, ROOT, digest

SOURCES = {
    'ffmpeg': ('ffmpeg', 'lame', 'zlib'),
    'dependencies': ('boost', 'sndfile', 'png', 'jpeg', 'tbb', 'openmp', 'llvm-cmake', 'zlib'),
}
RECIPES = {
    'ffmpeg': ('script/build-ffmpeg.sh', 'script/audio_common.py'),
    'dependencies': ('script/build-audio.py', 'script/audio_common.py'),
}


def key(name, target, payload):
    encoded = json.dumps(payload, sort_keys=True, separators=(',', ':')).encode()
    return f'audio-{name}-v1-{target}-' + hashlib.sha256(encoded).hexdigest()


def build_key(component, target, pins, recipes, toolchain):
    return key(component, target, {
        'sources': {name: pins['sources'][name] for name in SOURCES[component]},
        'recipes': {name: recipes[name] for name in RECIPES[component]},
        'toolchain': toolchain,
    })


def version(*command):
    return subprocess.check_output(command, text=True, stderr=subprocess.STDOUT).strip()


def toolchain(component, work):
    result = {
        'os': platform.system(), 'arch': platform.machine(), 'prefix': str(work.resolve()),
        'flags': {name: os.environ.get(name, '') for name in
                  ('CC', 'CXX', 'CFLAGS', 'CPPFLAGS', 'CXXFLAGS', 'LDFLAGS', 'CL', '_CL_',
                   'LINK', 'MACOSX_DEPLOYMENT_TARGET')},
    }
    if os.name == 'nt' and component == 'dependencies':
        compiler = shutil.which('cl.exe')
        if compiler is None:
            raise RuntimeError('Run from the native MSVC developer environment')
        result['cl_sha256'] = digest(compiler)
        result['msvc_sdk'] = {name: os.environ.get(name, '') for name in
                              ('VCToolsVersion', 'WindowsSDKVersion', 'UCRTVersion')}
    else:
        compiler = '/usr/bin/clang' if platform.system() == 'Darwin' else 'gcc'
        result['cc'] = version(compiler, '--version')
        if component == 'dependencies':
            result['cxx'] = version('g++', '--version') if platform.system() != 'Darwin' else version('/usr/bin/clang++', '--version')
    if platform.system() == 'Darwin':
        result['sdk'] = version('xcrun', '--sdk', 'macosx', '--show-sdk-build-version')
        result['xcode'] = version('xcodebuild', '-version')
    elif platform.system() == 'Linux':
        result['distribution'] = Path('/etc/os-release').read_text()
        result['libc'] = platform.libc_ver()
        result['linker'] = version('ld', '--version')
    if component == 'ffmpeg':
        result['nasm'] = version('nasm', '--version')
        result['make'] = version('make', '--version')
        result['pkg_config'] = version('pkg-config', '--version')
        if os.name == 'nt':
            result['linker'] = version('ld', '--version')
            result['ar'] = version('ar', '--version')
    else:
        result['cmake'] = version('cmake', '--version')
        result['ninja'] = version('ninja', '--version')
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('component', choices=SOURCES)
    parser.add_argument('--target', required=True)
    parser.add_argument('--work', default='build/audio', type=Path)
    args = parser.parse_args()
    tools = toolchain(args.component, args.work)
    recipes = {name: digest(ROOT / name) for name in RECIPES[args.component]}
    outputs = {'key': build_key(args.component, args.target, PINS, recipes, tools)}
    if args.component == 'dependencies':
        rust_files = [ROOT / 'Cargo.toml', ROOT / 'Cargo.lock', ROOT / 'rust-toolchain.toml',
                      *sorted((ROOT / '.cargo').rglob('*.toml'))]
        outputs['rust-key'] = key('rust', args.target, {
            'toolchain': tools, 'rustc': version('rustc', '-vV'),
            'files': {str(p.relative_to(ROOT)): digest(p) for p in rust_files},
            'flags': {name: os.environ.get(name, '') for name in
                      ('RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'CARGO_INCREMENTAL')},
        })
        backend = json.loads((args.work / 'backend/environment/dependencies.json').read_text())
        outputs['sources-key'] = key('sources', args.target, {'sources': PINS['sources'], 'backend_assets': backend})
        outputs['cargo-home'] = str(Path(os.environ.get('CARGO_HOME', Path.home() / '.cargo')).resolve())
    for name, value in outputs.items():
        print(f'{name}={value}')
    if os.environ.get('GITHUB_OUTPUT'):
        with Path(os.environ['GITHUB_OUTPUT']).open('a', encoding='utf-8') as output:
            for name, value in outputs.items():
                print(f'{name}={value}', file=output)


if __name__ == '__main__':
    main()
