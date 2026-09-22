"""Generate PWA PNG icons for self-tools (192, 512, maskable 512).

Runs once at release time; output committed under apps/desktop/ui/public/icons/.
"""
from PIL import Image, ImageDraw

OUT = "D:/code/self-github/self-tools/apps/desktop/ui/public/icons"


def render(size: int, maskable: bool, path: str) -> None:
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)
    if maskable:
        # maskable safe zone: full-bleed background, glyph inside ~80% center.
        draw.rectangle([0, 0, size, size], fill=(13, 19, 21, 255))
        side = int(size * 0.8)
        ox = (size - side) // 2
        oy = (size - side) // 2
    else:
        radius = int(size * 0.22)
        draw.rounded_rectangle([0, 0, size, size], radius=radius, fill=(13, 19, 21, 255))
        ox, oy = 0, 0
        side = size
    k = side / 512.0

    def box(x0: int, y0: int, x1: int, y1: int):
        return [ox + x0 * k, oy + y0 * k, ox + x1 * k, oy + y1 * k]

    draw.rounded_rectangle(box(128, 144, 256, 368), radius=int(20 * k), fill=(22, 136, 255, 255))
    draw.rectangle(box(320, 160, 384, 352), fill=(214, 220, 224, 255))
    img.save(path, "PNG")
    print("wrote", path)


render(192, False, f"{OUT}/icon-192.png")
render(512, False, f"{OUT}/icon-512.png")
render(512, True, f"{OUT}/icon-maskable-512.png")
