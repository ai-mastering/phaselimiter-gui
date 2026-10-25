#!/usr/bin/env python3
"""Test the extracted portable artifact, with SDK/runtime search paths removed."""
import argparse
import math
import os
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import tempfile
import wave
import zipfile

from bundle_runtime import audit
from audio_common import extract_tar


def invoke(command, env, **kwargs):
    print('+', subprocess.list2cmdline([str(p) for p in command]), flush=True)
    result = subprocess.run([str(p) for p in command], env=env, stderr=subprocess.PIPE,
                            timeout=300, **kwargs)
    if result.returncode:
        raise RuntimeError(result.stderr.decode('utf-8', errors='replace'))
    return result


def check_audio(ffmpeg, path, env):
    samples = invoke([ffmpeg, '-v', 'error', '-i', path, '-ac', '2', '-ar', '44100',
                      '-c:a', 'pcm_f32le', '-f', 'f32le', '-'], env, stdout=subprocess.PIPE).stdout
    values = struct.unpack('<' + 'f' * (len(samples) // 4), samples)
    assert 0.8 * 44100 * 2 <= len(values) <= 1.3 * 44100 * 2, (path, len(values))
    assert all(math.isfinite(v) for v in values), path
    assert max(abs(v) for v in values) > 0.001, path


def test(archive, backend):
    with tempfile.TemporaryDirectory(prefix='phaselimiter-package-') as temporary:
        root = Path(temporary) / '日本語 配布 🎵'
        root.mkdir()
        if archive.suffix == '.zip':
            with zipfile.ZipFile(archive) as package:
                package.extractall(root)
                # zipfile does not restore Unix executable permissions.
                if os.name != 'nt':
                    for item in package.infolist():
                        (root / item.filename).chmod((item.external_attr >> 16) & 0o777)
        else:
            extract_tar(archive, root)
        payload, = root.iterdir()
        stage = payload / 'Contents/Resources' if payload.suffix == '.app' else payload
        suffix = '.exe' if os.name == 'nt' else ''
        ffmpeg = stage / ('ffmpeg' + suffix)
        tool = stage / 'phaselimiter/bin' / ('phase_limiter' + suffix)
        scratch = root / '一時 領域 🎵'
        scratch.mkdir()
        env = os.environ.copy()
        for key in list(env):
            if key.startswith(('DYLD_', 'LD_', 'CONDA_')) or key in ('LIBRARY_PATH', 'LIB', 'INCLUDE'):
                env.pop(key)
        system = Path(os.environ.get('SystemRoot', 'C:/Windows'))
        env['PATH'] = os.pathsep.join(map(str, [system / 'System32', system])) if os.name == 'nt' else '/usr/bin:/bin'
        env.update(TEMP=str(scratch), TMP=str(scratch), TMPDIR=str(scratch))
        license_text = invoke([ffmpeg, '-L'], env, stdout=subprocess.PIPE).stdout.decode('utf-8')
        assert 'GNU Lesser General Public License' in ' '.join(license_text.split()), license_text
        # Dependency auditing uses system inspection tools and has no build-prefix lookup.
        audit(stage)
        source = root / '入力 音楽 🎵.wav'
        with wave.open(str(source), 'wb') as audio:
            audio.setparams((2, 2, 44100, 0, 'NONE', 'not compressed'))
            audio.writeframes(b''.join(struct.pack('<hh', sample, sample)
                              for i in range(44100)
                              for sample in [int(9000 * math.sin(2 * math.pi * 440 * i / 44100))]))
        for codec, extension in [('pcm_s16le', 'wav'), ('pcm_s24le', 'wav'), ('pcm_f32le', 'wav'),
                                 ('flac', 'flac'), ('alac', 'm4a'), ('libmp3lame', 'mp3'), ('aac', 'm4a')]:
            output = root / f'変換 {codec}.{extension}'
            invoke([ffmpeg, '-v', 'error', '-y', '-i', source, '-ar', '48000', '-c:a', codec, output], env)
            check_audio(ffmpeg, output, env)
        image = root / 'スペクトラム.png'
        invoke([ffmpeg, '-v', 'error', '-y', '-i', source, '-lavfi',
                'showspectrumpic=s=320x180', '-frames:v', '1', image], env)
        assert image.read_bytes().startswith(b'\x89PNG\r\n\x1a\n')
        invoke([ffmpeg, '-v', 'error', '-i', image, '-f', 'null', '-'], env)
        cache = stage / 'phaselimiter/resource/sound_quality2_cache'
        reference = stage / 'phaselimiter/resource/mastering_reference.json'
        for optimizer in ('de_prmm', 'cmaes'):
            output = root / f'出力 {optimizer}.wav'
            invoke([tool, f'--input={source}', f'--output={output}', '--output_format=wav',
                    f'--ffmpeg={ffmpeg}', f'--tmp={scratch}', '--mastering',
                    '--mastering_mode=mastering5', '--mastering_reverb=false',
                    f'--sound_quality2_cache={cache}', f'--mastering_reference_file={reference}',
                    f'--mastering5_optimization_algorithm={optimizer}', '--mastering5_optimization_max_eval_count=2',
                    '--reference_mode=peak', '--reference=-6', '--max_iter1=2', '--max_iter2=8',
                    '--worker_count=1'], env)
            check_audio(ffmpeg, output, env)
        if os.name == 'nt':
            invoke([sys.executable, backend / 'script/test_windows_utf8.py',
                    '--stage-dir', stage / 'phaselimiter', '--ffmpeg', ffmpeg], env)
        if sys.platform == 'darwin':
            invoke(['/usr/bin/codesign', '--verify', '--deep', '--strict', payload], env)
        print('Extracted artifact: codecs, resampling, PNG, DE/CMA-ES mastering and Unicode paths passed')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--archive', required=True, type=Path)
    parser.add_argument('--backend', default='build/audio/backend', type=Path)
    args = parser.parse_args()
    test(args.archive.resolve(), args.backend.resolve())
