#!/usr/bin/env python3
"""Generate build/icons from the source PNG using Pillow and optipng."""
from pathlib import Path
import struct
import subprocess
import tempfile

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
ICONS = ROOT / "build/icons"


def optimize(path):
    with Image.open(path) as image:
        before = image.convert("RGBA").tobytes()
    old_size = path.stat().st_size
    subprocess.run(["optipng", "-quiet", "-o2", str(path)], check=True)
    with Image.open(path) as image:
        assert image.convert("RGBA").tobytes() == before, f"Pixels changed: {path}"
    print(f"{path.name}: {old_size} -> {path.stat().st_size} bytes; RGBA unchanged")


def main():
    source = ROOT / "assets/icons/phaselimiter-concept-v1.png"
    ICONS.mkdir(parents=True, exist_ok=True)
    with Image.open(source) as image:
        original = image.convert("RGBA")
    with tempfile.TemporaryDirectory() as directory:
        images = {}
        for size in (16, 24, 32, 48, 64, 128, 256, 512, 1024):
            path = Path(directory) / f"{size}.png"
            original.resize((size, size), Image.Resampling.LANCZOS).save(path)
            optimize(path)
            images[size] = path.read_bytes()
        for size in (256, 512):
            (ICONS / f"phaselimiter-{size}.png").write_bytes(images[size])
        # Modern Windows supports PNG images inside ICO. Preserve the optimized
        # streams instead of re-encoding them when constructing the container.
        sizes = (16, 24, 32, 48, 64, 128, 256)
        offset = 6 + 16 * len(sizes)
        entries = []
        for size in sizes:
            entries.append(struct.pack("<BBBBHHII", size % 256, size % 256,
                                       0, 0, 1, 32, len(images[size]), offset))
            offset += len(images[size])
        (ICONS / "phaselimiter.ico").write_bytes(
            struct.pack("<HHH", 0, 1, len(sizes)) + b"".join(entries)
            + b"".join(images[size] for size in sizes))
        chunks = []
        for kind, size in ((b"icp4", 16), (b"icp5", 32), (b"icp6", 64),
                           (b"ic07", 128), (b"ic08", 256), (b"ic09", 512), (b"ic10", 1024)):
            chunks.append(kind + struct.pack(">I", 8 + len(images[size])) + images[size])
        payload = b"".join(chunks)
        (ICONS / "phaselimiter.icns").write_bytes(b"icns" + struct.pack(">I", 8 + len(payload)) + payload)


if __name__ == "__main__":
    main()
