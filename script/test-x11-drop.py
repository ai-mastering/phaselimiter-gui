#!/usr/bin/env python3
"""Deliver a real XDND file drop to the running app (Xvfb is supported).

Requires python3-xlib. This exercises the OS/windowing event path;
it does not call the application's update method or inject an iced message.
"""
import os
import pathlib
import subprocess
import sys
import tempfile
import time

from Xlib import X, display, protocol


def wait_for(check, description, timeout=15):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        result = check()
        if result:
            return result
        time.sleep(0.1)
    raise RuntimeError(f"Timed out waiting for {description}")


def main():
    executable = pathlib.Path(sys.argv[1]).resolve()
    connection = display.Display()
    root = connection.screen().root
    name_atom = connection.intern_atom("_NET_WM_NAME")

    def title(window):
        try:
            value = window.get_full_property(name_atom, X.AnyPropertyType)
            return bytes(value.value).decode("utf-8") if value else ""
        except Exception:
            return ""

    def find_window():
        for window in root.query_tree().children:
            if title(window).startswith("Phaselimiter |"):
                return window
        return None

    with tempfile.TemporaryDirectory(prefix="phaselimiter-xdnd-") as directory:
        path = pathlib.Path(directory) / "日本語 audio file.wav"
        path.write_bytes(b"test input; audio is not processed in this test")
        env = dict(os.environ)
        env.pop("WAYLAND_DISPLAY", None)
        env.pop("WAYLAND_SOCKET", None)
        application = subprocess.Popen([str(executable)], env=env)
        source = None
        try:
            target = wait_for(find_window, "GUI window")
            assert "0 queued" in title(target), title(target)
            # This independent X11 client owns the selection, like a file manager.
            selection = connection.intern_atom("XdndSelection")
            uri_type = connection.intern_atom("text/uri-list")
            source = root.create_window(0, 0, 1, 1, 0, X.CopyFromParent)
            source.set_selection_owner(selection, X.CurrentTime)
            connection.sync()
            assert connection.get_selection_owner(selection).id == source.id

            def send(name, values):
                event = protocol.event.ClientMessage(
                    window=target, client_type=connection.intern_atom(name), data=(32, values)
                )
                target.send_event(event, event_mask=0)
                connection.flush()

            send("XdndEnter", [source.id, 5 << 24, uri_type, 0, 0])
            geometry = target.get_geometry()
            position = target.translate_coords(root, geometry.width // 2, geometry.height // 2)
            coordinates = ((position.x & 0xFFFF) << 16) | (position.y & 0xFFFF)
            position_data = [source.id, 0, coordinates, X.CurrentTime, connection.intern_atom("XdndActionCopy")]
            send("XdndPosition", position_data)
            transfer = {"served": False, "statuses": 0}

            def pump():
                while connection.pending_events():
                    event = connection.next_event()
                    if event.type == X.SelectionRequest:
                        property_atom = event.property or event.target
                        accepted = event.target == uri_type
                        if accepted:
                            event.requestor.change_property(property_atom, uri_type, 8,
                                                            (path.as_uri() + "\r\n").encode())
                        event.requestor.send_event(protocol.event.SelectionNotify(
                            time=event.time, requestor=event.requestor,
                            selection=event.selection, target=event.target,
                            property=property_atom if accepted else X.NONE), event_mask=0)
                        connection.flush()
                        transfer["served"] = accepted
                    elif event.type == X.ClientMessage and event.client_type == connection.intern_atom("XdndStatus"):
                        transfer["statuses"] += 1

            def transferred():
                pump()
                return transfer["served"]

            wait_for(transferred, "URI selection transfer during drag")
            # A second status acknowledges processing of SelectionNotify before
            # XdndDrop, as an interactive file manager's drag would do.
            connection.sync()
            pump()
            before = transfer["statuses"]
            send("XdndPosition", position_data)

            def acknowledged():
                pump()
                return transfer["statuses"] > before

            wait_for(acknowledged, "drag acceptance after selection transfer")
            send("XdndDrop", [source.id, 0, X.CurrentTime, 0, 0])

            def queued():
                pump()
                return "1 queued" in title(target)

            wait_for(queued, "dropped Unicode filename in queue")
            print("PASS: real XDND drop with Unicode and spaces added one file to the queue")
            assert application.poll() is None, "GUI exited unexpectedly"
        finally:
            application.terminate()
            application.wait(timeout=5)
            if source is not None:
                source.destroy()
            connection.close()


if __name__ == "__main__":
    main()
