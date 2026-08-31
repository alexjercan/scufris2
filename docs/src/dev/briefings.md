# Morning briefings

[Previous: Messages](messaging.md)

```text
one timer -> collect every declared source -> run directory -> Scufris writes it
                                                           -> chat
                                                           -> page, when asked
```

One briefing is one run. Every run is a directory named for its local date, and
it holds everything the morning was built from: the manifest, one contribution
for each source, the prose Scufris wrote, and the page rendered from the same
run. Chat and the page are two readings of one artifact, so neither can say
something the other does not.

## What a source is

A source is a Git project under `SCUFRIS_PROJECT_ROOTS` that declares a
briefing in its own `.scufris.toml`:

```toml
[briefings.morning]
description = "Report the cadence gap, recent statistics, and pending QA."
keywords = { harness = "claude", model = "opus", thinking = "high" }
guidance = """
Read web/data and the published slate. Report what changed overnight and what
today needs. Never invent a number you did not measure.
"""
```

The three fields are the ones an agent entry takes, so nothing new has to be
learned to write one. What differs is that nobody chooses a briefing: it is
asked on a schedule, so `guidance` is what the source is asked and is required.

A project may declare any profile name. Only `morning` is scheduled today; the
schema leaves room for `weekly`, `evening` or `release` without a change.

The `briefings` table never reaches the delegation menu. `scufris_project_context`
renders conventions and agents only, because a briefing rendered beside the
agents would read as one more agent a request could name. Each table has its own
reader, so a mistake in a briefing costs the project its briefing and not its
agents.

## What a source answers

One JSON envelope with a Markdown body:

```json
{
  "title": "The Den",
  "status": "attention",
  "headline": "Two tasks are left over from yesterday.",
  "facts": [{ "label": "Restant", "value": "2 tasks" }],
  "body": "### Yesterday\n\n- call the dentist\n"
}
```

`status` is `ok`, `attention` or `stale`. `facts` is at most six measured
values. Free Markdown would read well and lay out badly: the page needs a
title, a state and a few values it can put in a row without a model in the
loop.

Only the runner writes `failed`. A source that answers with prose, exits badly,
or misses its deadline becomes a failed contribution whose headline says why,
with its own words kept beside it, and it is named in the briefing rather than
quietly dropped.

## How a source runs

One bounded headless run in the project root, not a job:

|           | job                  | briefing source |
| --------- | -------------------- | --------------- |
| lives in  | a tmux pane          | one process     |
| owned by  | a foreground session | the run         |
| steerable | yes                  | no              |
| session   | restored by ID       | none            |

`pi --print` or `claude --print`, with the edit tools off. That is an intent
boundary and not a sandbox: a source that runs a refresh command runs it with
the owner's own hands, exactly as a review workspace does. The project's
guidance is what keeps it honest.

Every source starts at once. Each is bounded on its own at 900 seconds, and the
whole run at 1800; both move with `SCUFRIS_BRIEFING_SOURCE_DEADLINE` and
`SCUFRIS_BRIEFING_DEADLINE`. One project that hangs costs the run its own
deadline and nothing else.

## The run directory

```text
$XDG_STATE_HOME/scufris/briefings/2026-08-31/
├── manifest.json          the index: state, every source, every diagnostic
├── contributions/*.json   one envelope for each source, with its body
├── briefing.md            the prose Scufris wrote
└── briefing.html          the page rendered from all of it
```

The last thirty runs are kept. The manifest state is the record of what
happened, and it is what a restarting session reads:

| state        | what it means                  | what a session does                |
| ------------ | ------------------------------ | ---------------------------------- |
| `none`       | nothing was started            | collect, if the morning has passed |
| `collecting` | a run no process owns any more | collect again                      |
| `collected`  | gathered, prose never written  | ask for the writing                |
| `delivered`  | the owner has it               | wait for tomorrow                  |
| `failed`     | nothing answered               | wait for tomorrow                  |

A restart therefore neither delivers a briefing twice nor loses one that was
gathered and never written up.

## The schedule

One timer for each foreground session, armed at `session_start` and re-armed
after each decision. Nothing polls.

The morning is `programs.scufris.agent.briefing.time`, default `08:00`, local
to the host, or `off`. A session that opens after that time with no run for
today catches it up once, at any hour: a morning nobody was awake for is still
a morning that was never delivered.

A morning no project declared is not an event. The run is recorded and the
foreground is never woken, so the schedule costs nothing until a project asks
for something.

## Writing and delivery

The collected run wakes the foreground once, through the same proactive path a
worker event uses. Scufris reads the run, writes one briefing in its own voice
from what the sources reported, publishes that prose, and says it in chat.

The page never opens by itself. Nothing in the stack answers "is the owner at
the desk": the service tracks registered surfaces, but no presence field
reaches the agent, and a graphical session is there whether or not anyone is
home. Chat reaches every surface; the page opens when it is asked for.

### Before the owner speaks

The service associates an answer with the surface that sent the last message. A
service that has just started holds no association at all, so a briefing
delivered before anyone has spoken is rejected with `no_surface` and reported as
an error notice. There is no display-only period: the answer is not shown on a
surface and then left unspoken, it never reaches a surface. Once the owner
speaks once, the ordinary rule applies again, and every surface displays the
answer while only the associated ready surface speaks it.

Nothing is lost when that happens. `scufris_briefing_publish` writes the prose
and the page before Scufris says anything, so the run holds the briefing whether
or not chat accepted it, and the run is already `delivered`, so no later session
collects the morning twice. The owner reads it with `scufris_briefing_show` or
opens the page.

This is not particular to briefings. Every proactive wake shares it, a finished
job included. `a_proactive_response_waits_for_the_first_surface_message` in
`host/service/src/service.rs` holds the behavior.

## Tools

| Tool                       | What it does                                                      |
| -------------------------- | ----------------------------------------------------------------- |
| `scufris_briefing_run`     | Collect now. Returns at once; the finished run wakes the session. |
| `scufris_briefing_show`    | Read one run: every contribution, the prose, every diagnostic.    |
| `scufris_briefing_publish` | Keep the prose Scufris wrote and render the page.                 |
| `scufris_briefing_open`    | Open a page on this machine.                                      |

## From a terminal

The same program, for whoever is not the agent:

```bash
scufris-briefing sources
scufris-briefing collect
scufris-briefing show --json
scufris-briefing publish < prose.md
scufris-briefing open --date 2026-08-30
```

`tools/briefing/page.py` renders and asks nothing: given a finished run it
writes the same page a year from now. Everything a source wrote is escaped
before any markup is applied, and the page carries its own styling, so it
opens from a state directory with no server, no fonts to fetch and no script to
run.

---

Next: [Tmux](tmux.md)
