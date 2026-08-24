# Roadmap: staged work

## Resequenced after post-research pairing (2026-08-24)

The stages below remain valid as work packages, but the order changed
and the library work moves to the curator tool outside the-den:

1. scufris-desktop HUD v1 (task `20260822-132001`; architecture
   decided: own Tauri app, future third host of dashboardd-runtime).
2. today CLI and widget improvement, using the local widget dev loop
   (locally run dashboardd, DASHBOARDD_WIDGET_PATH at repo bundles).
3. Scufris x today (stage 1 below).
4. The curator: stages 2 through 5 below transfer to it, with its own
   store outside the-den. Stage 6 (briefing) follows Scufris x today
   and later moves onto the HUD canvas.

Original staging follows.

Stages are ordered by dependency and value density. Each is a small
vertical slice that ships through the release gate (prototype, test,
release, pin in nix.dotfiles, rebuild) and is useful on its own. The
HUD companion (task `20260822-132001`) proceeds in parallel and is not
a dependency of anything here.

## Stage 1: Scufris x today

The decided first slice. Adapter helper `tools/today/scufris-today`,
grouped native tools `scufris_den_read` and `scufris_den_write`
(ARCHITECTURE.md option 1B), `skills/den/SKILL.md` with the
answer-plus-widget rules, capability gating at session start.

- Value: Scufris finally answers from personal data; the flagship
  daily flows (UX.md 1-3) work end to end with widgets that already
  exist.
- Dependencies: none - today v0.4.0 and its widget are deployed.
- Risks: union-schema ergonomics (mitigate: start with the read tool,
  add writes once reads feel right); revision-conflict handling UX.
- Verification: extension tests with a fixture den (today's test
  fixtures are reusable); a live session answering UX flows 1-3;
  `npm run check` and `nix flake check`.

## Stage 2: den CLI and capture

New `den` repository (option 2A, v0 unstable): manifest format,
`add/set/note/list/show/path/search(rg)/verify`, fetch helpers plus
staging in scufris2, the capture conversation flow (UX.md 4-5) via
direct call and worker job. `domain: library` joins the read and write
tools.

- Value: the library exists; nova-protocol research becomes real
  captured material; the moat starts filling.
- Dependencies: stage 1 (tool plumbing and skill patterns).
- Risks: manifest format churn while unstable (accept until the first
  real research session, then tag v1); capture quality on JS-heavy
  pages (two fetch tools cover most; log failures).
- Verification: CLI unit tests plus a scripted end-to-end capture of a
  known page and video; one real "collect resources on roguelike
  deckbuilders" session producing kept items that survive `den verify`
  and a fresh clone (blobs absent, manifests and extracts intact).

## Stage 3: den viewer widget and browser helper

The `den` widget (item, inbox, board variants) shipped from the den
repo like today ships its widget; sanitize-and-forbid rendering;
keep/discard as direct widget actions. `scufris-browse` helper plus i3
window class rules.

- Value: "look what I found" works; triage gets a surface; citations
  become clickable.
- Dependencies: stage 2.
- Risks: sanitizer strictness versus readable extracts (the tatr-tasks
  forbid-list is the tested starting point); widget scope creep
  (variants stay three).
- Verification: widget catalog check against a live dashboardd (the
  today flake shows how); a session doing UX flows 5-6 including the
  browser handoff.

## Stage 4: search upgrade

`den search` gains SQLite FTS5 (derived index file, `den index
--rebuild`), ranking, and range citations. No daemon.

- Value: recall over a growing library; the citation path sharpens.
- Dependencies: stage 2 content actually existing (do not build search
  before there is something to search).
- Risks: none structural; the index is disposable by construction.
- Verification: delete the index, rebuild, identical results; recall
  questions from UX flow 6 against the real library.

## Stage 5: video and document ingestion

The pipeline from RESEARCH.md: whisper-server verbose_json transcripts,
ffmpeg scene keyframes, time-window join, docling for PDFs; persisted
as transcript.json plus keyframes through `den add`. Viewer gains the
video mode (UX.md 7).

- Value: videos and papers become consumable, citable knowledge - the
  learning-material substrate.
- Dependencies: stages 2-3; GPU minutes budgeted per item.
- Risks: ingestion cost creep (budget and log per-item cost from day
  one - the Rewind lesson); transcript quality on music-heavy videos.
- Verification: ingest one GDC talk end to end; timestamp citations
  resolve to the right keyframes.

## Stage 6: morning briefing

Proactive v1 within the designed policy: one systemd user timer, fixed
prompt, graduated delivery, per-topic mute, stated trigger, audit
entries. No watcher yet.

- Value: the machine speaks first, calmly; the proactivity design gets
  real-world calibration on the lowest-risk trigger.
- Dependencies: stage 1 (data to brief on); policy design from this
  task.
- Risks: annoyance (the budget and mute must exist in v1, not later).
- Verification: a week of use without a single unwanted interruption
  beyond the briefing itself; mute honored; audit trail complete.

## Evidence-gated and later

- Embeddings (sqlite-vec plus fastembed): only after recorded FTS5
  misses on real queries (RESEARCH.md gate).
- Course builder: after stages 2-5 provide material; concept exists in
  designs/course-concept.html; UX.md flow 10.
- Calendar: khal plus vdirsyncer when a real timed-event need appears;
  presumptive design in research/calendar.md.
- den watcher as a proactive source: only when a concrete trigger need
  exists; scope and debounce budgeted.
- Hardware embodiment: library research topic now, no build.
- Wake word: HUD task territory, reuses its start action.

## What is explicitly not built

No custom calendar or workout CLIs. No database as canonical store. No
Scufris hooks inside dashboardd. No external-URL webview widget (the
browser covers it). No always-on capture or screen recording. No
speculative embeddings, knowledge graph, or sync service. No merging of
repositories.
