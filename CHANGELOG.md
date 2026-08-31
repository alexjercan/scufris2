# Changelog

All notable user-facing changes to Scufris.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html). Releases are
immutable `vX.Y.Z` tags; see [RELEASE.md](RELEASE.md) for the process.

## [Unreleased]

### Added

- The-den journal is read and written inside Scufris. `tools/den/den.py` is the
  whole format - days, backlog, and the food database - compiled into the
  desktop's `den` backend and run by the new `scufris-den` command.
- `scufris-den`, a non-interactive command line over the journal, plus a `den`
  agent skill, so the agent reads and writes the day by the same rules a panel
  does.
- A backlog: ideas with no day yet, kept in `Backlog.md` at the top of the den.
  The agenda panel keeps one and pulls one onto the day that is showing.
- Restant on the agenda: what was left undone on earlier days, bounded by a
  horizon of 60 days by default and marked on the month with everything else.
- A workout log. A `### Workout` section holds `split,exercise,weight,reps`
  rows, the macros panel logs and shows the day's sets with its total volume,
  and splits and movements are offered out of what was trained before. An entry
  written before the section existed gains it on the first write.
- A widget backend may name libraries in a `prelude` file, which `build.rs`
  compiles in ahead of it.

### Changed

- The agenda, macros, and notes panels no longer run the `today` command. They
  read the journal in their own process, so a panel no longer depends on what
  is on the path and a click costs a read rather than a process.
- The macros panel shows one whole day - eaten, weighed, and lifted - and is
  taller for it. The agenda holds its two new lists in the frame it already
  had, which is the tallest a pair of panels on one edge can be.

### Removed

- **(breaking)** `programs.scufris.desktop.widgets.todayCommand` and its
  `desktop.todayCommand` alias, with `SCUFRIS_TODAY_COMMAND`. There is no
  command to point at. `desktop.widgets.denPath` and `DEN_PATH` are unchanged
  and still say where the journal is.

## [2.0.0] - 2026-08-30

### Added

- Managed attachments across the service, agent, desktop, and iOS. Messages
  carry bounded opaque references while the service owns private durable bytes,
  canonical metadata, quotas, retention, and replay.
- Authenticated upload, download, HEAD, and single-range attachment transfer on
  the existing private surface gateway.
- Native document and photo selection, protected downloads, Save to Files, and
  inline image and video thumbnails on iOS.
- Native desktop file selection, atomic private saves, inline raster images,
  extracted video thumbnails, and safe media preview behavior.
- An orchestrator-only `store_attachment(path)` tool for delivering generated
  files through canonical responses.
- Staging-only offline Swagger and OpenAPI documentation for the remote gateway.

### Changed

- Surface protocol v5 replaces v4 without negotiation. Service, gateway, agent,
  desktop, and iOS must be updated together.
- Desktop and iOS attachment presentation uses prominent Save actions and
  thumbnail-driven native previews. iOS also has larger accented controls and
  interactive keyboard dismissal.

### Fixed

- Empty attachment arrays omitted by canonical serialization now decode as
  empty arrays at the agent boundary.
- Bot-generated videos retain recognized media types. Existing opaque video
  descriptors use a conservative filename-extension fallback.
- Desktop video thumbnails avoid WebKit custom-scheme media playback and its
  blank conversation-window failure.

## [1.1.1] - 2026-08-30

### Fixed

- Starting or restarting the background service now also starts its enabled
  remote-surface gateway, so Home Manager activation cannot leave the private
  API route without a loopback backend.

## [1.1.0] - 2026-08-30

### Added

- A native SwiftUI iOS surface delivered through TestFlight. It stores its
  stable identity, WSS URL, and bearer token in Keychain, reconnects with
  canonical replay, and submits text to the shared conversation.
- An optional loopback-only `scufris-surface-gateway` serves an authenticated
  HTTP and WebSocket API. It bridges strict protocol-v4 surfaces and forwards
  bounded private audio transcription to host `ai-tools-api`. Its Home Manager
  option also owns a declarative Tailscale Serve route that provides private TLS
  without exposing agent, control, or inference listeners.
- iOS hold-to-dictate records up to 60 seconds locally, transcribes on the
  private host, and returns text to the editable composer for explicit send or
  discard.

### Changed

- Home Manager now treats `programs.scufris.agent` as the core interactive
  launcher that the optional background service also runs. API process
  ownership and the shared machine endpoint are top-level under `aiToolsApi`,
  while the desktop can override its consumer base URL.
- Desktop speech exposes model and voice. Transcription exposes model and
  language and always derives its route from the desktop API base URL.
- Desktop controls are named `popupKey`, `backgroundKey`, and `abortKey`.
  Terminal integration is `terminalCommand`, and journal-backed settings are
  grouped under `desktop.widgets`.
- `staging up` and `staging backend` now start an isolated external-surface
  gateway. They can own an exact temporary Tailscale Serve path and remove only
  that path during teardown.
- The iOS surface now follows the dark terminal interaction study:
  route and state header, speaker-column transcript, inline details, and a
  compact bottom composer. Its next TestFlight marketing version is 1.1.0.
- Desktop and iOS conversation views show one transient `thinking...` row while
  the service reports working. It clears on the final response or another state
  and never enters canonical replay.

### Removed

- The Home Manager transcription endpoint override. Compatible routes are
  derived from `desktop.aiToolsApi.baseUrl` instead.

## [0.6.0] - 2026-08-30

### Added

- Strict protocol v4 with separate surface, agent, and control sockets. Several
  desktop surfaces can share one canonical 200-message conversation, reconnect
  with stable identities, and receive replay without repeating speech or widget
  effects.
- Named split staging commands: `nix run .#staging -- backend` and
  `nix run .#staging -- frontend NAME`. Each frontend has private state,
  identity, data, command socket, lock, and teardown.
- Structured backend and desktop logs. INFO records major lifecycle events;
  DEBUG adds typed payload and routing detail for local diagnosis.
- A pinned `ai-tools-api` v0.1.1 package and app. The Home Manager module reuses
  an enabled shared provider, can consume an explicit external endpoint, or can
  run one hardened fallback service.

### Changed

- Protocol v4 replaces protocol v3 outright. The service owns canonical
  conversation history, the agent emits atomic final responses, and only the
  live originating frontend performs speech and widget presentation.
- Speech inference now uses the bounded OpenAI-compatible transcription and
  speech routes on `ai-tools-api` port 10300. Scufris retains recording,
  validated WAV playback, mute, and cancellation.
- Home Manager now groups the supervised agent under `service.agent`, desktop
  speech under `desktop.speech`, and speech-to-text under
  `desktop.transcription`. The former option paths remain deprecated aliases
  for one release.
- Source and package layout now follows agent, host, shared protocol, and
  surface ownership boundaries.
- The isolated staging stack still runs with `nix run .#staging -- up`; it uses
  a deployed shared API by default and supports explicit managed API mode.

### Removed

- Direct Scufris ownership of Whisper, Piper, their models, patches, packages,
  and user services. One `ai-tools-api` deployment now owns inference per
  machine.
- Protocol v3, RPC prompt ingress, dynamic model-defined widgets, spoken-event
  routing, response detail artifacts, and their compatibility paths.

## [0.5.0] - 2026-08-29

### Added

- Ambient tray notices for unattended jobs. A blocked job paints the tray
  wisteria and a failed job paints it red until that job reports progress or
  completion. Notices are kept independently by job identifier in the service,
  so one job cannot clear another and a companion that connects later receives
  everything still waiting.
- The conversation window. The pill says what Scufris is doing and could never
  say what was said; this draws it, and gives you a line to type on. Click the
  pill to put the window up, and click it again to put it away - the pill's one
  pointer gesture. Bind `scufris-ctl hud` to a key for the same thing.
  `Enter` sends, `Shift+Enter` starts a new line, `Escape` closes it. The tray
  shows it too, on a left click and from the menu. It holds the same last two
  hundred lines the service keeps, so everything said is there whoever said it
  and however it was sent. `scufris-ctl debug` in a terminal is still the
  deeper tool and is not a fallback for this: it is a whole Pi session, and the
  window is the last few lines and a place to answer them.
- `scufris-service`, the headless half of Scufris. It supervises one
  `pi --mode rpc` agent, owns the session directory, and serves one socket that
  every surface connects to. It builds and runs with no graphical dependency,
  so a machine with no display keeps the conversation. Off by default;
  `programs.scufris.service.enable` gives it a systemd user unit of its own.
  See the [background service](docs/src/dev/service.md) chapter.
- `scufris-ctl send`, `state`, `watch`, `abort` and `debug`, which reach the
  background service from any terminal. `debug` takes the agent away and opens
  its session where you are; closing the terminal gives it back, so there is no
  way to be left detached with nothing to put it back. `scufris-ctl` is now its
  own package, installed by whichever half of Scufris you enable.
- The service says so when the agent it started never connects back to it. Such
  an agent still holds a conversation, because the service reads Pi's own
  events, but it can report nothing it said, nothing to speak, and no widget,
  which looks exactly like a broken speaker. The usual cause is a `scufris` from
  somewhere else on `PATH`, built without the service extension, so the warning
  names the binary it started.

- Native widgets. Scufris can open a small panel on the desktop while it
  answers: an exhibit on a shelf above the pill, which ages out on its own, or
  an instrument in one of four edge slots when you ask to keep it. A widget
  window never takes the keyboard. The `widgets` extension registers
  `scufris_widget_open`, `scufris_widget_update`, `scufris_widget_close`, and
  `scufris_widget_clear`, typed from what the companion says it has installed.
  Four widgets ship with it: `cpu`, `timer`, `claude`, and `codex`. See the
  [widgets](docs/src/dev/widgets.md) chapter.
- Exhibits age out on their own. A panel the conversation has moved past dims
  and retires a minute later, and an update or the pointer over it brings it
  back. Clocks stop while Scufris speaks and while you are reading the panel.
  Instruments and pinned panels are yours and never age.
- Widgets go away with the pill and come back with it. Putting the pill down
  takes the whole layer off the screen with their state and their remaining
  time intact. Panels you pinned and instruments stay where they are, because
  they are yours rather than the runtime's.
- A widget Scufris opened follows you between workspaces. Pinning it brings it
  down onto the workspace you are on and parks it in a screen-edge slot of its
  own, so nothing the shelf does afterwards lands on top of it. A pin with no
  free slot says so on the panel instead of doing nothing.
- Widgets that show live numbers, and the `cpu` widget on the first of them. It
  draws the last minute of processor load as a graph, with the package
  temperature beside it - in the warning colour once it is hot - and the memory
  in use and the load average under it. A widget can name a backend, a small
  program that reports readings, and two panels asking for the same numbers
  share one process. A backend that goes quiet says so on the panel; one that
  dies turns the frame red and offers a restart tick, rather than leaving a
  frozen number that looks live. Nothing is left running when the last panel
  closes or when the companion exits.
- Widgets you can act on, and the `timer` widget on the first of them. A panel's
  own buttons write back to its backend, which answers with the refreshed
  reading, so what the panel shows is always what the backend knows. Ask for a
  timer and it counts down on the desktop, with ticks to pause, resume, add a
  minute, and start over. Two timers of the same length are two timers.
- Put a widget up yourself. The tray menu offers the widgets that fill
  themselves, and one you open from there is yours: it goes in an edge slot,
  stays until you close it, and Scufris is not told about it.
- The `claude` and `codex` widgets, which say how much of each subscription is
  spent. Every usage window is a meter, the one closest to its limit is the
  headline, and the panel says how long until it starts over. They read the
  token the vendor's own CLI already keeps on this machine, so there is nothing
  to sign in to and nothing to configure, and a machine that never signed in
  gets a panel that says so rather than a stale number. The `rfr` tick asks
  again without waiting out the poll.
- Panels are bigger and their type is larger. A widget sits on the desktop
  beside the work rather than in it, and is read from across the room; the type
  scale went up a step and every panel went up with it, because type that grew
  inside a window that did not would only have less room to say the same thing.
- `SCUFRIS_WIDGET_PATH` names extra widget roots, separated the way `PATH` is,
  so another project can ship a widget for the desktop. Widgets that shipped
  with Scufris always win, and one that will not install is reported in the log
  rather than stopping the companion.
- `scufris-ctl open`, which puts the pill up from outside its window, so a
  window manager binding can be the thing that opens it. See
  [Using Scufris](docs/src/guide/using.md).
- The pill answers `Super+Escape` while it is on screen, built from whatever
  modifier your activation hotkey uses. It cancels a take, and it puts a
  resting pill away without opening the microphone on the way.
- `scufris_conversation`, so Scufris can show the conversation window itself.
  Ask to see the conversation and it opens the window; ask it to put the window
  away and it does. It shows and closes rather than toggling, because it cannot
  see your screen and a toggle would leave it unable to say which of the two it
  had just done.
- `Super+Delete` stops Scufris, built the same way and grabbed on the same
  terms. It cuts what is being spoken and ends the run, and it changes nothing
  else: a transcript you are still editing stays where it is, and the
  conversation keeps everything said so far. With nothing running it does
  nothing. `scufris-ctl abort` is the same verb from a terminal.
- `programs.scufris.desktop.cancelKey` and `stopKey` name those two keys
  yourself. Deriving them from the hotkey is what ships, so most deployments
  set neither; `"none"` takes a key off the companion entirely, which is the
  answer where your desktop already means something by it.
- `packages.scufris-speak`, the synthesiser the companion runs. It binds the
  pinned Piper package, model, and configuration, so the voice is a property of
  the package rather than a run-time setting.
- `SCUFRIS_RUNTIME_DIR` names the socket directory outright, used as named with
  no `scufris` below it. The service, the companion, `scufris-ctl` and the
  agent's own service extension resolve their sockets through it, so one export
  moves a whole stack together and none of them can end up in another Scufris's
  conversation. A socket named outright still outranks it, and nothing sets it
  in an ordinary session.
- `nix run .#staging -- up` runs this source tree's Scufris beside the deployed
  one: its own sockets, state, sessions, and `Super+G`, against a disposable
  root under `/tmp`. It stays in the foreground and Ctrl+C stops both halves.
  It speaks with the packaged `scufris-speak`, so staging has the voice the
  deployment would have, and says on start when it could find no synthesiser
  rather than leaving a missing voice to look like a broken one. See the
  [staging](docs/src/dev/staging.md) chapter.
- Your journal on the desktop, as three panels over the `today` command:
  `agenda` is a month to pick a day from and then that day's habits, tasks and
  what follows it; `macros` is the day's calories and food with a month of
  weight behind them; `notes` is the day's notes. They write as well as read.
  Tick a habit or a task to mark it done, click a weight to log one, click a
  note to rewrite it, and use the `+` ticks to add a task, a food or a note.
  Logging a food offers your database as you type the name. Everything lands on
  the day the panel is showing, and goes through `today`, so a habit ticked
  here and one ticked in your editor are the same habit. They need
  `programs.scufris.desktop.todayCommand`, and a food needs
  `programs.scufris.desktop.macrosDatabase` unless your database is where
  `today` looks by default.
- A panel that needs words asks for them in a small box of its own, over the
  panel that asked. `Enter` saves, `Escape` closes it with nothing written, and
  the keyboard goes back where it was. A panel still never takes the keyboard
  itself.

### Changed

- Two panels on one screen side no longer stand on each other. The second place
  on a side used to be measured halfway down the screen, whatever was already
  there, so any panel taller than a quarter of the screen overlapped the one
  above it. The two places now hang from opposite ends of the side. The shelf
  above the pill holds a lane per panel for the same reason, wide enough for
  the widest one that ships.

- Scufris delegates literally. `.scufris.toml` is a menu of agent types, not a
  workflow: one `conventions` table for what Scufris infers when you do not
  say, and one `agents.<name>` table per agent, each with a `description` of
  what it is for and `keywords` for how it is run. Ask to implement something
  and Scufris runs the work agent and stops. Ask to implement and then review
  and it runs both, in that order. It starts no agent because the project
  declares one, and it queues no follow-on work of its own. An explicit
  instruction such as "do it directly on master" wins over a convention, and an
  agent name Scufris has never seen is delegated to like any other. A later
  round of an agent already running steers that job rather than starting a
  second one, so a reviewer keeps what it already accepted instead of finding
  new fault every round. The retired `preferences` shape is refused with a
  diagnostic rather than half-read; see the
  [jobs chapter](docs/src/dev/jobs.md) for the file to write instead.
- Scufris is a background service with clients now, which is the whole shape of
  this release. `scufris-service` owns the conversation, the session, and the
  socket; the Pi agent, the desktop companion, and `scufris-ctl` are all
  clients of it. There is no terminal that owns the conversation any more, so
  putting the pill away, closing the terminal, or a companion crash leave the
  conversation exactly where it was, and a machine with no display still has
  one.
- `programs.scufris.desktop.enable` requires `programs.scufris.service.enable`.
  The tray's restart hook restarts `scufris-service.service`.
- The tray's "Open chat" is now "Open in terminal", under the new "Show
  conversation" entry, and a left click on the tray icon shows the conversation
  window rather than opening a terminal. `chatCommand` is unchanged and still
  optional; the terminal is a different tool, not a fallback.
- Control protocol version 3, which replaces version 2 outright. It adds the
  `agent` role, so the Pi process reports what it said, the paragraph it wants
  spoken, and the widgets it asks for, and it carries stable refusal codes a
  caller branches on. There is no conversion from version 2: the companion, the
  service, and the Scufris package must be updated together.
- Speech is the companion's, all of it. It owns the speaker, so it owns the
  mute: "Mute Scufris" in the tray silences Scufris without touching the
  conversation. Nothing in the agent's process tree makes sound and nothing in
  it decides to; every answer is one prose paragraph whatever is listening,
  which is the shape of the assistant rather than a speech setting. A session
  with no companion is silent, and so is a companion with no synthesiser, which
  is the one thing enabling `voice` now does.
- `Super+D` is the one key of the take. Press it once and the pill rises and
  the microphone opens; press it again and the take stops and what you said
  arrives in a textbox above the pill. The textbox is an ordinary focused
  window, so `Enter`, `Escape`, and every editing key are its own and work
  wherever you are. The pill is an indicator and never takes the keyboard.

### Removed

- The Kitty popup. `programs.scufris.voice.popup.*` and the
  `scufris-popup.service` unit are gone; use `programs.scufris.service.enable`
  and reach the conversation with `scufris-ctl debug`. Nothing is migrated: a
  configuration that set the popup options fails to evaluate.
- Control protocol version 2, the `desktop` Pi extension that served it,
  `SCUFRIS_DAEMON`, and `tools/desktop/scufris-socket-lock`.
- Dashboardd widget control. The `dashboard` extension, its skill, the
  `scufris-dashboard` helper, the `dashboardd` flake input, and the
  `programs.scufris.dashboard.*` options are gone. Widgets return as a native
  runtime inside the desktop companion.
- The window manager binding mode, and with it the `accept` and `cancel` verbs
  of `scufris-ctl` and `programs.scufris.desktop.modeCommand`. The textbox holds
  the keyboard itself, so there is nothing left for a binding mode to route. A
  configuration that sets `modeCommand` fails to evaluate; nothing is migrated.
- `Enter` while the microphone is open. One take is one key: `Super+D` stops
  it, and the words are sent from the textbox.
- The speech mode, and with it `/speech`, `SCUFRIS_SPEECH`, and
  `SCUFRIS_VOICE_AVAILABLE`. Whether Scufris makes a sound was a switch kept in
  the session and seeded from a variable on a process that owns no speaker. It
  is the tray's now. A configuration that sets either variable is ignored;
  nothing is migrated, so a session that recorded `/speech off` is audible
  again until the tray is told otherwise.
- The voice build variants: the `scufris-voice` package and app, the
  `voice-resources` package, and `npm run dev:voice`. They existed to ship a
  speech module and set a variable for it, and both are gone. There is one
  launcher, and it is the one that was always silent.

## [0.4.0] - 2026-08-25

### Added

- `scufris-desktop`, the voice pill and tray companion. `Super+D` opens a
  bottom-center pill and starts recording. `Enter` transcribes and sends,
  `Escape` discards, and the accelerator again opens the transcript for
  editing. The tray shows the assistant state and can open the chat, start
  voice input, restart the backend, and quit. See the
  [desktop companion](docs/src/dev/desktop.md) chapter.
- `packages.scufris-desktop`, a separate Linux flake output. Nothing else
  pulls Tauri or WebKitGTK into its closure, which a closure check enforces.
- `programs.scufris.desktop` Home Manager options, among them `enable`,
  `hotkey`, `chatCommand`, and `stt`. The module defines the
  `scufris-desktop.service` user service and a generated backend restart hook.
- A bundled loopback `whisper-server` with a pinned model on
  `127.0.0.1:10302`, used when `desktop.stt.endpoint` is not set, so voice
  input works on any Nix system.
- Control protocol v1 on `$XDG_RUNTIME_DIR/scufris/daemon.sock`. The popup Pi
  process serves it. Submissions are acknowledged against the session, and an
  unacknowledged transcript is kept and reported as uncertain instead of being
  sent again.
- The `desktop` Pi extension, which serves that socket in the daemon role and
  reports one assistant state: idle, working, speaking, attention, or error.

### Changed

- Independent review honors the harness and model configured for the project.
- Quick Review runs as a separate Pi RPC agent that loads the standalone npm
  extension. The in-repository walkthrough implementation is removed.
- The default Pi package comes from the `llm-agents.nix` input.
- `nix/checks/` replaces the single `nix/checks.nix` file, with one group per
  check concern.

## [0.3.0] - 2026-08-24

### Added

- Project workflow preferences in `.scufris.toml`: task tracking, isolated
  Sprout worktrees, the implementation harness and model, review, and the
  landing gate.
- Complete job inspection with `scripts/scufris-jobs`.
- Explicit `/wake` and `/calm` controls, restored with the session.
- The user and developer guides in the mdBook manual.

### Changed

- Delegated jobs never block the foreground conversation. Workers report
  `working`, `blocked`, `done`, and `failed` events.
- Scufris answers as a prose-only orchestrator. Optional detail is a private
  artifact opened with `/detail <id>`.
- Quick Review follows pull request review semantics.

### Fixed

- Foreground workflow acknowledgments, response termination, delegation
  routing, and speech ordering for prose-only responses.

## [0.2.0] - 2026-08-22

### Added

- Independent preflight review before landing.
- The mdBook manual and its generated option reference.
- Tagged release automation with `release.yml`.

## [0.1.0] - 2026-08-22

### Added

- The Scufris Pi package: foreground identity, the delegated job loop, and the
  Nix flake with the Home Manager module.

[Unreleased]: https://github.com/alexjercan/scufris2/compare/v2.0.0...HEAD
[2.0.0]: https://github.com/alexjercan/scufris2/compare/v1.1.1...v2.0.0
[1.1.1]: https://github.com/alexjercan/scufris2/compare/v1.1.0...v1.1.1
[1.1.0]: https://github.com/alexjercan/scufris2/compare/v0.6.0...v1.1.0
[0.6.0]: https://github.com/alexjercan/scufris2/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/alexjercan/scufris2/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/alexjercan/scufris2/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/alexjercan/scufris2/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/alexjercan/scufris2/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/alexjercan/scufris2/releases/tag/v0.1.0
