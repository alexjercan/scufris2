"""The briefing as one page, rendered from a run that is already finished.

This reads. It never asks a source anything, never calls a model, and never
decides what the day means - it is given a run and lays it out. That is the
whole reason the page exists as a separate step: the morning can be re-rendered
a year from now and say exactly what it said, because nothing in here is a
judgement.

The page is one file with its styling inside it. It opens from a state
directory with no server, no fonts to fetch and no script to run, which is also
what makes it safe to point at content the day produced.

The palette is the one the desktop panels wear. It is copied rather than read,
because `surfaces/desktop/shell/tokens.css` is not part of what the agent is
given, and a page that silently loses its colours in a deployment is worse than
one that carries them.
"""

from __future__ import annotations

import html
import re
from datetime import date as Date
from typing import Any

SAFE_SCHEME = re.compile(r"^(?:https?|mailto|file):", re.IGNORECASE)
#: One pass over a line: a code span, or a link. Whichever starts first wins,
#: so a link inside backticks stays text and a path in backticks stays a label.
SPAN = re.compile(r"`([^`]+)`|\[([^\]]+)\]\(((?:[^()\s]|\([^()\s]*\))+)\)")
BOLD = re.compile(r"\*\*([^*]+)\*\*")
ITALIC = re.compile(r"(?<!\*)\*([^*\n]+)\*(?!\*)")
BULLET = re.compile(r"^[-*]\s+(.*)$")
NUMBERED = re.compile(r"^\d+[.)]\s+(.*)$")
HEADING = re.compile(r"^(#{1,6})\s+(.*)$")

STATUS_WORDS = {
    "ok": "clear",
    "attention": "needs you",
    "stale": "no data",
    "failed": "no answer",
}

STYLE = """
:root {
  --bg: #101010;
  --panel: #161616;
  --line: #33302e;
  --fg: #e4e4ef;
  --strong: #f4f4ff;
  --muted: #95a99f;
  --accent: #95a99f;
  --attention: #9e95c7;
  --warn: #ffdd33;
  --alarm: #f43841;
  --mono: Iosevka, "Iosevka Nerd Font", "JetBrains Mono", ui-monospace, monospace;
}
* { box-sizing: border-box; }
body {
  margin: 0;
  padding: 48px 24px 96px;
  background: var(--bg);
  color: var(--fg);
  font-family: var(--mono);
  font-size: 15px;
  line-height: 1.6;
}
main { max-width: 760px; margin: 0 auto; }
header { border-bottom: 1px solid var(--line); padding-bottom: 20px; margin-bottom: 32px; }
h1 {
  margin: 0;
  font-size: 30px;
  font-weight: 500;
  letter-spacing: 0.16em;
  text-transform: uppercase;
  color: var(--strong);
}
.when { margin: 8px 0 0; color: var(--muted); font-size: 13px; }
.lede { margin-bottom: 40px; }
.lede p:first-child { margin-top: 0; }
.card {
  border: 1px solid var(--line);
  background: var(--panel);
  border-radius: 4px;
  padding: 20px 22px;
  margin-bottom: 20px;
}
.card > h2 {
  margin: 0;
  font-size: 13px;
  font-weight: 500;
  letter-spacing: 0.16em;
  text-transform: uppercase;
  color: var(--strong);
  display: flex;
  align-items: baseline;
  gap: 10px;
  flex-wrap: wrap;
}
.pill {
  font-size: 11px;
  letter-spacing: 0.08em;
  padding: 1px 8px;
  border: 1px solid currentColor;
  border-radius: 999px;
}
.ok { color: var(--accent); }
.attention { color: var(--attention); }
.stale { color: var(--warn); }
.failed { color: var(--alarm); }
.source { margin-left: auto; color: var(--muted); font-size: 11px; letter-spacing: 0.04em; }
.headline { margin: 12px 0 0; color: var(--strong); }
.facts {
  display: flex;
  flex-wrap: wrap;
  gap: 8px 28px;
  margin: 16px 0 0;
  padding: 14px 0 0;
  border-top: 1px solid var(--line);
}
.fact { min-width: 120px; }
.fact dt { color: var(--muted); font-size: 11px; letter-spacing: 0.08em; text-transform: uppercase; }
.fact dd { margin: 2px 0 0; color: var(--strong); font-size: 18px; }
.body { margin-top: 16px; }
.body h3, .body h4, .body h5 {
  margin: 20px 0 6px;
  font-size: 13px;
  font-weight: 500;
  letter-spacing: 0.1em;
  text-transform: uppercase;
  color: var(--muted);
}
.body p, .body ul, .body ol { margin: 8px 0; }
.body ul, .body ol { padding-left: 20px; }
.body li { margin: 3px 0; }
.body a { color: var(--attention); }
.body code, .headline code { background: #1e1e1e; border-radius: 3px; padding: 0 4px; font-size: 13px; }
.body pre {
  background: #1e1e1e;
  border: 1px solid var(--line);
  border-radius: 3px;
  padding: 12px 14px;
  overflow-x: auto;
}
.body pre code { background: none; padding: 0; }
.body blockquote {
  margin: 8px 0;
  padding-left: 14px;
  border-left: 2px solid var(--line);
  color: var(--muted);
}
.body hr { border: none; border-top: 1px solid var(--line); margin: 20px 0; }
.empty { color: var(--muted); }
footer {
  margin-top: 48px;
  padding-top: 20px;
  border-top: 1px solid var(--line);
  color: var(--muted);
  font-size: 12px;
}
footer ul { margin: 8px 0 0; padding-left: 18px; }
@media (max-width: 620px) {
  body { padding: 28px 16px 64px; }
  h1 { font-size: 22px; }
}
"""


def inline(text: str) -> str:
    """Markdown inside one line, on text that is escaped first.

    Escaping before markup, rather than after, is what keeps a source from
    writing HTML into the page.

    Code spans and links are found in one left-to-right pass rather than one
    rule after another. Cutting the backticks out first would leave a link
    whose label is a path in backticks - which is most of the links a briefing
    writes - split across pieces no link rule could match again.
    """
    escaped = html.escape(text)
    rendered: list[str] = []
    end = 0
    for match in SPAN.finditer(escaped):
        rendered.append(emphasis(escaped[end : match.start()]))
        code, label, target = match.groups()
        if code is not None:
            rendered.append(f"<code>{code}</code>")
        else:
            rendered.append(anchor(label, target))
        end = match.end()
    rendered.append(emphasis(escaped[end:]))
    return "".join(rendered)


def emphasis(text: str) -> str:
    """The rules that hold no text of their own."""
    return ITALIC.sub(r"<em>\1</em>", BOLD.sub(r"<strong>\1</strong>", text))


def anchor(label: str, target: str) -> str:
    """A link, or its words when the target is not one this page will follow.

    The label is rendered whichever way it goes, so a dropped link costs the
    reader the target and never the words.
    """
    inside = "".join(
        f"<code>{piece}</code>" if index % 2 else emphasis(piece)
        for index, piece in enumerate(label.split("`"))
    )
    if SAFE_SCHEME.match(target) or target.startswith(("/", "#", ".")):
        return f'<a href="{target}" rel="noreferrer">{inside}</a>'
    return inside


def markdown(text: str) -> str:
    """The Markdown a briefing writes, and no more.

    Headings, paragraphs, lists, quotes, rules, fenced code, and the inline
    rules above. A source that reaches for a table or a footnote gets its
    source text back as a paragraph, which reads badly and loses nothing.
    """
    lines = text.replace("\r\n", "\n").split("\n")
    out: list[str] = []
    paragraph: list[str] = []
    listing: str | None = None
    fence: list[str] | None = None

    def close_paragraph() -> None:
        nonlocal paragraph
        if paragraph:
            out.append(f"<p>{inline(' '.join(paragraph))}</p>")
            paragraph = []

    def close_list() -> None:
        nonlocal listing
        if listing:
            out.append(f"</{listing}>")
            listing = None

    for line in lines:
        stripped = line.strip()
        if fence is not None:
            if stripped.startswith("```"):
                body = html.escape("\n".join(fence))
                out.append(f"<pre><code>{body}</code></pre>")
                fence = None
            else:
                fence.append(line)
            continue
        if stripped.startswith("```"):
            close_paragraph()
            close_list()
            fence = []
            continue
        if not stripped:
            close_paragraph()
            close_list()
            continue
        heading = HEADING.match(stripped)
        if heading:
            close_paragraph()
            close_list()
            level = min(len(heading.group(1)) + 2, 6)
            out.append(f"<h{level}>{inline(heading.group(2))}</h{level}>")
            continue
        if set(stripped) <= {"-", "*", "_"} and len(stripped) >= 3:
            close_paragraph()
            close_list()
            out.append("<hr>")
            continue
        if stripped.startswith("> "):
            close_paragraph()
            close_list()
            out.append(f"<blockquote>{inline(stripped[2:])}</blockquote>")
            continue
        bullet = BULLET.match(stripped)
        numbered = NUMBERED.match(stripped)
        if bullet or numbered:
            close_paragraph()
            wanted = "ul" if bullet else "ol"
            if listing != wanted:
                close_list()
                out.append(f"<{wanted}>")
                listing = wanted
            item = (bullet or numbered).group(1)
            out.append(f"<li>{inline(item)}</li>")
            continue
        close_list()
        paragraph.append(stripped)
    if fence is not None:
        # An unterminated fence is still what the source wrote. Closing it here
        # keeps the page valid and keeps the text visible.
        out.append(f"<pre><code>{html.escape(chr(10).join(fence))}</code></pre>")
    close_paragraph()
    close_list()
    return "\n".join(out)


def written_date(value: str) -> str:
    try:
        day = Date.fromisoformat(value)
    except ValueError:
        return value
    return day.strftime("%A, %-d %B %Y")


def facts_block(facts: list[dict[str, Any]]) -> str:
    if not facts:
        return ""
    entries = "".join(
        f'<div class="fact"><dt>{html.escape(fact["label"])}</dt>'
        f'<dd>{html.escape(fact["value"])}</dd></div>'
        for fact in facts
    )
    return f'<dl class="facts">{entries}</dl>'


def card(contribution: dict[str, Any]) -> str:
    status = contribution["status"]
    word = STATUS_WORDS.get(status, status)
    body = markdown(contribution.get("body", "") or "")
    return (
        f'<section class="card">'
        f"<h2>{html.escape(contribution['title'])}"
        f'<span class="pill {status}">{html.escape(word)}</span>'
        f'<span class="source">{html.escape(contribution["project"])}</span></h2>'
        f'<p class="headline">{inline(contribution["headline"])}</p>'
        f"{facts_block(contribution.get('facts', []))}"
        f'<div class="body">{body}</div>'
        f"</section>"
    )


def run_footer(manifest: dict[str, Any]) -> str:
    counted = len(manifest["sources"])
    failed = [item for item in manifest["sources"] if item["status"] == "failed"]
    collected = html.escape(str(manifest.get("finished") or "not yet"))
    lines = [
        f"{manifest['profile'].capitalize()} run of {manifest['date']}, "
        + f"{counted} source{'' if counted == 1 else 's'}, "
        + f"collected {collected}."
    ]
    if failed:
        named = "".join(
            f"<li>{html.escape(item['project'])}: {html.escape(item['headline'])}</li>"
            for item in failed
        )
        lines.append(f"<ul>{named}</ul>")
    for diagnostic in manifest.get("diagnostics", []):
        lines.append(
            f"<p>{html.escape(diagnostic['project'])}: "
            f"{html.escape(diagnostic['diagnostic'])}</p>"
        )
    return f"<footer>{''.join(lines)}</footer>"


def render_page(run: dict[str, Any]) -> str:
    """One run as one file."""
    manifest = run["manifest"]
    title = f"{manifest['profile'].capitalize()} briefing"
    prose = run.get("prose")
    lede = (
        f'<div class="lede">{markdown(prose)}</div>'
        if prose
        else '<p class="lede empty">This run has no prose yet.</p>'
    )
    cards = "".join(card(item) for item in run["contributions"])
    if not cards:
        cards = '<p class="empty">No project declared this briefing.</p>'
    return f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{html.escape(title)} - {html.escape(manifest["date"])}</title>
<style>{STYLE}</style>
</head>
<body>
<main>
<header>
<h1>{html.escape(title)}</h1>
<p class="when">{html.escape(written_date(manifest["date"]))}</p>
</header>
{lede}
{cards}
{run_footer(manifest)}
</main>
</body>
</html>
"""
