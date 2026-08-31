# Add extensible morning briefings

- STATUS: OPEN
- PRIORITY: 0
- TAGS: backlog

## Goal

Deliver one unprompted morning briefing assembled from configured project
sources. Scufris writes a coherent prose briefing, preserves it as a durable
artifact, and delivers it in chat. HTML or another richer presentation can
render the same artifact later.

Seedzero is the first source, not a special case. Non-project concerns can use
a small Git project with its own `.scufris.toml`.

## Configuration

Extend `.scufris.toml` with an explicit briefing source table. Keep scheduled
briefings separate from the request-driven `[agents.*]` menu.

```toml
[briefings.morning]
description = "Report production cadence, recent performance, and pending QA."
keywords = { harness = "pi", model = "openai-codex/gpt-5.6-sol", thinking = "medium" }
guidance = """
Read real project data and identify the useful changes or actions for today.
Never invent missing values.
"""
```

A project can omit the table. The initial implementation needs only the
`morning` profile, but the schema must leave room for later profiles such as
`weekly`, `evening`, or `release`.

## Direction

- Add a foreground briefing extension under
  `agent/extensions/scufris/`, registered in `package.json` and gated on
  `SCUFRIS_ROLE == "orchestrator"`.
- Configure one global morning time and timezone in Scufris. On
  `session_start`, arm one `setTimeout`; catch up immediately after the daily
  time. Do not poll.
- Discover Git projects under `SCUFRIS_PROJECT_ROOTS` that declare
  `[briefings.morning]` and ask each configured source for one bounded,
  evidence-based contribution.
- Aggregate all contributions in Scufris's voice. Do not relay a sequence of
  independent agent reports.
- Preserve canonical Markdown and a small run manifest under Scufris state.
  Chat uses the prose. A future HTML view must render this same artifact rather
  than invoke a second generation path.
- Track briefing state by local date. Distinguish completed delivery from
  failed or partial generation so a restart neither duplicates a delivered
  briefing nor silently loses an incomplete one.
- Use Pi's proactive wake mechanism:
  `pi.sendMessage(..., { deliverAs: "followUp", triggerTurn: true })`.
- Keep source-specific paths, refresh commands, and interpretation in each
  project's briefing guidance. Do not hard-code seedzero behavior in the
  Scufris extension.

Seedzero's first contribution should report its cadence gap, recent statistics
changes, and pending QA from real project data, and may offer relevant project
jobs. Its data refresh helper and status schema remain owned by seedzero.

## Acceptance

- A configured morning briefing arrives unprompted once per local day.
- A late login catches up exactly once. Restarts do not duplicate a delivered
  briefing.
- Only projects with `[briefings.morning]` contribute.
- Multiple project contributions become one coherent briefing.
- Every factual claim is grounded in source data; missing or failed sources are
  identified instead of guessed.
- The generated Markdown artifact and run manifest survive delivery.
- The chat response and any later rich view derive from the same canonical
  artifact.
- No polling loops run; there is one timer per foreground session.
- Association behavior after a fresh service restart is verified and
  documented, including any display-only period before the owner speaks.

## Decisions

- Use `[briefings.morning]`, not `[agents.briefing]`.
- Keep the schedule global and briefing sources project-local.
- Use Git projects as the initial container for non-project briefing sources.
- Make Markdown the canonical presentation artifact; defer the HTML renderer.
- Treat seedzero as the first integration rather than embedding it in the
  scheduler.

## Evidence
