# Research: how people use delegation assistants and what makes Scufris trustworthy for delegate-and-verify work

- STATUS: CLOSED
- PRIORITY: 80
- TAGS: research

## Request

Alex asked for market research on how other people work with tools like
Scufris, and for feature ideas that would make Scufris used more. Single
research agent, no code changes.

## Framing

Usage splits into three trust tiers:

1. Fire and forget. Morning briefing, "do the videos for seedzero and upload
   all changes". Works. Wanted: more of this.
2. Delegate but verify. Scufris features implemented by a delegated agent,
   nightly code review. Scufris says "started the agent" and later claims a
   merge or push that did not happen: branch deleted without merge, "pushed"
   with no CI release run. Alex falls back to tmux, which defeats the point.
3. Keep for yourself. nova-protocol story brainstorming, pair programming,
   flow work. Scufris should stay out of the way. Not a gap to fill.

The lever is tier 2: verifiable completion claims and visible agent state
without tmux. That would move some tier 3 work (nightly review) into tier 2.

## Deliverable

`NOTES.md` in this directory: trust-tier model, market findings with sources,
ranked feature candidates with a verification story each, one recommendation.
An artifact page rendered from the same note.

## Outcome (2026-09-08)

- `NOTES.md` holds the full research note: 11 tools, 32 sources, 6 audit
  questions with `file:line` evidence, 8 ranked candidates, 5 rejected.
- Page: https://claude.ai/code/artifact/1759a735-1cf8-4e03-9320-af364fc00865
- Spot-checked by hand after the agent finished: no `git push`, `ls-remote`,
  `gh run`, `gh pr`, `gh release`, or `git tag` under `tools`, `scripts`,
  `agent`, `.claude/skills`; `SCUFRIS_BRIEFING_PROFILE` is read at
  `agent/extensions/scufris/briefing/briefing.ts:121` and set nowhere;
  `sprout rm` runs `git branch -D` with no merged or role check.

## Decision

Recommended order, pending Alex's choice:

1. Landing receipts: helper `receipt` command, `receipt.json`, foreground
   quotes verified fields and marks the rest "claimed, not verified".
2. Worker boundary: `sprout rm` and `sprout land` refuse as worker; `rm`
   refuses unmerged without force; stop-with-removal needs a landed receipt.
3. Jobs widget on desktop, then iOS list frame.
4. Nightly review as a scheduled briefing profile.

Follow-up work gets its own Tatr task per item.

## Round 2 (2026-09-08)

Alex commented on the page and annotated the reply. Revised order:
receipts; scheduler with profiles, built-in sources, and offers; verify
agent; HUD response fields plus jobs rows in the conversation window.
Dropped: named routines, trust flag, jobs widget first. Four-widget cap is
the four edge slots in `surfaces/desktop/src/widgets/runtime.rs:36`, by
design. Details in `NOTES.md`, Round 2.

## Successor tasks

- 20260908-103402 landing receipts (p100)
- 20260908-103403 briefing scheduler, built-in sources, offers (p90)
- 20260908-103404 verify agent (p80)
- 20260908-103406 HUD receipts, offers, live job rows (p70)
