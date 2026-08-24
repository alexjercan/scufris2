# Jarvis vision: Scufris over the-den, answering with widgets

## Purpose

This is the research handoff for task `20260823-233541`. It replaces an
earlier generated draft. It states the actual vision, the verified current
state, the decided direction, and the open questions. Treat the vision and
the rejected directions as settled. Treat everything else as research input.

## Vision

I control the machine with my voice, and the machine talks back with what it
knows.

Concretely:

- The main interface is a conversation with Scufris. Voice is the preferred
  input and output. Text remains available.
- Scufris answers with details plus references. When showing beats telling,
  it opens a Dashboardd widget: a graph, a task list, an agenda, a source.
- Widgets are also places to act. Scufris can open a widget for me to do
  something in it directly, without routing every click through the model.
- The machine can start the conversation. Not shell notifications, but
  substantive contact: knowledge findings, project state, things worth
  saying. "Look what I found" with the reference open on screen.
- Widgets earn their place because strong CLI tools back them. An agent can
  drive a CLI cheaply and reliably. A browser needs heavy reasoning to use;
  a widget over a CLI does not.

This is Jarvis from Iron Man, scoped to one Linux desktop.

## Rejected directions

Do not spend research effort on these. They were considered and dropped.

- SQLite or any database as the canonical store. Canonical data stays
  plain files. A derived, rebuildable retrieval index (lexical or
  embedding) is explicitly allowed for the library; deleting it must
  never lose data.
- A custom calendar CLI. If calendar data arrives, it is standard `.ics`
  files in the-den plus existing tools (`khal`, `vdirsyncer`, or similar).
- A custom workouts CLI or moving workouts out of daily notes. Workout
  content, if recorded, stays in the daily Markdown file.
- "Everything is a widget." i3 plus rofi already handle application-level
  workflow. Widgets are for assistant-driven presentation and interaction,
  not a second window manager.
- Restructuring the-den for its own sake. Round 2: per-domain formats
  are negotiable when they serve the today CLI and widgets; the-den
  stays personal notes, journal, and tracking. The research library
  moved out to the curator tool.
- Merging repositories. Composition happens in nix.dotfiles.

## The pieces and their roles

Six parts, five repos, one data vault. All verified 2026-08-23.

### Scufris (`~/personal/scufris2`, v0.2.0) - the brain

A Pi package. Pi owns the foreground conversation; Scufris adds identity,
delegated job workflow, widget control, calm presentation, and speech
output. Native tools today: `scufris_final_response`, the `scufris_job_*`
suite, and `scufris_widget_open/update/list/focus/close` generated at
session start from the discovered widget catalog
(`extensions/scufris/dashboard/index.ts`, adapter
`tools/dashboard/scufris-dashboard` shelling to `dashboardctl`).

Scufris has zero integration with `today` or the-den. Grep confirms no
references anywhere in the repo. This is the largest gap.

### scufris-desktop (task `20260822-132001`) - the face

The planned voice HUD and tray companion. Super+D opens a bottom-center
HUD, records, transcribes with local Whisper, and submits to the popup Pi
backend over a narrow same-user control channel. The popup backend (the
Scufris daemon) owns the conversation; the companion owns activation,
recording, review, tray state, and health monitoring. That task is the
interaction layer of this vision and proceeds in parallel. This research
must treat its control channel and states (listening, transcribing,
working, speaking, attention, error) as the front door that knowledge
answers and proactive contact eventually flow through.

### the-den (`~/personal/the-den`) - the knowledge

An Obsidian-style vault, 8.1 MB, private Git repo. `Daily/` holds 1,140
files (2023-04-15 to today) named `YYYY-MM-DD-Weekday.md` with H3 sections
Tasks, Habits, Macros (CSV rows), Weight, Notes (timestamped H4 entries).
`Notes/` holds annual goal pages, design docs, and `Notes/Videos/` saved
video notes. `Templates/daily.md` seeds new entries. `tasks/` is empty.
The format stays. Additive growth candidates: `calendar/` with `.ics`
files, a library or inbox area for saved pages, videos, courses, research
material, and attachments.

### today (`~/personal/today`, v0.4.0) - the proven contract

Stdlib-only Python CLI, the only reader and writer of daily Markdown.
Subcommands `path`, `create`, `show`, `task`, `habit`, `weight`, `macros`,
`note`, `upcoming`, all with `--json`. Edits splice only the five known H3
regions and preserve everything else byte for byte; writes are atomic with
revision-based conflict detection. It ships an agent skill
(`flake.skills.today`) and a complete Dashboardd widget package with six
variants (tasks, habits, macros, weight, notes, upcoming) over a JSON
Lines backend. Its README already names the missing piece: "scufris wraps
the subcommands as MCP tools" is planned and not done.

This is the template for every future domain tool: a small CLI that owns
one area of the-den, speaks JSON, preserves human edits, and ships its own
widget and skill.

### dashboardd (`~/personal/dashboardd`, v0.2.0) - the display

Two hosts embedding one runtime: `dashboardd` (Axum server, browser SPA,
SSE events) and `dashboardd-desktop` (Tauri tray app; each surface is a
real native window, floating under i3). `dashboardctl` speaks JSON over a
Unix socket: `discover`, `open`, `update`, `list`, `focus`, `close`,
`quit`, with stable error codes and audit logging. Widgets have typed
inputs and outputs, can receive frontend commands, publish outputs, and
mutate shared state; external clients can watch the SSE stream. Shipped
widgets: cpu, memory, disk, network, claude-usage, codex-usage, projects,
tatr-tasks, plus the today widget from the today flake.

One relevant absence: no widget can display an arbitrary URL or web page.
One deliberate boundary to keep: dashboardd knows nothing about Scufris
and stays that way. Scufris is just another `dashboardctl` client, and
research must not propose Scufris-specific hooks inside dashboardd.

### nix.dotfiles (`~/personal/nix.dotfiles`) - the gate

All tools are flake inputs pinned to release tags: today v0.4.0,
dashboardd v0.2.0, scufris2 v0.2.0, tatr v2.0.3, pi. `DEN_PATH` points at
the-den. The today widget is wired into both dashboardd hosts. Whisper STT
runs as a loopback systemd user service (`whisper-server`,
whisper-cpp-vulkan, large-v3-turbo) consumed by Pi's `voice-stt`
extension. Piper TTS ships inside the scufris flake. i3, rofi, kitty,
dunst, PipeWire. The Scufris popup is a Mod4+s scratchpad window.

Updates are a manual ritual: bump the tag in `flake.nix`, `nix flake
update <input>`, `nix flake check`, rebuild. This friction is a safety
gate, not a problem to solve away. Research must fit the lifecycle:
prototype, test, release, pin, rebuild.

## What already works

The voice loop exists end to end today: Whisper STT in, Pi conversation,
Piper TTS out, popup on Mod4+s. Scufris can already discover, open,
update, focus, and close widgets. The today widget is deployed. The HUD
task upgrades the front door; it does not create it.

What does not exist: Scufris cannot read or write anything in the-den. It
can open the today tasks widget but cannot answer "what tasks do I have
tomorrow" from data. Observation is the missing half; presentation is
mostly built.

## The gaps, in priority order

1. Scufris x today (decided first vertical slice). Give Scufris the today
   contract as native Pi tools wrapping `today --json`: query and CRUD
   daily data (tasks, habits, weight, macros, notes, upcoming) and pair
   spoken answers with the already-deployed widget variants. Research the
   exact tool surface and how answer-plus-widget becomes one smooth
   response.
2. The library (decided second focus; collection before retrieval). The
   den holds zero reference material today, so capture comes first. See
   "The library" below.
3. References as widgets. A way for Scufris to show a source: a web page,
   a saved note, a library item. Both paths stay open - a webview-style
   dashboardd widget and the real browser - chosen per content type by
   the research. This is the "look what I found" move.
4. Proactive contact (design now, build later). The machine speaks first
   for things that matter. All trigger sources are candidates - systemd
   timers, a den watcher, worker job events - likely behind one policy
   layer. Design the path through the Scufris daemon and HUD attention
   states, quiet rules, and audit. Implementation comes after the first
   slices.

## The library

Round-2 update: the library's home moves out of the-den into the
curator tool's own store. The design below - division of labor,
storage, retrieval, ingestion, course builder - transfers to the
curator unchanged.

It starts empty; collection comes before retrieval.

Purpose: a place where research material accumulates and stays useful.
Concrete first use cases:

- nova-protocol: documents about similar games, art for inspiration,
  design references.
- Future game ideas: horror, RTS, whatever appears next.
- Hardware: how to give Scufris a speaker and a body.
- Random knowledge pieces worth keeping.

Division of labor (decided):

- Scufris is the main capture path. It goes to the web, fetches, judges,
  and downloads. Capture is a conversation: "collect resources about X
  for nova-protocol."
- The library CLI is the final persistence step, in the today mold. Its
  write surface is narrow and deterministic: take a blob that already
  exists locally (a download), store it, and create its manifest. It
  never fetches from the network. Its read surface is richer: list,
  show, search, and resolve items for Scufris and widgets.

Storage (decided): tracked manifests, ignored blobs. Notes, metadata,
and manifests with content hashes live in Git and stay canonical. Media
lives in ignored library directories that manifests reference. Blob
backup happens outside Git (rsync, restic, or similar); the research
must state the backup story explicitly.

Retrieval (later, once content exists): a derived, rebuildable index.
Lexical first, embeddings where a concrete use case justifies them.
Models run local-first - whisper-cpp and Piper set the precedent - and a
cloud model may be justified per modality with the tradeoff stated.

Multimodal ingestion (research direction): turn web media into useful
learning material. Example pipeline: a video becomes a word:timestamp
transcript plus its relevant frames, so the content can be consumed as
text and images without replaying the video.

### Course builder (vision pillar, build later)

A tool that uses Scufris to compile online references, books, and notes
into an interactive HTML course on a topic: pages, tests, small graphs,
widgets to play with while learning. It consumes library content, so it
shapes what the library must store: provenance, extracts, media,
citations. This research designs a UX flow and a designs/ HTML concept
for it; the build comes after library capture exists.

## Principles that stand

- Local-first, plain files, human-editable in Neovim. Git history is the
  backup story for canonical text.
- One owning tool per structured area. Everything else reads through its
  contract. Scufris and widgets never parse the same files independently.
- Observation is separate from presentation. Scufris learns facts from
  CLIs, then optionally opens a widget. A widget is never Scufris's only
  source of a fact.
- Imported content is data, not authority. Saved pages and transcripts
  must not become instructions. Provenance and citation matter from the
  first library design.
- Mutations that touch the outside world or the system need explicit
  approval. The NixOS lifecycle is the deployment gate.

## Expected research outputs

Full suite, in this task directory, refocused on the vision above.

- `INVENTORY.md` - current behavior and boundaries of scufris2,
  dashboardd, today, the-den, nix.dotfiles, plus the HUD task interface.
  Much of this is surveyed above; make it citable with file paths.
- `RESEARCH.md` - existing tools and patterns worth reusing, judged
  against real workflows. Calendar tooling around `.ics`, web-content
  capture and archival, local search, widget webview options. Depth over
  link lists.
- `ARCHITECTURE.md` - at least two credible options for the Scufris x
  today integration and the reference-widget path, with a recommendation.
  Data ownership, tool interfaces, trust boundaries, failure behavior,
  NixOS packaging.
- `UX.md` - end-to-end flows: ask about tomorrow, capture a task by
  voice, weight trend with graph, "look what I found", morning briefing,
  proactive finding with source shown. What is said, what opens, what the
  HUD states show.
- `designs/` - self-contained HTML concepts where visual exploration
  helps: briefing, reference viewer, agenda.
- `ROADMAP.md` - staged work starting from the decided first slice, each
  stage with value, dependencies, risks, and a cheap verification.
- `IDEAS.md` - organized future ideas within the local-first and
  controlled-action constraints.

## Final recommendation must answer

1. The exact native-tool surface Scufris gets over the today contract,
   and what answer-plus-widget looks like as one response.
2. What the reference-showing surface is per content type (webview
   widget, real browser, library viewer) and what it needs from
   dashboardd - without Scufris-specific hooks inside dashboardd.
3. The library's first increment: manifest format, storage layout, CLI
   read and write surface, and the Scufris capture flow.
4. Whether calendar `.ics` enters now or waits for a concrete need.
5. How proactive contact will work when built, and what minimal hooks the
   first slices should leave for it.
6. What explicitly stays unbuilt.
7. The first demonstrable end-to-end prototype and its verification.

## Decisions from pairing

- Scufris x today uses native Pi tools wrapping `today --json`, not the
  shipped skill alone. Pi is the harness.
- References: both the webview-widget path and the real browser stay
  open; choose per content type.
- Proactive triggers: all sources are candidates (timers, watcher,
  worker events); research proposes the policy layer.
- Library before retrieval: capture first, since content is at zero.
- Library capture: Scufris fetches; the library CLI only persists local
  blobs and writes manifests. CLI leans read-side otherwise.
- Library storage: tracked manifests with hashes, ignored blobs, backup
  outside Git.
- Models: local-first; cloud per modality only when justified.
- Course builder: vision pillar with UX flow and HTML concept now,
  build after library capture exists.

## Decisions from pairing, round 2 (2026-08-24, post-research)

These supersede earlier lines where they conflict.

- Sequence: the scufris-desktop HUD comes first, then today CLI and
  widget improvement, then Scufris x today, then the curator. Today
  can wait until tomorrow.
- The one-place invariant is the today CLI, not the daily Markdown
  file. Per-domain storage is negotiable: Markdown where prose wins,
  JSON or SQLite where structure wins, wrapped external CLIs where one
  already fits. The single-spine versus per-domain-stores decision is
  deferred until the CLI and widget work makes it obvious.
- the-den is personal notes, journal, and tracking only. The library
  and learning material move out of the-den into the curator: a
  separate CLI-first tool (SDK-shaped, a platform could grow later)
  with its own store. Scufris asks it to research a topic from seed
  references; it produces curated artifacts - research brief with
  citations, explainer, or course with checks. The capture, ingestion,
  retrieval, and reference-display research transfers to it wholesale.
- scufris-desktop is its own Tauri app living in the scufris2 repo as
  a `desktop/` cargo workspace (scufris2 is a Pi package; npm there is
  LSP tooling, not a build system), shipped as a separate flake
  package. V1 is the pill: an always-on-top Siri-style overlay (the
  desktop stays usable while it listens), transcript review, tray.
  V2 adds the full-screen conversation mode. Same stack as
  dashboardd-desktop on purpose: v3 embeds dashboardd-runtime as a
  third host and takes over primary widget hosting, demoting
  dashboardd-desktop to a manual tool. dashboardd itself stays
  Scufris-agnostic. STT is a configurable endpoint with an optional
  bundled whisper-server (the Piper precedent), so scufris works out
  of the box on any Nix system. No window manager gets built; i3
  remains the app-level answer.
- Widget iteration runs against a locally started dashboardd with
  DASHBOARDD_WIDGET_PATH pointed at repo-built bundles; release-time
  skew is avoided by bumping related pins in one nix.dotfiles commit
  (the switch is atomic).
- Morning briefing delivery: conversation answer plus opened widgets
  now; the HUD full-screen mode becomes the briefing canvas later; no
  briefing page-builder tool.

## Open questions for pairing

- Manifest format: one file per item? Minimum fields (source URL, hash,
  capture date, modality, topics, license)? Research proposes.
- Where the library CLI lives: a new repo in the today mold, or inside
  an existing one.
- How Scufris cites library items in answers, and how a citation opens
  the right viewer.
- Whether the first library increment ships any retrieval at all, or
  pure capture with `rg` over manifests as interim search.
- Should the HUD control channel and dashboardd's SSE stream converge
  into one attention path later, or stay separate channels?
