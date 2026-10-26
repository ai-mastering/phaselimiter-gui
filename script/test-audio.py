#!/usr/bin/env python3
"""Run upstream tests and generate a native cache, in separately observable phases."""
import argparse
import os
import shutil
import subprocess
import sys

from audio_common import Build, run


def ctest(build, env, selection, report):
    run(['ctest', '--test-dir', build.work / 'build/backend', '--verbose',
         '--parallel', '1', '--timeout', '900', '--no-tests=error', *selection,
         '--output-junit', build.work / 'build/backend/Testing' / report], env=env)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--work', default='build/audio')
    parser.add_argument('--phase', choices=['all', 'tests', 'cli', 'unit', 'cache'], default='all')
    args = parser.parse_args()
    build = Build(args.work, 2)
    env = os.environ.copy()
    env['PATH'] = os.pathsep.join([str(build.prefix / 'bin'), str(build.work / 'ffmpeg-prefix/bin'), env['PATH']])
    if sys.platform == 'linux':
        env['LD_LIBRARY_PATH'] = str(build.prefix / 'lib')
    if args.phase in ('all', 'tests', 'cli'):
        host_ffmpeg = shutil.which('ffmpeg')
        if not host_ffmpeg or 'libx264' not in subprocess.check_output(
                [host_ffmpeg, '-hide_banner', '-encoders'], text=True, stderr=subprocess.STDOUT):
            raise RuntimeError('The upstream visualizer test requires a test-only FFmpeg with libx264 on PATH')
        ctest(build, env, ['-E', '^(auxiliary-cli|unit)$'], 'results.xml')
        # The optional PPM-to-H.264 pipe requires the host test tool. All other
        # tests use our LGPL audio FFmpeg; libx264 is not distributed.
        auxiliary_env = env.copy()
        auxiliary_env['PATH'] = os.pathsep.join([str(build.prefix / 'bin'), os.environ['PATH']])
        ctest(build, auxiliary_env, ['-R', '^auxiliary-cli$'], 'auxiliary-results.xml')
    if args.phase in ('all', 'tests', 'unit'):
        ctest(build, env, ['-R', '^unit$'], 'unit-results.xml')
    if args.phase in ('all', 'cache'):
        dataset = build.work / 'dataset'
        run([sys.executable, build.backend / 'script/prepare_dataset.py', '--destination', dataset], timeout=600)
        data = build.work / 'data'
        data.mkdir(exist_ok=True)
        analyzer = build.work / 'build/backend/bin' / ('audio_analyzer.exe' if build.is_windows else 'audio_analyzer')
        run([analyzer, '--mode=sound_quality2_preparation', f'--worker_count={build.jobs}',
             f'--analysis_data_dir={dataset / "analysis_shortpath"}',
             f'--sound_quality2_cache={data / "sound_quality2_cache"}'], env=env, timeout=900)


if __name__ == '__main__':
    main()
