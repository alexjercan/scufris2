# Dashboardd embed: exhibits and instruments

- STATUS: OPEN
- PRIORITY: 50
- TAGS: voice, desktop, hud

## Goal

The v3 increment of task 20260822-132001: embed dashboardd-runtime so
scufris-desktop hosts two kinds of floating windows out of the same
runtime. dashboardd itself stays Scufris-agnostic.

## Scope

- Exhibits: Scufris spawns them as visual aids while speaking, through
  the same spawn interface as every surface.
- Instruments: the user summons them (calendar, tasks); interactive,
  focused, alive until closed. Timers sit in both camps.
- Exhibit lifecycle (review verdict): exhibits age on topic relevance,
  not on the pill; closing the pill changes nothing. A topic change dims
  to ~40%, a ~60s grace window follows, then a quick exit - dimming was
  the feedback. Nothing disappears straight from LIVE. Citation or hover
  revives. Every clock freezes while the mic is hot, Scufris speaks, or
  the pointer is over the exhibit. Only the close tick and a "clear"
  verb exit instantly. The pin tick promotes an exhibit into an
  instrument: it stops aging and is user-owned.
- The agent feeds both kinds; no per-widget config.
- dashboardd-desktop demotes to a manually launched tool.

## Verification

- An exhibit spawned mid-speech survives pill close, dims on topic
  change, revives on citation, and pins into an instrument.
- Clocks freeze while dictating with the pointer off the exhibit.
- A summoned calendar instrument stays until closed.

Decisions: `tasks/20260822-132001/RESEARCH.md` design review section
(exhibits and instruments; exhibit lifecycle). Depends on the session
HUD task for the spawn interface.
