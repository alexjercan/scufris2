#!/usr/bin/env python3
"""Checks that the paths and symbols the lane briefs name still resolve.

A lane that greps for a file that does not exist reports a pass, so a brief
that has gone stale is worse than no brief. Run this after editing one.

Every backticked token in the briefs is classified. A token that looks like a
path must exist; a token that looks like a code symbol must appear somewhere
outside `tasks/`, which is append-only history and proves nothing about now.
Anything else is prose and is skipped, which is reported so the classification
itself can be reviewed.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

BRIEFS = Path(__file__).parent
ROOT = BRIEFS.parents[2]

# Where a brief may write a path relative to, in the order they are tried.
# The desktop crate is its own world: a brief says `src/pill.rs`, not the
# whole way down from the repository root, and often just `state.rs`.
BASES = [
    ROOT,
    ROOT / "surfaces" / "desktop",
    ROOT / "surfaces" / "desktop" / "src",
    BRIEFS,
]

# Prose, shell, and things owned by somebody else. A token here is a claim
# this script does not check, so keep it short and keep it honest.
SKIP = {
    # Severities and report headings.
    "BLOCKER",
    "MAJOR",
    "MINOR",
    "Checked:",
    "Not checked:",
    # X11, window manager, and browser vocabulary.
    "PointerRoot",
    "None",
    "WM_HINTS.input",
    "_NET_WM_STATE_ABOVE",
    "focus_follows_mouse",
    "focus_follows_mouse yes",
    "prefers-reduced-motion",
    "activeElement",
    "Object.assign",
    "<textarea>",
    "xprop",
    "xwininfo",
    "xdotool",
    "xmessage",
    # TypeScript and Cargo vocabulary.
    "any",
    "strict",
    "noUncheckedIndexedAccess",
    "dependencies",
    "peerDependencies",
    "std::thread",
    # Nix and npm option names, checked by the nix build rather than here.
    "programs.scufris.*",
    "pi.extensions",
    "pi.skills",
    # Runtime artifacts, not tree paths.
    "pending.json",
}

# Command lines, git ranges, and skill arguments: anything with a space, a
# leading dash, an angle bracket placeholder, or a `..`.
PROSE = re.compile(r"[ <>]|^-|\.\.")

PATHISH = re.compile(r"^\.?[\w][\w./*-]*$")
EXTENSIONS = {".rs", ".ts", ".js", ".css", ".html", ".json", ".nix", ".md", ".toml"}
SYMBOL = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*(::[A-Za-z_][A-Za-z0-9_]*)*$")


def tokens() -> set[str]:
    found: set[str] = set()
    for brief in sorted(BRIEFS.rglob("*.md")):
        found |= set(re.findall(r"`([^`\n]+)`", brief.read_text()))
    return found


def is_path(token: str) -> bool:
    if not PATHISH.match(token):
        return False
    # A dotfile has no suffix to recognise it by, and is a path either way.
    return "/" in token or token.startswith(".") or Path(token).suffix in EXTENSIONS


def path_resolves(token: str) -> bool:
    for base in BASES:
        if "*" in token:
            if any(base.glob(token)):
                return True
        elif (base / token).exists():
            return True
    return False


def symbol_resolves(token: str) -> bool:
    # The last segment is the one that has to be written down somewhere: the
    # qualifier is often a module or a type the brief spells for the reader.
    needle = token.split("::")[-1]
    # Whole words only. `every_window` is a substring of a test named
    # `every_window_label_...`, and a substring match would have reported a
    # symbol that does not exist as present.
    found = subprocess.run(
        ["git", "grep", "--untracked", "-lwF", "--", needle],
        cwd=ROOT,
        capture_output=True,
        text=True,
        # A symbol nothing mentions is exit 1 and is the answer, not a failure.
        check=False,
    )
    for line in found.stdout.splitlines():
        if not line.startswith("tasks/") and not line.startswith(".agents/"):
            return True
    return False


def main() -> int:
    paths, symbols, skipped, stale = [], [], [], []
    for token in sorted(tokens()):
        if token in SKIP or PROSE.search(token):
            skipped.append(token)
        elif is_path(token):
            paths.append(token)
            if not path_resolves(token):
                stale.append(f"path does not exist: {token}")
        elif SYMBOL.match(token):
            symbols.append(token)
            if not symbol_resolves(token):
                stale.append(f"symbol is written nowhere: {token}")
        else:
            skipped.append(token)

    print(f"{len(paths)} paths, {len(symbols)} symbols, {len(skipped)} skipped")
    print("skipped: " + ", ".join(skipped))
    for line in stale:
        print(line, file=sys.stderr)
    return 1 if stale else 0


if __name__ == "__main__":
    sys.exit(main())
