#!/usr/bin/env python3
"""Generate the desktop conversation glyphs.

Every glyph the HUD draws is computed here rather than drawn by hand, so the
shipped file is readable as the arithmetic that produced it and a change to the
shape is a change to a number. `tests/test_desktop_glyphs.py` regenerates each
one and fails when the committed file has drifted.

The glyphs are flat single-colour marks on the HUD's own ground colour. They
sit on filled quartz controls, the way the `+` on the attach control does, so
the mark is painted in the window's background colour rather than in
`currentColor`: an `<img>` cannot inherit one, and a control this small is
better served by a shape that cannot fail to render than by a mask that can.
"""

from pathlib import Path

UI = Path(__file__).resolve().parent.parent / "ui"

# `--bg` in ui/hud.css. The mark is the hole in a filled control.
GROUND = "#101010"

# One square viewBox for every glyph, so a caller sizes with CSS alone.
SIZE = 16


def down_arrow() -> str:
    """A solid arrow pointing at the newest line: a stem under a head."""
    centre = SIZE / 2
    top = 3
    tip = SIZE - 3
    stem = 2
    head = 10
    shoulder = tip - head / 2
    points = [
        (centre - stem / 2, top),
        (centre + stem / 2, top),
        (centre + stem / 2, shoulder),
        (centre + head / 2, shoulder),
        (centre, tip),
        (centre - head / 2, shoulder),
        (centre - stem / 2, shoulder),
    ]
    return polygon(points)


def polygon(points: list[tuple[float, float]]) -> str:
    """One closed shape, with whole numbers written without a decimal point."""
    drawn = " ".join(f"{number(x)},{number(y)}" for x, y in points)
    return f'  <polygon points="{drawn}" fill="{GROUND}" />'


def number(value: float) -> str:
    return str(int(value)) if float(value).is_integer() else f"{value:g}"


def document(title: str, body: str) -> str:
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {SIZE} {SIZE}"'
        f' width="{SIZE}" height="{SIZE}" role="img">\n'
        f"  <title>{title}</title>\n"
        f"{body}\n"
        f"</svg>\n"
    )


GLYPHS = {"latest.svg": ("Newer messages below", down_arrow)}


def render(name: str) -> str:
    title, draw = GLYPHS[name]
    return document(title, draw())


def main() -> None:
    for name in GLYPHS:
        (UI / name).write_text(render(name), encoding="utf-8")


if __name__ == "__main__":
    main()
