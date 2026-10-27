#!/usr/bin/env python3
"""Master the bundled sample and capture the real Linux GUI in a private Xvfb display."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
WIDTH, HEIGHT = 1600, 1000


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable', type=Path, help='Linux GUI executable (prefer a complete distribution)')
    parser.add_argument('--output', type=Path, default=ROOT / 'build/screenshots/workspace.png')
    parser.add_argument('--backend', type=Path, help='phase_limiter; analyzer and detector must be beside it')
    parser.add_argument('--ffmpeg', type=Path, help='override bundled FFmpeg')
    parser.add_argument('--cache', type=Path, help='override bundled sound_quality2_cache')
    parser.add_argument('--timeout', type=int, default=1800, help='processing timeout per master in seconds')
    parser.add_argument('--inside-xvfb', action='store_true', help=argparse.SUPPRESS)
    args = parser.parse_args()
    if sys.platform != 'linux':
        parser.error('Run this script on Linux, including the devcontainer.')
    if not args.executable.is_file():
        parser.error(f'Executable not found: {args.executable}')
    for name in ('backend', 'ffmpeg', 'cache'):
        value = getattr(args, name)
        if value is not None and not value.is_file():
            parser.error(f'{name} not found: {value}')
    if args.timeout <= 0:
        parser.error('--timeout must be positive')
    if not shutil.which('optipng'):
        parser.error('Install optipng; see README.md, Screenshots.')
    if not args.inside_xvfb:
        if not shutil.which('xvfb-run'):
            parser.error('Install xvfb and xauth; see README.md, Screenshots.')
        return subprocess.call(['xvfb-run', '-a', '-s', f'-screen 0 {WIDTH}x{HEIGHT}x24 -dpi 96',
                                sys.executable, str(Path(__file__).resolve()),
                                *sys.argv[1:], '--inside-xvfb'])

    from PIL import Image
    from Xlib import X, XK, display, protocol
    from Xlib.ext import xtest

    if not shutil.which('xclip'):
        parser.error('Install xclip; see README.md, Screenshots.')
    connection = display.Display()
    root = connection.screen().root
    name_atom = connection.intern_atom('_NET_WM_NAME')
    work = Path(tempfile.mkdtemp(prefix='phaselimiter-screenshot-'))
    library_file = work / 'phaselimiter-gui/library.json'
    env = dict(os.environ, XDG_DATA_HOME=str(work), XDG_CACHE_HOME=str(work / 'cache'),
               WINIT_X11_SCALE_FACTOR='1')
    for name in ('WAYLAND_DISPLAY', 'WAYLAND_SOCKET'):
        env.pop(name, None)
    app = None
    log = (work / 'application.log').open('w')
    success = False

    def wait_for(check, description, timeout=20):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if app.poll() is not None:
                raise RuntimeError(f'Application exited ({app.returncode}) while waiting for {description}')
            value = check()
            if value:
                return value
            time.sleep(0.2)
        raise RuntimeError(f'Timed out waiting for {description}')

    def find_window():
        for window in root.query_tree().children:
            try:
                value = window.get_full_property(name_atom, X.AnyPropertyType)
                if (value and bytes(value.value).decode('utf-8') == 'Phaselimiter'
                        and window.get_attributes().map_state == X.IsViewable):
                    return window
            except Exception:
                continue
        return None

    def launch():
        nonlocal app
        app = subprocess.Popen([str(args.executable.resolve())], env=env, stdout=log, stderr=log)
        window = wait_for(find_window, 'main window')
        window.configure(x=0, y=0, width=WIDTH, height=HEIGHT)
        window.set_input_focus(X.RevertToParent, X.CurrentTime)
        connection.sync()
        time.sleep(1)
        return window

    def close(window):
        window.send_event(protocol.event.ClientMessage(
            window=window, client_type=connection.intern_atom('WM_PROTOCOLS'),
            data=(32, [connection.intern_atom('WM_DELETE_WINDOW'), X.CurrentTime, 0, 0, 0])),
            event_mask=0)
        connection.flush()
        app.wait(timeout=15)
        if app.returncode:
            raise RuntimeError(f'Application exited with {app.returncode}')

    def key(name, down=True):
        code = connection.keysym_to_keycode(XK.string_to_keysym(name))
        xtest.fake_input(connection, X.KeyPress if down else X.KeyRelease, code)
        connection.sync()

    def press(name):
        key(name)
        key(name, False)
        time.sleep(0.15)

    def click(x, y):
        xtest.fake_input(connection, X.MotionNotify, x=x, y=y)
        xtest.fake_input(connection, X.ButtonPress, 1)
        xtest.fake_input(connection, X.ButtonRelease, 1)
        connection.sync()
        time.sleep(0.2)

    def toggle_path():
        # Click the menu so a focused text field cannot consume Alt mnemonics.
        click(25, 20)
        press('p')

    def read_library():
        try:
            return json.loads(library_file.read_text())
        except (OSError, ValueError):
            return None

    try:
        # Let the app create its own current defaults, then set only tool paths
        # and worker count while it is closed. No fabricated history or analysis.
        print('Initializing a fresh workspace…', flush=True)
        close(launch())
        library = json.loads(library_file.read_text())
        for name in ('backend', 'ffmpeg', 'cache'):
            value = getattr(args, name)
            if value:
                library['app_settings'][name] = str(value.resolve())
        library['draft']['workers'] = '2'
        library_file.write_text(json.dumps(library))
        window = launch()
        toggle_path()
        # The path field is immediately below the menu, at fixed scale/size.
        click(250, 55)
        subprocess.run(['xclip', '-selection', 'clipboard'],
                       input=str(ROOT / 'samples/screenshot/Goldberg-Variation-4.mp3'), text=True,
                       check=True, timeout=5)
        key('Control_L')
        press('v')
        key('Control_L', False)
        press('Return')
        wait_for(lambda: (read_library() or {}).get('jobs'), 'sample entering the queue')
        toggle_path()
        print('Mastering and analyzing the 20-second sample…', flush=True)
        last_status = None

        def completed():
            nonlocal last_status
            library = read_library()
            if not library or not library['jobs']:
                return False
            job = library['jobs'][-1]
            if job['status'] != last_status:
                last_status = job['status']
                print(f'  {last_status}', flush=True)
            if job['status'] in ('Failed', 'Cancelled', 'Interrupted'):
                raise RuntimeError(f"Mastering {job['status']}: {job.get('log', [])[-10:]}")
            for item in [library['tracks'][0], job]:
                if item.get('analysis_error'):
                    raise RuntimeError(f"Analysis failed: {item['analysis_error']}")
            return (job['status'] == 'Succeeded' and job.get('analysis')
                    and library['tracks'][0].get('analysis'))

        for count in range(1, 4):
            if count > 1:
                # Create variation repeats the original input with the unchanged
                # draft. Each click submits a real job, preserving earlier results.
                click(WIDTH - 170, HEIGHT - 53)
                wait_for(lambda: len((read_library() or {}).get('jobs', [])) == count,
                         f'master {count} entering the queue')
                last_status = None
            print(f'Master {count}/3', flush=True)
            wait_for(completed, f'master {count} and both analysis reports', args.timeout)
        library = read_library()
        if (len(library['jobs']) != 3
                or any(job['status'] != 'Succeeded' for job in library['jobs'])
                or any(job['config'] != library['jobs'][0]['config'] for job in library['jobs'])):
            raise RuntimeError('Expected three completed masters with identical settings')
        # History is newest first. Select the latest result for the screenshot.
        click(150, 177)
        press('Up')
        press('Up')
        wait_for(lambda: (read_library() or {}).get('selected')
                 == [library['jobs'][-1]['track_id'], library['jobs'][-1]['id']],
                 'latest result selection')
        # Allow plot geometry / playback preparation to finish after persisted reports.
        time.sleep(3)
        xtest.fake_input(connection, X.MotionNotify, x=WIDTH - 1, y=HEIGHT - 1)
        connection.sync()
        shot = window.get_image(0, 0, WIDTH, HEIGHT, X.ZPixmap, 0xffffffff)
        if shot.depth != 24 or len(shot.data) != WIDTH * HEIGHT * 4:
            raise RuntimeError('Expected Xvfb 24-bit BGRX pixels')
        image = Image.frombytes('RGB', (WIDTH, HEIGHT), shot.data, 'raw', 'BGRX')
        if all(low == high for low, high in image.getextrema()):
            raise RuntimeError('Captured an empty window')
        args.output.parent.mkdir(parents=True, exist_ok=True)
        image.save(args.output)
        original_size = args.output.stat().st_size
        subprocess.run(['optipng', '-quiet', '-o2', str(args.output)], check=True, timeout=120)
        with Image.open(args.output) as optimized:
            if optimized.convert('RGB').tobytes() != image.tobytes():
                raise RuntimeError('PNG optimization changed the captured pixels')
        print(f'optipng: {original_size} -> {args.output.stat().st_size} bytes; pixels unchanged',
              flush=True)
        close(window)
        success = True
        print(f'Saved {args.output.resolve()}', flush=True)
        return 0
    finally:
        if app is not None and app.poll() is None:
            app.terminate()
            try:
                app.wait(timeout=10)
            except subprocess.TimeoutExpired:
                app.kill()
                app.wait()
        log.close()
        connection.close()
        if success:
            shutil.rmtree(work)
        else:
            print(f'Workspace and diagnostic log retained at {work}', file=sys.stderr)


if __name__ == '__main__':
    sys.exit(main())
