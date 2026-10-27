#!/usr/bin/env python3
"""Source integrity and archive extraction contracts for the packaging pipeline."""
import hashlib
import importlib.util
import io
import os
from pathlib import Path
import tarfile
import tempfile
import unittest
import shutil
import subprocess
import sys
from types import SimpleNamespace
from unittest.mock import patch
import zipfile

from audio_common import Build, digest, download, extract_tar
import bundle_runtime
from bundle_runtime import audit, collect


class Sources(unittest.TestCase):
    def test_download_checks_existing_cache_too(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / 'source'
            source.write_bytes(b'original source')
            asset = {'filename': 'cached.tar', 'url': source.as_uri(),
                     'sha256': hashlib.sha256(source.read_bytes()).hexdigest()}
            cache = root / 'cache'
            result = download(asset, cache)
            self.assertEqual(result.read_bytes(), source.read_bytes())
            result.write_bytes(b'corrupted cache')
            with self.assertRaisesRegex(ValueError, 'SHA-256 mismatch'):
                download(asset, cache)

    def test_source_update_replaces_previously_extracted_contents(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            assets = []
            for version in (1, 2):
                source = root / f'library-{version}.tar.gz'
                with tarfile.open(source, 'w:gz') as archive:
                    contents = f'version {version}'.encode()
                    item = tarfile.TarInfo('library/README')
                    item.size = len(contents)
                    archive.addfile(item, io.BytesIO(contents))
                assets.append({'filename': source.name, 'url': source.as_uri(),
                               'sha256': digest(source)})
            pins = {'sources': {'library': assets[0]}}
            build = Build(root / 'work', 1)
            with patch('audio_common.PINS', pins):
                first = build.source('library')
                self.assertEqual((first / 'README').read_text(), 'version 1')
                (first / 'local-build-artifact').write_text('keep while source is unchanged')
                self.assertEqual(build.source('library'), first)
                self.assertTrue((first / 'local-build-artifact').exists())
                pins['sources']['library'] = assets[1]
                updated = build.source('library')
                self.assertEqual((updated / 'README').read_text(), 'version 2')
                self.assertFalse((updated / 'local-build-artifact').exists())
                # Old markers recorded only the root path, without a source fingerprint.
                marker = build.sources / 'library/.extracted'
                marker.write_text(str(updated.relative_to(marker.parent)))
                (updated / 'README').write_text('legacy extraction')
                rebuilt = build.source('library')
                self.assertEqual((rebuilt / 'README').read_text(), 'version 2')

    def test_extract_rejects_traversal_and_escape_links(self):
        for name, link in [('../escape', None), ('/escape', None), ('safe/link', '../../escape')]:
            with self.subTest(name=name, link=link), tempfile.TemporaryDirectory() as directory:
                stream = io.BytesIO()
                with tarfile.open(fileobj=stream, mode='w') as archive:
                    item = tarfile.TarInfo(name)
                    if link:
                        item.type = tarfile.SYMTYPE
                        item.linkname = link
                    archive.addfile(item)
                stream.seek(0)
                with self.assertRaises(ValueError):
                    extract_tar(stream, Path(directory) / 'unpacked')

    def test_extract_preserves_relative_sources(self):
        with tempfile.TemporaryDirectory() as directory:
            stream = io.BytesIO()
            with tarfile.open(fileobj=stream, mode='w') as archive:
                item = tarfile.TarInfo('source/hello.c')
                item.size = 5
                archive.addfile(item, io.BytesIO(b'hello'))
            stream.seek(0)
            extract_tar(stream, Path(directory))
            self.assertEqual((Path(directory) / 'source/hello.c').read_bytes(), b'hello')

    def test_header_package_does_not_extract_binary_symlinks(self):
        with tempfile.TemporaryDirectory() as directory:
            stream = io.BytesIO()
            with tarfile.open(fileobj=stream, mode='w') as archive:
                header = tarfile.TarInfo('./usr/include/header.h')
                archive.addfile(header)
                link = tarfile.TarInfo('./usr/lib/libunused.so')
                link.type = tarfile.SYMTYPE
                link.linkname = 'libunused.so.1'
                archive.addfile(link)
            stream.seek(0)
            root = Path(directory)
            extract_tar(stream, root, ('usr/include/',))
            self.assertTrue((root / 'usr/include/header.h').is_file())
            self.assertFalse((root / 'usr/lib').exists())


@unittest.skipUnless(sys.platform == 'linux' and shutil.which('cc') and shutil.which('patchelf'),
                     'ELF runtime collection requires Linux cc and patchelf')
class RuntimeBundle(unittest.TestCase):
    def test_transitive_libraries_survive_removal_of_sdk(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            prefix = root / 'sdk'
            lib = prefix / 'lib'
            lib.mkdir(parents=True)
            (root / 'child.c').write_text('int child(void) { return 42; }')
            (root / 'parent.c').write_text('int child(void); int parent(void) { return child(); }')
            (root / 'main.c').write_text('int parent(void); int main(void) { return parent() != 42; }')
            def cc(*args):
                subprocess.run(['cc', *map(str, args)], check=True)
            cc('-shared', '-fPIC', root / 'child.c', '-o', lib / 'libchild.so')
            cc('-shared', '-fPIC', root / 'parent.c', '-L' + str(lib), '-lchild',
               '-Wl,-rpath,' + str(lib), '-o', lib / 'libparent.so')
            stage = root / 'distribution'
            bindir = stage / 'phaselimiter/bin'
            bindir.mkdir(parents=True)
            executable = bindir / 'probe'
            cc(root / 'main.c', '-L' + str(lib), '-lparent', '-Wl,-rpath,' + str(lib), '-o', executable)
            shutil.copy2(executable, stage / 'ffmpeg')
            collect(stage, prefix)
            prefix.rename(root / 'hidden-sdk')
            audit(stage)
            subprocess.run([str(executable)], check=True)
            (stage / 'phaselimiter/lib/libchild.so').unlink()
            with self.assertRaisesRegex(RuntimeError, 'Unresolved dependency'):
                audit(stage)


class WindowsArchive(unittest.TestCase):
    def test_bin_only_dll_layout_survives_zip_extraction(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            archive = root / 'portable.zip'
            imports = {
                'phase_limiter.exe': ['KERNEL32.dll', 'tbb12.dll'],
                'tbb12.dll': ['KERNEL32.dll', 'libomp.dll'],
                'libomp.dll': ['KERNEL32.dll'],
                'ffmpeg.exe': ['msvcrt.dll'],
            }
            with zipfile.ZipFile(archive, 'w') as package:
                for name in imports:
                    relative = name if name == 'ffmpeg.exe' else 'phaselimiter/bin/' + name
                    package.writestr('Phaselimiter/' + relative, b'PE fixture')
            with zipfile.ZipFile(archive) as package:
                package.extractall(root / 'extracted')
            stage = root / 'extracted/Phaselimiter'
            self.assertFalse((stage / 'phaselimiter/lib').exists())
            def inspect(binary):
                return {'imports': imports[binary.name], 'delay_imports': []}
            # Test Windows layout/import validation on every host; PE parsing
            # itself is exercised by the native extracted-distribution test.
            with patch.object(bundle_runtime, 'os', SimpleNamespace(name='nt')), \
                    patch.object(bundle_runtime, 'pe', side_effect=inspect):
                audit(stage)
                (stage / 'phaselimiter/bin/libomp.dll').unlink()
                with self.assertRaisesRegex(RuntimeError, 'Unresolved DLL:.*libomp.dll'):
                    audit(stage)
                shutil.rmtree(stage / 'phaselimiter/bin')
                with self.assertRaises(FileNotFoundError):
                    audit(stage)


class CorrespondingSources(unittest.TestCase):
    def test_binary_cache_hit_still_downloads_all_corresponding_sources(self):
        spec = importlib.util.spec_from_file_location('distribution', Path(__file__).with_name('package-distribution.py'))
        distribution = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(distribution)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / 'library-source.tar.gz'
            with tarfile.open(source, 'w:gz') as archive:
                item = tarfile.TarInfo('library/README')
                item.size = 6
                archive.addfile(item, io.BytesIO(b'source'))
            backend = root / 'backend'
            subprocess.run(['git', 'init', '-q', str(backend)], check=True)
            (backend / 'README.md').write_text('backend source')
            subprocess.run(['git', '-C', str(backend), 'add', 'README.md'], check=True)
            gui = root / 'gui'
            for name in ('src', 'script', 'packaging', 'assets', '.cargo', '.github'):
                (gui / name).mkdir(parents=True)
            (gui / 'assets/icon.png').write_bytes(b'icon fixture')
            (gui / 'build/icons').mkdir(parents=True)
            (gui / 'build/icons/phaselimiter.ico').write_bytes(b'generated icon')
            for name in ('Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', 'build.rs', 'LICENSE', 'README.md'):
                (gui / name).write_text('source snapshot')
            pins = {'backend': {'revision': 'a' * 40}, 'sources': {'library': {
                'filename': source.name, 'url': source.as_uri(), 'sha256': digest(source)}}}
            build = SimpleNamespace(downloads=root / 'empty-download-cache', backend=backend)
            output = root / 'corresponding-sources.tar.gz'
            with patch.object(distribution, 'PINS', pins), patch.object(distribution, 'ROOT', gui):
                distribution.source_archive(build, output)
            with tarfile.open(output) as archive:
                self.assertEqual(archive.extractfile('downloads/' + source.name).read(), source.read_bytes())
                self.assertEqual(archive.extractfile('backend/SOURCE-REVISION').read(), b'a' * 40 + b'\n')
                self.assertEqual(archive.extractfile('gui/build.rs').read(), b'source snapshot')
                self.assertEqual(archive.extractfile('gui/assets/icon.png').read(), b'icon fixture')
                self.assertNotIn('gui/build/icons/phaselimiter.ico', archive.getnames())


class MacMetadata(unittest.TestCase):
    def test_bundle_uses_cargo_version_and_macos_metadata(self):
        import plistlib
        import tomllib
        import package_common
        with tempfile.TemporaryDirectory() as directory:
            app = Path(directory) / 'App.app'
            (app / 'Contents').mkdir(parents=True)
            package_common.write_macos_plist(app)
            with (package_common.ROOT / 'Cargo.toml').open('rb') as source:
                package = tomllib.load(source)['package']
            with (app / 'Contents/Info.plist').open('rb') as source:
                info = plistlib.load(source)
            self.assertEqual(info['CFBundleExecutable'], package['name'])
            self.assertEqual(info['CFBundleShortVersionString'], package['version'])
            self.assertEqual(info['CFBundleVersion'], package['metadata']['macos']['bundle-version'])
            self.assertEqual(info['LSMinimumSystemVersion'], package['metadata']['macos']['minimum-system-version'])
            icon = app / 'Contents/Resources' / info['CFBundleIconFile']
            self.assertEqual(icon.read_bytes(), (package_common.ROOT / 'build/icons/phaselimiter.icns').read_bytes())
            self.assertTrue(icon.read_bytes().startswith(b'icns'))


class LinuxDesktop(unittest.TestCase):
    def setUp(self):
        spec = importlib.util.spec_from_file_location('install_desktop', Path(__file__).with_name('install-desktop.py'))
        self.installer = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(self.installer)

    def test_exec_argument_quotes_linux_paths_on_every_host(self):
        # No filesystem access: Linux filenames may contain characters that
        # Windows cannot create, but quoting can still be checked on every OS.
        path = '/tmp/日本語 audio %f $HOME `test` "quoted"/phaselimiter-gui'
        self.assertEqual(
            self.installer.exec_argument(path),
            r'"/tmp/日本語 audio %%f \\$HOME \\`test\\` \\"quoted\\"/phaselimiter-gui"',
        )

    @unittest.skipUnless(sys.platform == 'linux', 'Desktop registration requires Linux paths')
    def test_registration_uses_extracted_location_and_updates_after_move(self):
        import package_common
        installer = self.installer
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            bundle = root / '日本語 audio %f $HOME `test` "quoted"'
            bundle.mkdir()
            (bundle / 'phaselimiter-gui').touch()
            (bundle / 'phaselimiter.png').write_bytes(b'icon fixture')
            shutil.copy2(package_common.ROOT / 'packaging' / (installer.APP_ID + '.desktop'), bundle)
            data = root / 'user data'
            entry = installer.install(bundle, data)
            content = entry.read_text()
            self.assertIn('%%f', content)
            self.assertIn(r'\\$HOME', content)
            self.assertIn(r'\\"quoted\\"', content)
            self.assertIn('Icon=' + str(data / 'icons/hicolor/512x512/apps' / (installer.APP_ID + '.png')), content)
            if shutil.which('desktop-file-validate'):
                subprocess.run(['desktop-file-validate', str(entry)], check=True)
            moved = root / 'moved'
            bundle.rename(moved)
            self.assertEqual(installer.install(moved, data), entry)
            self.assertIn(f'Exec="{moved}/phaselimiter-gui"', entry.read_text())
            self.assertNotIn('%%f', entry.read_text())


class CargoOutputDirectory(unittest.TestCase):
    def test_packaging_follows_cargo_configuration_and_environment_without_building(self):
        import package_common
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            project = root / 'gui'
            (project / '.cargo').mkdir(parents=True)
            (project / 'src').mkdir()
            (project / 'src/main.rs').write_text('fn main() {}')
            (project / 'Cargo.toml').write_text(
                '[package]\nname = "output-directory-test"\nversion = "0.1.0"\nedition = "2021"\n')
            subprocess.run(['cargo', 'generate-lockfile', '--offline'], cwd=project, check=True)
            configured = root / 'configured output'
            (project / '.cargo/config.toml').write_text(
                '[build]\ntarget-dir = ' + repr(str(configured)) + '\n')
            environment = {key: value for key, value in os.environ.items()
                           if key not in ('CARGO_TARGET_DIR', 'CARGO_BUILD_TARGET_DIR')}
            for override, expected in [(None, configured),
                                       (str(root / 'external output'), root / 'external output'),
                                       ('relative output', project / 'relative output')]:
                with self.subTest(override=override), patch.dict(os.environ, environment, clear=True), \
                        patch.object(package_common, 'ROOT', project):
                    if override is not None:
                        os.environ['CARGO_TARGET_DIR'] = override
                    # Cargo may resolve symlinks in the working directory (for example,
                    # macOS /var -> /private/var) when expanding a relative target path.
                    self.assertEqual(package_common.cargo_target_directory().resolve(),
                                     expected.resolve())
                    self.assertFalse(expected.exists())
                    self.assertFalse((project / 'target').exists())


if __name__ == '__main__':
    unittest.main()
