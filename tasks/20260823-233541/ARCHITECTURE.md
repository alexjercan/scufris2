# Architecture: options and recommendations

Covers the three build areas plus the proactive design and the shared
concerns (trust, failure, packaging). Grounded in INVENTORY.md facts and
RESEARCH.md findings.

## System shape

```
                      +---------------------------+
                      |         the-den           |
                      |  Daily/  Notes/  Library/ |
                      |  canonical plain files    |
                      +------+-------------+------+
                             |             |
                       today CLI       den CLI (new)
                       daily JSON      manifests, blobs,
                       contract        search, verify
                             |             |
                   +---------+-------------+---------+
                   |  Scufris (Pi orchestrator)      |
                   |  native tools via private       |
                   |  helper adapters                |
                   +---+---------------------+-------+
                       |                     |
                 dashboardctl          fetch helpers
                       |               (monolith, yt-dlp,
                +------+------+        trafilatura, ...)
                | dashboardd  |        staging dir, then
                | widgets:    |        den CLI persists
                | today, den  |
                | viewer, ... |
                +-------------+
```

Scufris observes through CLIs, presents through dashboardctl, and
fetches through helpers into a staging area that only the den CLI
persists. dashboardd stays Scufris-agnostic; new widgets ship from the
repos that own the data (the today widget precedent).

## Area 1: Scufris x today

Both options reuse the proven adapter pattern: a private Python helper
(`tools/today/scufris-today`, mirroring `tools/dashboard/
scufris-dashboard`) called through `runPrivateHelper`, shelling to
`today --json` with a deadline and bounded JSON envelopes. The helper
validates requests and never invokes bare `today` (which opens an
editor).

### Option 1A: one tool per subcommand

Register `scufris_today_show`, `_task`, `_habit`, `_weight`, `_macros`,
`_note`, `_upcoming` (~7-9 tools) with exact per-command schemas.

- For: maximal schema guidance; mirrors the widget-tool precedent.
- Against: pushes the total tool count toward the ~30 ceiling
  (MARKET.md) as more domains arrive; every future domain repeats the
  pattern; the model must still learn seven names.

### Option 1B (recommended): grouped read and write tools

Two tools, mirroring the observation/mutation split:

- `scufris_den_read` - union query schema: `{domain: today, query:
show|upcoming|weight_history|macros_day|notes|habits, date?, days?}`.
  Read-only, always safe, no acknowledgment gate.
- `scufris_den_write` - union mutation schema: `{domain: today, action:
task_add|task_done|habit_toggle|weight_set|macros_add|note_add|...,
date?, payload}`. Revision conflicts surface as tool errors that
  instruct a re-read; destructive actions (rm) require explicit user
  confirmation in conversation.

- For: two names to learn; the read/write asymmetry encodes the safety
  model; scales to future domains (`domain: library` later) without new
  tools; stays far under the tool ceiling.
- Against: union schemas are bigger than per-command ones; slightly
  weaker per-action guidance (mitigated by a `skills/den/SKILL.md` in
  the mold of the dashboard skill).

Answer-plus-widget: after a read, the model may open the matching today
widget variant with the same date scope. The skill codifies the rule:
answer from data first, open a widget when the user asks to see it,
when a trend or list is easier shown than spoken, or as confirmation
after a mutation. One spoken sentence plus one relevant surface; never
a widget instead of an answer.

Failure behavior: if `today` is absent or its `--version` is outside
the declared compatibility range at `session_start`, the tools do not
register and identity notes the capability as unavailable (same
pattern as a missing dashboardd). Revision conflicts and lock timeouts
map to typed tool errors.

Packaging: scufris2's Home Manager module gains an optional
`todayPackage`; nix.dotfiles already pins today. Compatibility is a
declared range over today's JSON contract version.

## Area 2: the library

### Layout inside the-den

```
the-den/
  Library/
    items/<id>/
      manifest.md        # tracked: frontmatter + curated notes
      extract.md         # tracked: trafilatura/docling text
      transcript.json    # tracked: word:timestamp (videos)
      blobs/             # ignored: snapshot.html, media, keyframes
  .gitignore             # adds Library/items/*/blobs/
```

`<id>` is date plus slug (`20260824-astar-blog`). Manifest.md carries
YAML frontmatter: id, title, source_url, captured_at, modality, topics,
license if known, status (inbox|kept|discarded), trust (untrusted web
content by default), a files list with per-file sha256 and byte size,
and provenance (capturing conversation or command). The body is for
human or Scufris curation notes. Tracked text (manifest, extract,
transcript) is greppable and Obsidian-linkable; blobs are content
addressed by recorded hash and excluded from Git. `den verify` detects
missing or corrupted blobs; blob backup is an rsync or restic job
documented in nix.dotfiles, separate from Git.

This follows ArchiveBox's per-item-directory precedent (MARKET.md)
without its coupled fetch-store-serve design.

### The den CLI

In the today mold: stdlib-only Python, JSON everywhere, atomic writes,
revision checks, never fetches the network.

- Write surface (narrow, deterministic): `den add <path...> --url
--title --topics --modality` (moves or copies local files into
  blobs/, computes hashes, writes manifest with status inbox),
  `den set <id> --status --topics` (triage: keep, discard, retag),
  `den note <id>` (append curation note).
- Read surface (rich): `den list --status --topic --json`, `den show
<id> --json`, `den path <id> [--file]`, `den search <text> --json`
  (v1: rg under the hood; v2: FTS5 in a derived SQLite file), `den
verify [--json]`, later `den index --rebuild`.

Curation is first-class: capture lands in `inbox` status and a triage
moment (conversation or viewer widget) moves items to `kept`, because
capture without triage becomes a write-only archive (MARKET.md).

### Where the CLI lives

- Option 2A (recommended): a new `den` repository in the today mold -
  own tests, releases, widget, and skill; pinned in nix.dotfiles.
  Clear ownership: today owns Daily/, den owns Library/. Cost: one
  more repo through the release gate.
- Option 2B: incubate as scufris2 helpers first, promote to a repo
  once the manifest format survives real use. Cheaper start, but the
  format becomes load-bearing while unversioned, and today's own
  history (two ad-hoc scripts replaced by a package) argues for
  starting packaged.

Recommendation: 2A, but with a v0 tag treated as unstable until the
first real nova-protocol research session has exercised it.

### Capture flow

1. In conversation: "collect resources about roguelike deckbuilders
   for nova-protocol."
2. Scufris runs fetch helpers (monolith or single-file-cli, yt-dlp,
   gallery-dl) into a staging directory, then extraction (trafilatura,
   docling, whisper-server plus ffmpeg for video) - directly for one
   URL, or as a delegated worker job for a broad sweep.
3. `den add` persists each item with provenance; Scufris reports what
   was captured and opens the viewer or list widget for triage.
4. Nothing outside staging is written except by `den add`; failed
   fetches never touch the library.

## Area 3: reference display

Per RESEARCH.md, split by content type; no dashboardd core changes.

- A generic `den` viewer widget, shipped from the den repo exactly as
  today ships its widget. Typed input: a den item reference (id plus
  optional file or range). Backend resolves it through `den show` and
  `den path` and serves markdown, text, or image content; the frontend
  renders through sanitize-and-forbid (the tatr-tasks details
  precedent). Variants: `item` (single reference), `inbox` (triage
  list), `board` (topic collection, e.g. nova-protocol inspiration).
- Live URLs and PDFs: a small `scufris-browse` helper opens the real
  browser with a dedicated window class; i3 rules float and place it.
  Scufris can close or focus it via i3-msg by class. Weaker contract
  than a widget, correct isolation for live web content.
- Deferred: a generic external-URL window in dashboardd (Tauri CSP and
  external-URL rough edges documented in research/reference-display.md).

Citations: Scufris cites den items by id (and line or timestamp range);
the citation maps 1:1 to a viewer-widget input, so "show me" is always
one tool call away.

## Proactive contact (design only)

One policy layer inside the Scufris daemon; trigger sources feed it,
they never reach the user directly:

- systemd user timers (morning briefing) - first source when built.
- worker job events - already exist.
- a den watcher (later, if a real trigger need appears; watch scope
  and debounce budgeted from day one).

Policy: offer, do not act. Every proactive surface states its trigger
and why; per-topic mute and graduated escalation (transcript note ->
HUD attention state -> spoken) instead of one kill switch; quiet hours;
a daily interruption budget; every decision audited as a session entry.
The HUD task's attention state is the natural delivery point; until it
ships, the popup plus a dunst summary suffices for the briefing.

Hooks the first slices should leave: typed tool errors and events
already flow through the daemon; the briefing needs only a way to
start a turn from a timer with a fixed prompt - no new architecture.

## Trust boundaries

- Captured content is data. Manifests label trust and provenance;
  extracts render sanitized; retrieved text is quoted, never obeyed.
  Imperative text inside a captured page must never authorize actions.
- The den CLI never fetches; fetch helpers never persist. The split is
  the injection firebreak: nothing goes from web to library without
  the deterministic persist step recording provenance.
- Mutations of den data follow the same acknowledgment discipline as
  workflow actions; external effects (fetching is read-only network
  access; publishing anything is out of scope) keep needing explicit
  approval.

## Failure and recovery

- today or den CLI missing or version-skewed: tools degrade to absent
  with a stated capability gap.
- Blob missing (unsynced machine): `den verify` explains; the viewer
  shows manifest plus extract and says the snapshot is unavailable.
- Derived index deleted or stale: rebuilt from tracked files in
  minutes; never authoritative.
- dashboardd down: answers still work (observation is CLI-side);
  presentation degrades to text, matching the existing dashboard
  extension behavior.

## Packaging and deployment

Every component rides the existing gate: den repo (CLI, widget, skill)
released and pinned like today; scufris2 gains the den and today
adapters, tools, and skills in a release; nix.dotfiles bumps pins and
wires `DEN_PATH`, the widget package, and the backup job. Iteration
before release uses `nix run` and dev shells, per the established
workflow. No new daemons; the only new always-running thing is nothing.
