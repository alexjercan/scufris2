# Redesign the custom Quick Review page

- STATUS: CLOSED
- PRIORITY: 100
- TAGS: quick-review, ui, review

## Scope

Rebuild the Quick Review page as a GitHub-PR-review-inspired but distinct
terminal-styled interface. This recovers crashed job c9958e6a5315. The change
is confined to the UI layer of `tools/quick-review/quick_review.py` (CSS, JS,
prose rendering, page rendering) plus a new deterministic preview harness. The
bridge protocol, init/result validation, HTTP server hardening, and the
`walkthrough.ts` state machine are unchanged, so every capability and the
exact-revision safety protocol are preserved.

## Design decisions

- One coherent token set (`--bg`, `--panel`, `--ink`, `--muted`, `--line`,
  `--strong`, `--accent`, `--ok`, `--err`, `--warn`, diff backgrounds,
  `--hover`, `--on-solid`) drives light and dark through
  `prefers-color-scheme`. No other colors exist in the stylesheet.
- Square everywhere: the stylesheet contains no `border-radius` and no
  `box-shadow`; a render test asserts both. Chips became bordered uppercase
  tags, the progress bar is a bordered rectangle, and the busy spinner is a
  blinking square instead of a rotating circle.
- The whole page uses one monospace stack. Hierarchy comes from weight,
  size steps, uppercase letterspaced labels, and 1px separators, not from
  mixed families or decoration.
- Section `markdown` and the document summary render through a new
  `render_markdown` safe subset renderer: paragraphs, headings (demoted to
  h3-h6 so page chrome keeps h1/h2), unordered and ordered lists,
  blockquotes, fenced code blocks, horizontal rules, inline code, bold, and
  italic. Everything is HTML-escaped first; inline markup is applied to
  escaped text only. Link syntax renders as text plus a code-styled URL, and
  the page emits no `<a>` except internal `#change-*` anchors and the
  stylesheet link; a test enumerates every `href`.
- Navigation is a Changes index: one row per section with a live `[ ]`/`[x]`
  viewed marker, the section title, `file:lines`, and the importance tag,
  each linking to its card. The progress counter and bar live in the index
  header and update through the existing `data-reviewed`/`data-total`/
  `data-progress` hooks.
- Keyboard behavior: `j`/`k` move focus between change cards
  (`tabindex="-1"` targets with `scroll-margin-top`), `v` toggles the focused
  card's viewed checkbox through the same transactional action path. Keys are
  ignored while typing and with modifiers held. A skip link jumps to the
  index; `:focus-visible` gets a 2px accent outline globally.
- All DOM contracts consumed by the client script are unchanged
  (`data-card`, `data-viewed`, `data-state`, `data-answers`, `data-comments`,
  `data-blocks`, `data-scope`, `.overall-comment`, feedback scopes), and the
  action protocol keeps the exact action names. The checkbox scope's compact
  feedback is now visible when non-empty so persistence errors surface at the
  control, matching the recorded transactional-checkbox decision.
- Long content: file paths and inline code wrap with `overflow-wrap:anywhere`,
  diffs and code blocks scroll horizontally inside their own
  container (the diff `pre` is keyboard-focusable), comment bodies are
  `pre-wrap`, and the facts grid, index rows, and card headers reflow below
  680px. Reduced-motion preferences disable the spinner and smooth scroll.
- Full revisions (head and base) are shown in the masthead facts grid; the
  short revision stays in the header line. Approval gating is still rendered
  server-side (`disabled` until all sections are viewed and no change
  requests exist) and re-derived client-side from authoritative state.
- Preview support is a new `tools/quick-review/preview.py`: it serves the
  real page and server against a deterministic in-process bridge whose
  reducer mirrors the foreground walkthrough semantics (viewed transitions,
  bounded comments, canned explain/ask answers, context, terminal
  approve/request-changes). Every reducer response passes the production
  `validate_result` before reaching the page. The fixture exercises
  structured prose, long paths, long diff lines, warnings, a preexisting
  comment, an answered question, and a pre-viewed collapsed section.

## Verification

- `python3 -m unittest tests.test_quick_review tests.test_quick_review_preview`:
  17 tests passed (renderer escaping and structure, markdown subset safety,
  navigation and internal-link enumeration, keyboard wiring, viewed
  collapse, transport bounds, shutdown ordering, preview fixture validity,
  full preview flow to approval and terminal lock).
- `ruff check` and `ruff format --check` on both tools and both test
  modules: passed.
- `npm run check`: typecheck passed, all 55 TypeScript tests passed
  (including the walkthrough bridge lifecycle test that spawns the
  redesigned `quick_review.py` end to end), Prettier passed.
- `git diff --check`: passed.
- `nix flake check`: all checks passed.
- End-to-end smoke test against `preview.py --no-open`: page, `style.css`,
  and `app.js` served 200; `mark-viewed`, `context`, and `add-comment`
  actions round-tripped through the real server and validated reducer.
- Headless Chromium screenshots at 1280px (light and forced dark) and 420px:
  masthead facts, warning, index with viewed marks, collapsed viewed cards,
  structured prose, diff coloring with horizontal overflow, prompt block,
  answered question, comment threads, and the final review with the enabled
  "Approve with comments" primary action and hidden Request changes all
  render correctly in both themes; narrow layout wraps index rows and stacks
  card headers.

## Preview

- `python3 tools/quick-review/preview.py` prints a local URL and opens the
  browser (`--no-open` to suppress). Ctrl-C stops it. State is in-memory and
  resets on restart; approve or request-changes locks the session terminal,
  matching production.
