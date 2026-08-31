# Add extensible morning briefings

- STATUS: CLOSED
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
- Treat seedzero as the first integration rather than embedding it in the
  scheduler.

Revised on 2026-08-31, with the owner:

- The canonical artifact is the run directory, not one Markdown file. It holds
  the manifest, one contribution file for each source, the aggregated prose,
  and the rendered page. The chat prose and the page are two readings of the
  same run.
- The HTML view is in scope now, not deferred. It is a deterministic render of
  a completed run, read-only, self-contained, and styled from the desktop
  tokens.
- A contribution is a JSON envelope with a Markdown body, not free Markdown.
  The envelope carries `title`, `status`, `headline`, `facts` and `body` so the
  page can be laid out without a model in the loop. The runner stamps `source`.
  A source that answers with something unparseable is recorded as failed with
  its raw text kept, and is named in the briefing.
- Build the configuration, the collector, the renderer, and one manual run tool
  before the scheduler. The format is what needs proving first.
- Do not collect through `scufris_job_spawn`. Jobs are tmux panes bound to an
  owner session and steerable; a morning source is one bounded headless run.
  Each source runs `pi -p` or `claude --print` in its project root.
- The `briefings` table must never reach the delegation menu. The foreground
  orchestrator would read a briefing entry as an agent it may start.
- Catch up unconditionally inside the local date. A start at any hour with no
  delivered run for today runs the briefing then. No cutoff hour at first.
- Never open the page by itself. Nothing in the stack answers "is the owner at
  the desk": the service tracks registered surfaces, but no presence field
  reaches the agent, and a graphical session is present whether or not the
  owner is home. The prose goes to chat, which reaches every surface, and the
  page opens when it is asked for.
- One deadline for each source at 900 seconds, and one for the whole run. All
  sources start at once; there is no concurrency cap. A source that misses its
  deadline is recorded as failed and named in the briefing, and the run
  publishes with what came back.

## Evidence

Completed on 2026-08-31:

- `npm run check`: 85 tests passed; type checking and Prettier passed.
- `python3 -m unittest discover -s tests -p 'test_*.py'`: 249 passed.
- `cargo test -p scufris-service`: 9 service tests passed.
- `nix flake check`: all checks passed on `x86_64-linux`.
- One real run over two sources with a scratch `XDG_STATE_HOME`: this project
  answered `attention` in 69.5 s and `the-den` answered `stale` in 75.0 s. The
  Den reported the missing data for yesterday instead of guessing it. `publish`
  wrote the prose and `render` wrote a 7.2 KB self-contained page.
- The renderer escapes every source value before it applies markup.
  `tests/test_briefing.py` holds the script, attribute, and `javascript:` link
  cases, and asserts that the page loads nothing from the network.
- The page is a deterministic render of a completed run. Chat and the page read
  the same manifest and the same contributions.
- Association after a fresh service restart: an answer sent before any surface
  message was rejected with `no_surface` and reached no surface at all. A live
  staging run on 2026-08-31 showed what that costs - the briefing was written
  and rendered, and the conversation stayed empty. The service now records such
  an answer against the reserved surface name `unprompted`, so every surface
  displays it and none speaks it. That is the display-only period this
  acceptance asked about.
  `an_unprompted_response_is_shown_by_every_surface_and_spoken_by_none` in
  `host/service/src/service.rs` holds it,
  `no_surface_may_register_the_name_an_unprompted_answer_carries` in
  `shared/control/src/service.rs` keeps the name unclaimable, and
  `docs/src/dev/briefings.md` and `docs/src/dev/surfaces.md` document it.
- Seedzero declares no briefing yet, so it is not part of this run. Its entry is
  three fields in its own `.scufris.toml` and needs no change here.
