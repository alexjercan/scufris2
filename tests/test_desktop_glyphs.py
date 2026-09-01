"""The desktop conversation glyphs are generated, and the committed files match.

A shipped asset nobody can regenerate is one nobody can change. These assert
that every glyph the generator knows about is on disk exactly as it renders it,
and that what it renders is the flat single-colour shape the window's controls
are built for.
"""

import importlib.util
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
GENERATOR = ROOT / "surfaces" / "desktop" / "tools" / "generate_glyphs.py"
UI = ROOT / "surfaces" / "desktop" / "ui"


def load():
    spec = importlib.util.spec_from_file_location("generate_glyphs", GENERATOR)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class DesktopGlyphTests(unittest.TestCase):
    def setUp(self) -> None:
        self.glyphs = load()

    def test_every_committed_glyph_matches_its_generator(self) -> None:
        for name in self.glyphs.GLYPHS:
            with self.subTest(glyph=name):
                self.assertEqual(
                    (UI / name).read_text(encoding="utf-8"),
                    self.glyphs.render(name),
                )

    def test_rendering_is_deterministic(self) -> None:
        for name in self.glyphs.GLYPHS:
            with self.subTest(glyph=name):
                self.assertEqual(self.glyphs.render(name), self.glyphs.render(name))

    def test_the_way_back_is_a_down_arrow_on_the_window_ground(self) -> None:
        drawn = self.glyphs.render("latest.svg")
        self.assertIn('viewBox="0 0 16 16"', drawn)
        self.assertIn(f'fill="{self.glyphs.GROUND}"', drawn)
        self.assertIn("<title>Newer messages below</title>", drawn)
        points = [
            tuple(float(part) for part in pair.split(","))
            for pair in drawn.split('points="')[1].split('"')[0].split(" ")
        ]
        centre = self.glyphs.SIZE / 2
        tip = max(points, key=lambda point: point[1])
        self.assertEqual(tip[0], centre, "the arrow does not point straight down")
        # The head is the widest part of the shape and sits under the stem.
        widest = max(point[0] for point in points) - min(point[0] for point in points)
        top = [point for point in points if point[1] == min(y for _, y in points)]
        self.assertLess(max(x for x, _ in top) - min(x for x, _ in top), widest)

    def test_the_committed_glyphs_are_exactly_what_the_generator_knows(self) -> None:
        self.assertEqual(
            sorted(path.name for path in UI.glob("*.svg")),
            sorted(self.glyphs.GLYPHS),
        )


if __name__ == "__main__":
    unittest.main()
