# Fast-verb tier with timers and agent fallback

- STATUS: OPEN
- PRIORITY: 65
- TAGS: voice, desktop

## Goal

Deterministic intents answered in under 1.5 s with an earcon; everything
else falls through to Pi unchanged. Sub-1.5 s decides voice adoption.

## Scope

- First verbs: timers ("set a timer for N"), open/focus a named window,
  mute, "brief me". Timers lead.
- Verb matching is deterministic and local (small grammar, no model
  call). A miss falls through to the normal agent submission with no
  extra latency.
- A verb answers with the accept earcon and a one-line pill
  confirmation. Zero snark in confirmations.
- Timers persist while running and become the first widget content for
  the later dashboardd embed (a "both" surface: born from a verb,
  persistent while running).
- Keep the verb tier in small deterministic helpers, not extension
  complexity.

## Verification

- A timer round-trips under 1.5 s from end of speech.
- A non-verb utterance reaches Pi exactly as before.
- A timer survives a companion restart.

Backlog item 2 in `tasks/20260822-132001/RESEARCH.md` section 5.
