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
from urllib.parse import urlsplit

from markdown_it import MarkdownIt
from markdown_it.token import Token
from markdown_it.utils import EnvType, OptionsDict

CONTROL = re.compile(r"[\x00-\x1f\x7f]")
MAX_LINK = 8 * 1024
HEADING_OFFSET = 2

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
.markdown { min-width: 0; max-width: 100%; }
.markdown h3, .markdown h4, .markdown h5, .markdown h6 {
  margin: 20px 0 6px;
  font-size: 13px;
  font-weight: 500;
  letter-spacing: 0.1em;
  text-transform: uppercase;
  color: var(--muted);
}
.markdown p, .markdown ul, .markdown ol { margin: 8px 0; }
.markdown ul, .markdown ol { padding-left: 20px; }
.markdown li { margin: 3px 0; }
.markdown a, .headline a { color: var(--attention); overflow-wrap: anywhere; }
.markdown code, .headline code { background: #1e1e1e; border-radius: 3px; padding: 0 4px; font-size: 13px; }
.markdown pre {
  max-width: 100%;
  background: #1e1e1e;
  border: 1px solid var(--line);
  border-radius: 3px;
  padding: 12px 14px;
  overflow-x: auto;
}
.markdown pre code { background: none; padding: 0; }
.markdown blockquote {
  margin: 8px 0;
  padding-left: 14px;
  border-left: 2px solid var(--line);
  color: var(--muted);
}
.markdown hr { border: none; border-top: 1px solid var(--line); margin: 20px 0; }
.table-scroll {
  max-width: 100%;
  margin: 16px 0;
  overflow-x: auto;
  overscroll-behavior-inline: contain;
  scrollbar-color: #5b5855 #1e1e1e;
  scrollbar-width: thin;
  -webkit-overflow-scrolling: touch;
}
.table-scroll::-webkit-scrollbar { height: 8px; }
.table-scroll::-webkit-scrollbar-track { background: #1e1e1e; }
.table-scroll::-webkit-scrollbar-thumb { background: #5b5855; border-radius: 4px; }
.markdown table {
  width: 100%;
  min-width: 32rem;
  border-collapse: collapse;
  border-spacing: 0;
  font-size: 13px;
  line-height: 1.45;
}
.markdown th, .markdown td {
  min-width: 8rem;
  max-width: 28rem;
  padding: 8px 10px;
  border: 1px solid #4b4845;
  vertical-align: top;
  white-space: normal;
  overflow-wrap: anywhere;
}
.markdown th {
  background: #252321;
  color: var(--strong);
  font-weight: 600;
}
.markdown tbody tr:nth-child(even) td { background: #1b1b1b; }
.empty { color: var(--muted); }
.offers ol { list-style: none; margin: 0; padding: 0; }
.offers li { border-top: 1px solid var(--line); padding: 14px 0; }
.offers li:first-child { border-top: none; padding-top: 0; }
.offers .pick { display: flex; align-items: baseline; gap: 10px; }
.offers .number {
  min-width: 1.2em; font-variant-numeric: tabular-nums;
  font-weight: 600; color: var(--muted);
}
.offers .label { color: var(--strong); }
.offers .detail { margin: 4px 0 0 calc(1.2em + 10px); color: var(--muted); }
footer {
  margin-top: 48px;
  padding-top: 20px;
  border-top: 1px solid var(--line);
  color: var(--muted);
  font-size: 12px;
}
footer ul { margin: 8px 0 0; padding-left: 18px; }
@media (max-width: 620px) {
  body { padding: 28px 12px 64px; }
  h1 { font-size: 22px; }
  .card { padding: 16px 14px; }
  .markdown th, .markdown td { min-width: 7.5rem; max-width: 20rem; padding: 7px 8px; }
}
"""


def safe_link(target: str) -> bool:
    """Whether untrusted Markdown may turn this destination into a link."""
    if len(target) > MAX_LINK or CONTROL.search(target):
        return False
    try:
        parsed = urlsplit(target)
        # Reading the port also rejects malformed and out-of-range values.
        _ = parsed.port
    except ValueError:
        return False
    return (
        parsed.scheme.lower() in ("http", "https")
        and parsed.hostname not in (None, "")
        and parsed.username is None
        and parsed.password is None
    )


def render_token(
    tokens: list[Token], index: int, options: OptionsDict, env: EnvType
) -> str:
    return MARKDOWN.renderer.renderToken(tokens, index, options, env)


def render_link_open(
    tokens: list[Token], index: int, options: OptionsDict, env: EnvType
) -> str:
    """Keep a safe link, or render only its label when its target is unsafe."""
    token = tokens[index]
    allowed = safe_link(token.attrGet("href") or "")
    depth = 1
    for following in tokens[index + 1 :]:
        if following.type == "link_open":
            depth += 1
        elif following.type == "link_close":
            depth -= 1
            if depth == 0:
                following.meta["safe_link"] = allowed
                break
    if not allowed:
        return ""
    token.attrSet("rel", "noreferrer")
    return render_token(tokens, index, options, env)


def render_link_close(
    tokens: list[Token], index: int, options: OptionsDict, env: EnvType
) -> str:
    return (
        render_token(tokens, index, options, env)
        if tokens[index].meta.get("safe_link")
        else ""
    )


def render_heading(
    tokens: list[Token], index: int, options: OptionsDict, env: EnvType
) -> str:
    """Keep the page title above headings supplied by briefing Markdown."""
    token = tokens[index]
    token.tag = f"h{min(int(token.tag[1:]) + HEADING_OFFSET, 6)}"
    return render_token(tokens, index, options, env)


def render_image(
    tokens: list[Token], index: int, _options: OptionsDict, _env: EnvType
) -> str:
    """Keep image alt text without letting a page fetch untrusted resources."""
    return html.escape(tokens[index].content)


def markdown_parser() -> MarkdownIt:
    """The one Markdown pipeline used by prose, contributions, and offers."""
    parser = MarkdownIt(
        "commonmark",
        {"html": False, "linkify": False, "xhtmlOut": False},
    ).enable("table")
    # Parse every syntactically valid destination so the renderer can remove
    # an unsafe target without also returning its Markdown delimiters.
    parser.validateLink = lambda _target: True
    parser.renderer.rules["link_open"] = render_link_open
    parser.renderer.rules["link_close"] = render_link_close
    parser.renderer.rules["heading_open"] = render_heading
    parser.renderer.rules["heading_close"] = render_heading
    parser.renderer.rules["image"] = render_image
    parser.renderer.rules["table_open"] = lambda *_args: (
        '<div class="table-scroll" role="region" aria-label="Scrollable table" '
        'tabindex="0">\n<table>\n'
    )
    parser.renderer.rules["table_close"] = lambda *_args: "</table>\n</div>\n"
    return parser


MARKDOWN = markdown_parser()


def inline(text: str) -> str:
    """Inline Markdown through the same safe parser as every block."""
    return MARKDOWN.renderInline(text)


def markdown(text: str) -> str:
    """CommonMark plus GFM tables, with raw HTML and unsafe links inert."""
    return MARKDOWN.render(text)


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
        f"<dd>{html.escape(fact['value'])}</dd></div>"
        for fact in facts
    )
    return f'<dl class="facts">{entries}</dl>'


def card(contribution: dict[str, Any]) -> str:
    status = str(contribution["status"])
    status_class = status if status in STATUS_WORDS else "failed"
    word = STATUS_WORDS.get(status, status)
    body = markdown(contribution.get("body", "") or "")
    return (
        f'<section class="card">'
        f"<h2>{html.escape(contribution['title'])}"
        f'<span class="pill {status_class}">{html.escape(word)}</span>'
        f'<span class="source">{html.escape(contribution["project"])}</span></h2>'
        f'<p class="headline">{inline(contribution["headline"])}</p>'
        f"{facts_block(contribution.get('facts', []))}"
        f'<div class="body markdown">{body}</div>'
        f"</section>"
    )


def offers_block(offers: list[dict[str, Any]]) -> str:
    """The numbered list of what could be done next.

    Its own block and not a card, because it belongs to no one source: the
    numbers are the run's, merged across everything that answered. A run where
    nothing was offered draws nothing rather than an empty heading.
    """
    if not offers:
        return ""
    items = "".join(
        f'<li><div class="pick">'
        f'<span class="number">{html.escape(str(offer["number"]))}</span>'
        f'<span class="label">{inline(offer["label"])}</span>'
        f'<span class="source">{html.escape(offer["project"])}</span>'
        f'</div><p class="detail">{inline(offer["detail"])}</p></li>'
        for offer in offers
    )
    return f'<section class="card offers"><h2>Next</h2><ol>{items}</ol></section>'


def run_footer(manifest: dict[str, Any]) -> str:
    counted = len(manifest["sources"])
    failed = [item for item in manifest["sources"] if item["status"] == "failed"]
    collected = html.escape(str(manifest.get("finished") or "not yet"))
    profile = html.escape(str(manifest["profile"]).capitalize())
    date = html.escape(str(manifest["date"]))
    lines = [
        f"{profile} run of {date}, "
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
        f'<div class="lede markdown">{markdown(prose)}</div>'
        if prose
        else '<p class="lede empty">This run has no prose yet.</p>'
    )
    cards = "".join(card(item) for item in run["contributions"])
    if not cards:
        cards = '<p class="empty">No project declared this briefing.</p>'
    cards += offers_block(manifest.get("offers", []))
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
