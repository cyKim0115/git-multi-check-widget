"""Build a Windows-reliable multi-size BMP ICO from app-icon.png."""
from __future__ import annotations

import struct
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
SRC = ROOT / "app-icon.png"
ICONS = ROOT / "src-tauri" / "icons"
OUT_ICO = ICONS / "icon.ico"
# Standard Windows shell sizes — BMP (not PNG) entries embed reliably via winres.
SIZES = [16, 32, 48, 256]


def to_bmp_dib(im: Image.Image) -> bytes:
    """Return BITMAPINFOHEADER + BGRA XOR bitmap + empty AND mask (ICO style)."""
    im = im.convert("RGBA")
    w, h = im.size
    pixels = im.split()
    r, g, b, a = [ch.tobytes() for ch in pixels]
    bgra = bytearray(w * h * 4)
    for i in range(w * h):
        bgra[i * 4 + 0] = b[i]
        bgra[i * 4 + 1] = g[i]
        bgra[i * 4 + 2] = r[i]
        bgra[i * 4 + 3] = a[i]

    stride = w * 4
    xor = bytearray(stride * h)
    for row in range(h):
        src_off = row * stride
        dst_off = (h - 1 - row) * stride
        xor[dst_off : dst_off + stride] = bgra[src_off : src_off + stride]

    and_row = ((w + 31) // 32) * 4
    and_mask = bytes(and_row * h)

    header = struct.pack(
        "<IIIHHIIIIII",
        40,
        w,
        h * 2,
        1,
        32,
        0,
        len(xor) + len(and_mask),
        0,
        0,
        0,
        0,
    )
    return header + bytes(xor) + and_mask


def write_ico(frames: list[Image.Image], dest: Path) -> None:
    dibs = [to_bmp_dib(im) for im in frames]
    count = len(frames)
    offset = 6 + 16 * count
    entries = bytearray()
    blob = bytearray()
    for im, dib in zip(frames, dibs):
        w, h = im.size
        entries += struct.pack(
            "<BBBBHHII",
            0 if w >= 256 else w,
            0 if h >= 256 else h,
            0,
            0,
            1,
            32,
            len(dib),
            offset + len(blob),
        )
        blob += dib
    dest.write_bytes(struct.pack("<HHH", 0, 1, count) + entries + blob)


def main() -> None:
    img = Image.open(SRC).convert("RGBA")
    # Opaque black tile — Explorer shows this more reliably than fully transparent bg.
    bg = Image.new("RGBA", img.size, (0, 0, 0, 255))
    composed = Image.alpha_composite(bg, img)

    frames: list[Image.Image] = []
    for s in SIZES:
        im = composed.resize((s, s), Image.Resampling.LANCZOS)
        frames.append(im)
        if s == 32:
            im.save(ICONS / "32x32.png")
        if s == 256:
            # Tauri conf also references these PNGs.
            im.resize((128, 128), Image.Resampling.LANCZOS).save(ICONS / "128x128.png")
            im.save(ICONS / "128x128@2x.png")
            im.save(ICONS / "icon.png")

    write_ico(frames, OUT_ICO)
    print(f"wrote {OUT_ICO} bytes={OUT_ICO.stat().st_size} sizes={SIZES}")


if __name__ == "__main__":
    main()
