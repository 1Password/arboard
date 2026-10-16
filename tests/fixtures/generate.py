"""Regenerate the original 3x2 red image fixtures with Python and Pillow."""

from pathlib import Path
import struct
import zlib
from PIL import Image

directory = Path(__file__).parent
image = Image.new("RGB", (3, 2), (255, 0, 0))
for extension in ("png", "jpeg", "webp", "bmp", "gif", "tiff"):
    image.save(directory / f"red.{extension}", lossless=True)
for extension in ("gif", "webp"):
    image.save(
        directory / f"animated.{extension}",
        save_all=True,
        append_images=[Image.new("RGB", (3, 2), (0, 255, 0))],
        duration=100,
        loop=0,
        lossless=True,
    )


def chunk(kind, data):
    return (
        struct.pack(">I", len(data))
        + kind
        + data
        + struct.pack(">I", zlib.crc32(kind + data))
    )


header = struct.pack(">IIBBBBB", 3, 2, 16, 2, 0, 0, 0)
pixels = (b"\0" + b"\xff\xff\0\0\0\0" * 3) * 2
(directory / "red16.png").write_bytes(
    b"\x89PNG\r\n\x1a\n"
    + chunk(b"IHDR", header)
    + chunk(b"IDAT", zlib.compress(pixels))
    + chunk(b"IEND", b"")
)
(directory / "qt-image.bin").write_bytes(
    b"\0\0\0\1" + (directory / "red.png").read_bytes()
)
