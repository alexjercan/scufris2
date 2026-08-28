# Integrate today into Scufris as native tools

- STATUS: CLOSED
- PRIORITY: 70
- TAGS: extension, today

## Objective

Give Scufris the today contract as native Pi tools so it can answer
from and write to daily data, pairing spoken answers with the
already-deployed today widget variants. This is roadmap stage 1 of the
research task `20260823-233541`; the implementation brief is its
`ARCHITECTURE.md` area 1 (option 1B) and `UX.md` flows 1-3.

Ordering: after the scufris-desktop HUD v1 (`20260822-132001`). The
today repo task `~/personal/today/tasks/20260824-130132` may change
the CLI API; build against the current contract and track changes.

## Scope

- A private helper `tools/today/scufris-today` in the
  `scufris-dashboard` adapter mold: validates requests, shells to
  `today --json`, deadline and bounded JSON envelopes, never invokes
  bare `today`.
- Two grouped native tools mirroring the observation/mutation split:
  `scufris_den_read` (show, upcoming, weight history, macros day,
  notes, habits) and `scufris_den_write` (task, habit, weight, macros,
  note mutations). Revision conflicts surface as typed tool errors
  that instruct a re-read; destructive actions require explicit user
  confirmation in conversation.
- `skills/den/SKILL.md` with the answer-plus-widget rules: answer from
  data first; open at most one relevant widget - when asked, when
  shape beats speech, or as mutation confirmation.
- Capability gating at session start: absent or version-skewed today
  means the tools do not register and identity notes the gap (the
  dashboardd pattern).
- Home Manager module gains an optional `todayPackage`.

## Completion criteria

- Extension tests run against a fixture den (reuse today's test
  fixtures) covering reads, writes, revision-conflict retry, and
  capability gating.
- A live session demonstrates UX flows 1-3: "what do I have tomorrow"
  answered from data; a voice-added future task surviving a concurrent
  Neovim edit; a weight-trend answer with the today.weight widget
  opened alongside.
- The tool surface stays at two tools; no per-subcommand tool sprawl.
- `npm run check` and `nix flake check` pass; released and pinned
  through the normal gate.

## Closed

Closed on 2026-08-28 without being built. Nothing here shipped: no helper, no
tools, no skill, no gating, no Home Manager option.

The panels landed instead, under `20260828-134642`. They read and write the-den
from the workspace, which covers the glance and the tick. What stays missing is
the spoken half - asking for the day and writing to it by voice - because that
is what these two tools were for.

Reopen with `tatr edit 20260824-130127 --status OPEN` when the voice road is
next. The scope above still holds; only the today CLI surface may have moved
under it.
