# Inventory: verified current state

Verified 2026-08-23 by parallel source surveys of the five repositories.
Versions are the tags pinned in `~/personal/nix.dotfiles/flake.nix`. This
document is the factual baseline for the research in this task; NOTES.md
holds the vision.

## scufris2 (v0.2.0, `~/personal/scufris2`)

A Pi package. Pi (`@earendil-works/pi-coding-agent` and friends, pinned
0.84.2 in `package.json`; Nix input `github:lukasl-dev/pi.nix`) owns the
foreground conversation. `package.json` registers four extension entry
points and a skills directory under the `pi` key.

Extensions (`extensions/scufris/`):

- `workflow/` - identity and system prompt, delegated job engine
  (spawn, steer, stop, land), worker event watching by `fs.watch`,
  Quick Review. Files: `identity.ts`, `orchestration.ts`,
  `worker-report.ts`, `walkthrough.ts`.
- `voice/` - `response.ts` always; `speech.ts` only when
  `SCUFRIS_VOICE_AVAILABLE=1`. Speech output pipes text through
  `tools/voice/scufris-speak` (Piper, `en_US-lessac-medium`, played via
  `pw-play`). `/speech on|off|once|replay`.
- `calm.ts` - hides thinking and tool noise in the TUI. `/calm`.
- `dashboard/` - widget control. At `session_start` it runs `discover`
  through the adapter `tools/dashboard/scufris-dashboard` (Python,
  shells to `dashboardctl`, 5 s timeout, 64 KiB bounds) and generates
  native tools from the catalog.
- `shared/runtime.ts` - `runPrivateHelper`: the one-subprocess,
  one-JSON-envelope protocol every extension uses for helpers.

Native tools: `scufris_final_response` (only user-visible output path);
`scufris_projects`, `scufris_project_context`, `scufris_job_spawn`,
`scufris_job_list`, `scufris_job_inspect`, `scufris_job_send`,
`scufris_job_stop`, `scufris_job_land`, `scufris_job_quick_review`,
`scufris_job_plannotator_review`; `scufris_widget_open`,
`scufris_widget_update`, `scufris_widget_list`, `scufris_widget_focus`,
`scufris_widget_close`; worker-only `scufris_report`.

Session state is persisted as custom Pi session entries (speech, wake,
calm, responses, review outcomes) and restored on start. The dashboard
extension is the only poller (1 s `setInterval` while surfaces are
owned, to notice externally closed windows); everything else is
notification driven and `tests/structure.test.ts` enforces it for
`orchestration.ts`.

Voice input is deliberately not Scufris code: docs state speech-to-text
is Pi configuration (`docs/src/overview.md`, `docs/src/guide/using.md`).

Gaps relevant to this task: zero references to `today` or the-den
anywhere in the repo; no linkage between `scufris_final_response` and
the widget tools (no packaged answer-plus-widget flow); the dashboard
skill (`skills/dashboard/SKILL.md`) forbids reading domain state back
through widgets, which is correct but leaves observation with no den
data source.

## today (v0.4.0, `~/personal/today`)

Stdlib-only Python 3.13 CLI. Per its `AGENTS.md`, the only reader and
writer of the-den journal Markdown.

CLI (`today/cli.py`): global `--den`, `--date`/`-N`, `--no-edit`; bare
`today` opens `$EDITOR`. Subcommands, all with `--json`: `path`,
`create`, `show`, `task add|done|rm`, `habit toggle|list`,
`weight [value] [--days N]`, `macros` (day totals, `add`, `rm`,
`query`, `calculate`, `insert`, food database from `$MACROS_DATABASE`),
`note add|list|edit|rm`, `upcoming` (scans future dated files for
incomplete tasks).

Editing model (`today/model.py`, `today/edit.py`): parses five known H3
sections (Tasks, Habits, Macros, Weight, Notes); mutations splice bytes
only inside the target section region and preserve everything else
exactly; atomic writes (tempfile, fsync, rename); optimistic
concurrency via file revision (`inode:mtime:size`) with
`RevisionConflict` on stale writes; at-most-once entry creation under
`Daily/.today.lock`.

Ships `skills/today/SKILL.md`, exported as `flake.skills.today` for
agent consumption. Ships a complete Dashboardd widget
(`widget/widget.toml`, schema v3): six variants (tasks, habits, macros,
weight, notes, upcoming), TypeScript frontend on `@dashboardd/widget-sdk`,
Python backend `today-dashboardd-widget` speaking the JSON Lines
protocol with revision-conflict handling. Flake builds
`packages.today`, `packages.dashboardd-widget`, overlay `pkgs.today`;
checks include a live widget-catalog test against a real dashboardd.

`README.md` names the missing piece: Scufris wrapping the subcommands
is planned and not done. No open tasks; no TODO markers.

This CLI is the template for future den domain tools: JSON contract,
human edits preserved, its own widget and skill, released and pinned.

## dashboardd (v0.2.0, `~/personal/dashboardd`)

Two hosts embed one runtime crate:

- `dashboardd` (server): Axum HTTP on 127.0.0.1, REST plus SSE
  (`/api/v1/events`), Swagger at `/docs`, dashboard SPA; widget
  frontends are ES modules mounted in Shadow DOM.
- `dashboardd-desktop`: Tauri tray app; each surface is a real
  `WebviewWindow` (WebKitGTK) loading the widget frontend through a
  custom `dashboardd-widget://` scheme
  (`crates/dashboardd-desktop/src/service.rs`). Controlled over a Unix
  socket (`$XDG_RUNTIME_DIR/dashboardd-desktop.sock`), JSON lines,
  protocol version 2, audit logged.

`dashboardctl` commands: `discover`, `open <widget> --variant
[--options] [--inputs] [--presentation focus|tile]`, `update`, `list`,
`focus`, `close`, `quit`. Stable error codes; exit codes 0/1/2.

Widget model (`widget.toml` schema v3): variants with size and focus
flags, typed options, typed input and output ports. Backends are one
process per instance speaking JSON Lines on stdio with ping health.
Feedback channels exist: backend update events, frontend-to-backend
messages, page-local published outputs, persisted shared state with
revisions, all observable on the SSE stream.

Shipped widgets: cpu, memory, disk, network, claude-usage, codex-usage,
projects, tatr-tasks; the today widget arrives from the today flake.

Facts that matter here: no widget can display an arbitrary URL and the
SPA has no iframe support; dashboardd contains zero Scufris awareness -
a deliberate boundary to keep. Home Manager modules define user
services for both hosts with `widgetPackages` composition.

## the-den (`~/personal/the-den`)

Obsidian-style vault, private Git repo, 8.1 MB.

- `Daily/`: 1,140 files, 2023-04-15 through today, named
  `YYYY-MM-DD-Weekday.md`. H1 date title, then H3 sections Tasks
  (checkboxes), Habits (checkboxes), Macros (CSV `what,protein,carbs,
  fat`), Weight (`NN.N kg`), Notes (timestamped `#### HH:MM - topic`
  entries with prose).
- `Notes/`: annual goal pages (2023-2026), `flow.md` design doc,
  `Videos/` with nine saved video notes.
- `Templates/daily.md` seeds new entries. `tasks/` is empty.
- `.gitignore`: Obsidian workspace cache, `.trash/`,
  `Daily/.today.lock`.

No calendar, library, or attachments areas exist yet. There is no
Workout section in the template and no workout tooling anywhere; if
workout content appears it stays in daily Markdown (decided).

## nix.dotfiles (`~/personal/nix.dotfiles`)

All personal tools are flake inputs pinned to release tags: `today`
v0.4.0, `dashboardd` v0.2.0, `scufris` (repo scufris2) v0.2.0, `tatr`
v2.0.3, `pi` (lukasl-dev/pi.nix), with `follows` wiring for shared
inputs.

- `today`: overlay provides `pkgs.today`; `DEN_PATH=/home/alex/
  personal/the-den` set in `home/modules/scripts/default.nix`; the
  today widget is wired into both dashboardd hosts' `widgetPackages`
  (`home/modules/dashboardd/default.nix`).
- `dashboardd`: Home Manager module, server on port 8000 (opened to
  LAN), desktop floats under i3 via window criteria.
- `scufris`: `programs.scufris` with voice and popup enabled; popup is
  a Mod4+s Kitty scratchpad (`home/modules/i3/scufris-popup.nix`);
  cross-repo build checks in `home/modules/agents/checks.nix`.
- Voice input: `whisper-server` systemd user service
  (whisper-cpp-vulkan, large-v3-turbo, loopback 127.0.0.1:10301),
  consumed by the Pi `voice-stt` extension
  (`home/modules/agents/pi-extensions/voice-stt/module.nix`); capture
  via ffmpeg and PipeWire.
- Desktop: i3 (Hyprland also available), rofi, kitty, dunst, PipeWire.
- Update ritual (manual, no automation): edit tag in `flake.nix`,
  `nix flake update <input>`, `nix flake check`, then
  `sudo nixos-rebuild switch --flake .#nixos` or `home-manager switch
  --flake .#alex`. This is the deployment gate.
- `nova-protocol` is not packaged here.

## HUD companion (task `20260822-132001`, open, priority 100)

Planned `scufris-desktop`: Super+D bottom-center voice HUD with
immediate-send and review flows, tray icon with health states, local
Whisper transcription, focus restoration. The popup Pi process (the
Scufris daemon) stays the single conversation owner; the companion
submits accepted transcripts over a narrow same-user control channel
and receives bounded health and lifecycle state. HUD states: listening,
transcribing, working, speaking, attention, error. Wake-word activation
is future work that reuses the same start action. The framework and
exact protocol are to be selected before implementation.

## What works end to end today

Voice loop: Whisper STT in (Pi), conversation (Pi plus Scufris), Piper
TTS out, popup on Mod4+s. Widget control: Scufris can discover, open,
update, focus, and close any installed widget, including the six today
variants. Delegated jobs, review flows, and the release-pin-rebuild
deployment gate all function.

What does not exist: any Scufris observation of den data (the today
integration), any library or capture tooling, any reference-display
surface, any proactive contact path.
