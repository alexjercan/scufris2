# Changelog

All notable user-facing changes to Scufris.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html). Releases are
immutable `vX.Y.Z` tags; see [RELEASE.md](RELEASE.md) for the process.

## [Unreleased]

### Added

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
- `Super+Period` stops Scufris, built the same way and grabbed on the same
  terms. It cuts what is being spoken and ends the run, and it changes nothing
  else: a transcript you are still editing stays where it is, and the
  conversation keeps everything said so far. With nothing running it does
  nothing. `scufris-ctl abort` is the same verb from a terminal.
- `packages.scufris-speak`, the synthesiser the companion runs. It binds the
  pinned Piper package, model, and configuration, so the voice is a property of
  the package rather than a run-time setting.

### Changed

- Scufris is a background service with clients now, which is the whole shape of
  this release. `scufris-service` owns the conversation, the session, and the
  socket; the Pi agent, the desktop companion, and `scufris-ctl` are all
  clients of it. There is no terminal that owns the conversation any more, so
  putting the pill away, closing the terminal, or a companion crash leave the
  conversation exactly where it was, and a machine with no display still has
  one.
- `programs.scufris.desktop.enable` requires `programs.scufris.service.enable`.
  The tray's restart hook restarts `scufris-service.service`.
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

[Unreleased]: https://github.com/alexjercan/scufris2/compare/v0.4.0...HEAD
[0.4.0]: https://github.com/alexjercan/scufris2/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/alexjercan/scufris2/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/alexjercan/scufris2/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/alexjercan/scufris2/releases/tag/v0.1.0
