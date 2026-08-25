# Widget runtime: implementation

- STATUS: OPEN
- PRIORITY: 85
- TAGS: widgets, desktop

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
   the helper, the skill, the docs, and the nix ripples.
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
