#!/usr/bin/env python3
"""Register an extracted Linux distribution for the current desktop user."""
import os
from pathlib import Path
import shutil

APP_ID = "com.aimastering.phaselimiter-gui"


def desktop_string(value):
    return value.replace("\\", "\\\\").replace("\n", "\\n").replace("\r", "\\r").replace("\t", "\\t")


def exec_argument(path):
    # Exec has a second quoting layer, and % introduces desktop field codes.
    value = str(path).replace("%", "%%")
    for character in ('\\', '"', '`', '$'):
        value = value.replace(character, "\\" + character)
    return desktop_string('"' + value + '"')


def install(bundle, data):
    executable = bundle / "phaselimiter-gui"
    if not executable.is_file():
        raise FileNotFoundError(f"Run the installer beside the extracted executable: {executable}")
    icon = data / "icons/hicolor/512x512/apps" / (APP_ID + ".png")
    icon.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(bundle / "phaselimiter.png", icon)
    template = (bundle / (APP_ID + ".desktop")).read_text()
    content = template.replace("Exec=phaselimiter-gui", "Exec=" + exec_argument(executable))
    content = content.replace("Icon=" + APP_ID, "Icon=" + desktop_string(str(icon)))
    entry = data / "applications" / (APP_ID + ".desktop")
    entry.parent.mkdir(parents=True, exist_ok=True)
    entry.write_text(content)
    return entry


if __name__ == "__main__":
    data = Path(os.environ.get("XDG_DATA_HOME") or Path.home() / ".local/share")
    if not data.is_absolute():
        data = Path.home() / ".local/share"
    print(install(Path(__file__).resolve().parent, data))
