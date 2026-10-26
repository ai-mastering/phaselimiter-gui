#!/usr/bin/env python3
"""Check generated formats, sizes and transparency consumed by the application."""
from pathlib import Path
import unittest

from PIL import Image

ICONS = Path(__file__).resolve().parent.parent / "build/icons"


class Icons(unittest.TestCase):
    def check_image(self, image, size):
        rgba = image.convert("RGBA")
        self.assertEqual(rgba.size, (size, size))
        self.assertEqual(rgba.getchannel("A").getextrema(), (0, 255))

    def test_window_and_linux_pngs(self):
        for size in (256, 512):
            with self.subTest(size=size), Image.open(ICONS / f"phaselimiter-{size}.png") as image:
                self.assertEqual(image.format, "PNG")
                self.check_image(image, size)

    def test_windows_ico_contains_all_sizes_and_matches_window_icon(self):
        with Image.open(ICONS / "phaselimiter.ico") as icon:
            sizes = {16, 24, 32, 48, 64, 128, 256}
            self.assertEqual(icon.ico.sizes(), {(size, size) for size in sizes})
            for size in sizes:
                self.check_image(icon.ico.getimage((size, size)), size)
            with Image.open(ICONS / "phaselimiter-256.png") as png:
                self.assertEqual(icon.ico.getimage((256, 256)).convert("RGBA").tobytes(),
                                 png.convert("RGBA").tobytes())

    def test_macos_icns_contains_all_sizes_and_matches_linux_icon(self):
        with Image.open(ICONS / "phaselimiter.icns") as icon:
            self.assertEqual({w * scale for w, h, scale in icon.info["sizes"]},
                             {16, 32, 64, 128, 256, 512, 1024})
            for size in icon.info["sizes"]:
                self.check_image(icon.icns.getimage(size), size[0] * size[2])
            with Image.open(ICONS / "phaselimiter-512.png") as png:
                self.assertEqual(icon.icns.getimage((512, 512, 1)).convert("RGBA").tobytes(),
                                 png.convert("RGBA").tobytes())


if __name__ == "__main__":
    unittest.main()
