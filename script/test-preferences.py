#!/usr/bin/env python3
"""Exercise Options and Diagnostics via X11 (python3-xlib, xclip; Xvfb supported)."""
import json
import os
import pathlib
import subprocess
import sys
import tempfile
import time

from Xlib import X, XK, display, protocol
from Xlib.ext import xtest


def wait_for(check, description, timeout=15):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        result = check()
        if result:
            return result
        time.sleep(0.1)
    raise RuntimeError(f"Timed out waiting for {description}")


def main():
    connection = display.Display()
    root = connection.screen().root
    name_atom = connection.intern_atom("_NET_WM_NAME")

    def find(title):
        for window in root.query_tree().children:
            try:
                value = window.get_full_property(name_atom, X.AnyPropertyType)
                if (value and bytes(value.value).decode("utf-8") == title
                        and window.get_attributes().map_state == X.IsViewable):
                    return window
            except Exception:
                continue
        return None

    def key(name, down=True):
        code = connection.keysym_to_keycode(XK.string_to_keysym(name))
        assert code, name
        xtest.fake_input(connection, X.KeyPress if down else X.KeyRelease, code)
        connection.sync()

    def press(name):
        key(name)
        key(name, False)
        time.sleep(0.12)

    def click(window, x, y):
        point = root.translate_coords(window, x, y)
        xtest.fake_input(connection, X.MotionNotify, x=point.x, y=point.y)
        xtest.fake_input(connection, X.ButtonPress, 1)
        xtest.fake_input(connection, X.ButtonRelease, 1)
        connection.sync()
        time.sleep(0.15)

    def close(window):
        window.send_event(protocol.event.ClientMessage(
            window=window,
            client_type=connection.intern_atom("WM_PROTOCOLS"),
            data=(32, [connection.intern_atom("WM_DELETE_WINDOW"), X.CurrentTime, 0, 0, 0]),
        ), event_mask=0)
        connection.flush()

    with tempfile.TemporaryDirectory(prefix="phaselimiter-preferences-") as directory:
        env = dict(os.environ)
        env.pop("WAYLAND_DISPLAY", None)
        env.pop("WAYLAND_SOCKET", None)
        env["XDG_DATA_HOME"] = directory
        app = subprocess.Popen([str(pathlib.Path(sys.argv[1]).resolve())], env=env)
        try:
            parent = wait_for(lambda: find("Phaselimiter"), "main window")
            library = pathlib.Path(directory) / "phaselimiter-gui" / "library.json"

            def open_preferences():
                parent.set_input_focus(X.RevertToParent, X.CurrentTime)
                connection.sync()
                time.sleep(0.3)
                key("Alt_L")
                press("t")
                key("Alt_L", False)
                time.sleep(0.15)
                press("o")
                dialog = wait_for(lambda: find("Options"), "Preferences window")
                dialog.set_input_focus(X.RevertToParent, X.CurrentTime)
                connection.sync()
                time.sleep(0.5)
                return dialog

            def edit_backend(dialog, value):
                click(dialog, 150, 142)
                key("Control_L")
                press("a")
                key("Control_L", False)
                for character in value:
                    code = connection.keysym_to_keycode(ord(character))
                    xtest.fake_input(connection, X.KeyPress, code)
                    xtest.fake_input(connection, X.KeyRelease, code)
                connection.sync()
                time.sleep(0.15)

            def accept(dialog):
                size = dialog.get_geometry()
                click(dialog, size.width - 140, size.height - 27)
                wait_for(lambda: find("Options") is None, "accepted dialog closing")

            for dismissal in ["cancel", "escape", "close"]:
                dialog = open_preferences()
                edit_backend(dialog, "/tmp/discarded")
                close(parent)
                time.sleep(0.2)
                assert find("Options") and app.poll() is None, "Parent close bypassed modality"
                if dismissal == "cancel":
                    size = dialog.get_geometry()
                    click(dialog, size.width - 52, size.height - 27)
                elif dismissal == "escape":
                    dialog.set_input_focus(X.RevertToParent, X.CurrentTime)
                    connection.sync()
                    press("Escape")
                else:
                    close(dialog)
                wait_for(lambda: find("Options") is None, f"{dismissal} closing dialog")
                accept(open_preferences())
                assert json.loads(library.read_text())["app_settings"]["backend"] == ""

            dialog = open_preferences()
            edit_backend(dialog, "/tmp/accepted")
            accept(dialog)
            assert json.loads(library.read_text())["app_settings"]["backend"] == "/tmp/accepted", library.read_text()
            dialog = open_preferences()
            dialog.configure(width=440, height=320)
            connection.sync()
            time.sleep(0.8)
            click(dialog, 388, 293)
            wait_for(lambda: find("Options") is None, "minimum-size Cancel")
            assert json.loads(library.read_text())["app_settings"]["backend"] == "/tmp/accepted", library.read_text()
            # Diagnostics is independent: opening Options from the parent must still work.
            parent.set_input_focus(X.RevertToParent, X.CurrentTime)
            connection.sync()
            time.sleep(0.3)
            key("Alt_L")
            press("t")
            key("Alt_L", False)
            press("d")
            report = wait_for(lambda: find("Diagnostics"), "diagnostic window")
            report.configure(width=440, height=280)
            report.set_input_focus(X.RevertToParent, X.CurrentTime)
            connection.sync()
            time.sleep(0.8)
            click(report, 60, 253)
            def read_clipboard():
                result = subprocess.run(["xclip", "-selection", "clipboard", "-o"],
                                        timeout=5, text=True, capture_output=True)
                return result.stdout if result.returncode == 0 else None

            snapshot = json.loads(wait_for(read_clipboard, "report clipboard contents"))
            assert snapshot["settings"]["backend"] == "/tmp/accepted"
            assert "status_messages" in snapshot and "audio_log" in snapshot
            dialog = open_preferences()
            assert find("Diagnostics"), "Options unexpectedly closed diagnostics"
            close(report)
            wait_for(lambda: find("Diagnostics") is None, "diagnostic window close")
            assert find("Options"), "Report close incorrectly closed Options"
            close(dialog)
            wait_for(lambda: find("Options") is None, "Options close")
            close(parent)
            app.wait(timeout=10)
            assert app.returncode == 0
            print("PASS: separate dialog, parent modality, OK persistence, Cancel/Esc/close rollback, minimum-size buttons, nonmodal diagnostics and JSON clipboard, normal shutdown")
        finally:
            if app.poll() is None:
                app.terminate()
                app.wait(timeout=5)
            connection.close()


if __name__ == "__main__":
    main()
