# Scufris widget runtime: research and design

- STATUS: IN_PROGRESS
- PRIORITY: 85
- TAGS: widgets, desktop, design, research

## Goal

Design a native widget runtime inside scufris-desktop that completely
replaces dashboardd in this project. Scufris spawns widgets as visual
aids and the user summons them as instruments; every widget may have a
backend for live data. dashboardd was the PoC that proved the idea and
stays its own project for linked browser dashboards; Scufris no longer
uses it.

## Direction (Alex, 2026-08-25)

- No backward compatibility. Not with dashboardd, not with the current
  scufris*widget*\* extension. Reimplementing agent extensions from
  scratch is fine and preferred; a new widget costs a couple hours, so
  nothing is worth building on dashboardd's contracts.
- No links or inputs between widgets. A Scufris widget shows something.
  Linked widgets remain dashboardd's territory.
- Backends stay: even a CPU widget benefits from one. Scufris says "CPU
  is at 90%" and spawns the widget to watch it live.
- The design document opens with a short "the pill spawns these" intro,
  then is only about the widgets themselves and how they work.

## Scope

- Research with parallel agents: dashboardd runtime lessons (done),
  scufris2 integration inventory and replace-list, prior art for native
  widget systems, Tauri/WebKitGTK multi-window practicalities.
- Synthesize into RESEARCH.md under this task.
- Write the design document as a reviewable design page, like the pill
  redesign did. Decisions land here after review.

## Verification

- RESEARCH.md records the findings with sources.
- The design page covers: widget model and manifest, backend contract,
  agent-facing protocol, window management and placement on i3/X11
  without a compositor, exhibit and instrument lifecycle, the v1 widget
  set, and the implementation increments.
- Alex reviews the design page; decisions are recorded before any
  implementation task starts.

Supersedes `tasks/20260825-153804` (dashboardd embed). Exhibit and
instrument lifecycle decisions from `tasks/20260822-132001/RESEARCH.md`
(design review) carry over as product behavior; the hosting story is
redesigned from scratch.
