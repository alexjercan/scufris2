# Widget runtime: implementation

- STATUS: IN_PROGRESS
- PRIORITY: 85
- TAGS: widgets, desktop

## Plan (2026-08-25)

Implementation plan: `widget-runtime-plan.html` in this directory -
data structures for both sides of the socket and the shell, protocol
v2 with the correlation machinery, the file-by-file change map with
the complete dashboardd removal, and increment 1 as five commits. Both
details flagged for Alex were decided in page review the same day:
idle after working/speaking is the turn-ended signal, and Escape hides
the scratchpad layer whole (the pill is already a resident HUD; the
plan's auto-hide premise was wrong and is withdrawn). Raw code
inventory and the decision record in `RESEARCH.md`.
Published copy:
https://claude.ai/code/artifact/891f3c61-93fd-4b2f-9863-b8b4b4b667a5

Plan accepted by Alex (2026-08-25, "the Widget Runtime Buildout is
sound"). The planning exploration is closed; the page stands as the
reference for implementation.

## Goal

Implement the native widget runtime in scufris-desktop per the reviewed
design in `tasks/20260825-194822/` (design page
`scufris-widgets.html`, findings and all seven review decisions in
`RESEARCH.md`). dashboardd leaves this project in the first increment.

## Scope

The four increments from design page section 09, in order:

1. The runtime: protocol v2 with fixtures
   (`desktop/control-protocol-v2.json`), the `widgets` module beside
   the pill's, warm window pool, shell page with chrome and tokens,
   the note widget (text only), new `extensions/scufris/widgets`
   tools; dashboardd removed - flake input, both extension surfaces,
   the helper, the skill, the docs, and the nix ripples. The shell's
   busy indicator is the thinking-orbs orb the pill task vendors
   (`tasks/20260825-222037`, Alex approved 2026-08-25).
2. Live and aging: backend supervision (refcounted sharing keyed on
   (backend id, spawn data), kill group, coalesce, staleness), the
   shared `system` backend with the cpu widget on it, the full exhibit
   lifecycle (dim, grace, revive, pin, clear, frozen clocks), sticky
   exhibits with pin promoting to the current workspace, and
   runtime-owned widgets hiding and returning with the pill (state
   intact, clocks frozen while hidden).
3. Instruments: tray plus the voice verb for summoning, the timer and
   a tasks widget, `ctx.send` stdin actions for hand-typed input,
   `SCUFRIS_WIDGET_PATH` external roots for today's own widget.
4. The session HUD lands as a widget on this runtime (the escalation
   surface of task 20260825-153801), spawned through the same verbs.

## Verification

Per the proof column of design page section 09:

- "Show a note" lands an exhibit beside the pill mid-speech without
  stealing focus; grep finds no dashboardd anywhere.
- "CPU is at 90 percent" spawns the live graph; it dims on topic
  change, revives on citation, pins into an instrument; a killed
  backend shows the red accent, never a frozen number.
- A summoned timer survives the conversation ending; today's widget
  loads from outside the repo; a hand-typed tasks entry round-trips
  through the backend.
- The session HUD per task 20260825-153801's own verification.

Design and decisions: `tasks/20260825-194822/RESEARCH.md`.

## Increment 1 landed (2026-08-26)

Five commits, in the plan's landing order:

1. `5441f2c` Remove Dashboardd widget control
2. `50c6f90` Bump the control protocol to version 2
3. `ddd81b3` Correlate widget commands with their answers
4. `e04d6e1` Give the companion a widgets runtime and its windows
5. this commit: the widgets extension, its four tools, and the skill

Two deviations from the plan, both deliberate:

- The protocol carries nine widget body variants, not eight.
  `widget_done` answers an update, a close, and a clear, which name no
  new surface. Without it those three would have to answer with
  `widget_opened` and a surface they did not create.
- Surface identifiers are minted by the shell pool and never reused. A
  retired shell is destroyed rather than re-adopted, and the host
  reserves a shell before it asks the runtime to open anything, so the
  label is the surface identifier from the start. This resolves the
  plan's own conflict between "the surface id doubles as the window
  label" and "a retired shell returns to the pool". `Slot::Stage` is
  deferred to increment 4 with its first user, and the pin tick is a
  toggle rather than one-way.

Checks: `npm run check` (138 tests, Prettier clean), `cargo test` (168
tests), `cargo clippy --all-targets` clean, `nix flake check -L
--offline` all checks passed, `python3 -m unittest discover -s tests`
(33 tests). A scoped grep finds no dashboardd outside `tasks/` and
outside the CHANGELOG entry that records its removal.

Live acceptance of "show a note" is Alex's, on the desktop.

## Increment 2 landed (2026-08-26)

Four commits:

1. `fbf8644` Age an exhibit out when the turn moves on
2. `3466230` Take the widget layer down with the pill
3. `3534fe5` Promote a pinned exhibit onto the person's own workspace
4. `2448354` Feed a widget from a supervised backend process

Deviations from the plan, all deliberate:

- The life states and the turn boundary landed as one commit. A
  `Cmd::TurnEnded` with no caller is dead code, and `Life::Stale` and
  `Life::Dead` never arrived at all - health is its own field on the
  surface rather than another life state, because the two hold at the
  same time and say different things. A panel the person pinned can
  still be showing numbers from a process that died.
- Backend supervision and the `system`/`cpu` pair landed as one
  commit rather than two. The generated backend table asserts at
  build time that a backend exists, and a supervisor with nothing
  using it would ship untested against a real process.
- Two review findings were fixed in the course of the work rather
  than deferred to the review pass: `clear` took down instruments
  (fixed by `Surface::transient`, which now decides what ages, what a
  clear takes, and what goes down with the pill), and pinning left two
  windows in one shelf column (fixed by promoting to an edge slot,
  which is what the design said a pin does).
- The aging sweep and the backend beat both run on threads that hand
  their measurement to the event loop rather than performing acts
  themselves. The runtime decides under its own lock and the host
  carries the decisions out after releasing it, so a thread performing
  its own acts would be one more place window moves come from - and
  the review's open finding about that path is not made worse.
- Python 3 for the backends comes from the package rather than from
  the person's PATH, asserted by the desktop closure check.

Checks: `cargo test -p scufris-desktop` 201 passed, `cargo clippy
--all-targets` clean, `cargo fmt --all` applied, `npm run check` (138
tests, Prettier clean), `nix build .#checks.x86_64-linux.desktop-closure`
green (which also runs the crate's tests in the sandbox).

Live acceptance of the increment 2 verification line - "CPU is at 90
percent" spawning the live graph, dimming on topic change, reviving on
citation, pinning into an instrument, and a killed backend showing the
red accent - is Alex's, on the desktop.

## Increment 3, two of three parts landed (2026-08-26)

Two commits:

1. `e8e3c9e` Let a widget act on its backend, and add the timer
2. `66039f4` Let the person put a widget up, and take widgets from
   elsewhere

What landed: `ctx.send` actions onto the backend's standard input, the
`timer` widget and backend on it, the `shared = false` manifest key, the
tray summon submenu, and `SCUFRIS_WIDGET_PATH` external roots.

Deviations from the plan, all deliberate:

- `shared = false` is a new manifest key the plan did not have. Keying a
  backend on its identifier and its payload alone would make two
  five-minute timers one timer counted twice, which the plan's "every
  timer carries its duration and start" does not prevent.
- `Cmd::Open` carries an optional correlation identifier rather than a
  required one. A summon from the tray has nobody waiting on an answer,
  and a `widget_opened` for a request the daemon never made is a reply
  to a question nobody asked.
- The tray submenu offers the widgets with a backend rather than every
  installed widget. A summon carries no payload, so the widget has to
  fill itself; `note` would summon as an empty panel.
- External roots are additive and never override, and a widget on the
  search path that will not install is reported and passed over rather
  than stopping the companion. A shipped widget that is wrong is a build
  failure the developer sees; a search-path widget is a project on the
  person's machine that may be half-installed, and a login session with
  no companion in it is the worse outcome.
- Both backends redirect standard output to `/dev/null` when the pipe
  breaks. Catching the error is not enough on its own: the interpreter
  flushes again on the way out and complains on standard error, which
  the companion reads and logs.

Checks: `cargo test -p scufris-desktop` 214 passed, `cargo clippy
--all-targets` clean, `cargo fmt --all` applied, `npm run check` (138
tests, Prettier clean), `nix build .#checks.x86_64-linux.desktop-closure`
green. The timer backend was exercised standalone for the countdown, the
done state, pause, add, and reset.

### The `tasks` widget is blocked on a design question

Design question 3 says typing a task by hand must work without going
through Scufris. The widget window law is `.focusable(false)`, and the
design's own posture table says an instrument is "clickable; still never
steals" focus. An unfocusable window cannot receive keystrokes, so there
is nowhere for a typed character to land.

Raised with Alex rather than resolved here. The natural home for it is
the i3 binding mode task `20260825-153746`, which is the task about
keyboard routing.
