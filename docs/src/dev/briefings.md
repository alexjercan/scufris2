# Briefings

[Previous: Messages](messaging.md)

```text
systemd timer -> collect every declared source -> run directory -> Scufris writes it
     |                                                          -> chat
     +-> wake the conversation                                  -> page, when asked
```

One briefing is one run. Every run is a directory named for its local date and
its profile, and it holds everything the briefing was built from: the manifest,
one contribution for each source, the prose Scufris wrote, and the page
rendered from the same run. Chat and the page are two readings of one artifact,
so neither can say something the other does not.

The date and the profile together name a run, so a morning and an evening on
one day are two runs and neither can write over the other.

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

A project may declare any profile name. Which of them are scheduled is the
host's decision and not the project's: a project that declares `[briefings.weekly]`
contributes to a weekly briefing on a host that schedules one, and to nothing on
a host that does not.

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

The envelope is read for its own end, not for a closing fence. A body is
Markdown and may fence a diff or a status listing of its own, so the first
fence after the opening one is usually inside the answer rather than after it.
Each fenced block is decoded from its opening brace, a block that starts inside
one already read was quoted by it, and the last block left is the answer.

Every limit the reader holds a source to is in the prompt it was given: the
title, headline, label, value and body lengths. A contribution dropped for a
rule nobody stated is work done twice.

A source that answered badly is asked once more. By then it has read its
project and spent whatever its guidance allowed, so the report exists and was
only mis-said: a stray quotation mark inside the body, a headline a few
characters long. The second asking is given the source's own words and the one
reason they could not be used, with no tools at all, and may change only that.
It is short - a real 4507 character answer was recovered in 26 seconds against
the 197 the first run cost - and it is bounded by
`SCUFRIS_BRIEFING_REPAIR_DEADLINE`, capped by whatever the source has left. A
source that never answered is not asked again: there is nothing to correct.

Only the runner writes `failed`. A source that answers with prose twice, exits
badly, or misses its deadline becomes a failed contribution whose headline says
why, with its own words kept beside it, and it is named in the briefing rather
than quietly dropped.

Nothing a source does ends the run. The reader refuses every malformed answer
by name rather than raising, including one nested past the decoder's stack or
written in bytes that are not text; a source that finds a way past that is
caught at the fan-out and named; and a page that cannot be laid out leaves the
collected run standing, with the reason kept in the manifest.

## How a source runs

One bounded headless run in the project root, not a job:

|           | job                  | briefing source |
| --------- | -------------------- | --------------- |
| lives in  | a tmux pane          | one process     |
| owned by  | a foreground session | the run         |
| steerable | yes                  | no              |
| session   | restored by ID       | none            |

`pi --print --approve` or `claude --print --permission-mode bypassPermissions`,
with the edit tools off. That is an intent boundary and not a sandbox: a source
that runs a refresh command runs it with the owner's own hands, exactly as a
review workspace does. The project's guidance is what keeps it honest.

Both harnesses answer without asking on purpose. Nobody is watching a source
run, so a question it cannot ask is a refusal. Under `claude`'s `dontAsk` the
shell is sandboxed, and a source told to read CI or refresh its numbers spent
its run reporting that `gh` or `python3` had been denied. The tool list and the
project's guidance decide what a source may reach; a sandbox it cannot see
decides nothing but whether the morning is empty.

A source is asked to change nothing and spend nothing unless its own guidance
names it, and then only what it names. A project whose morning is worth a
refresh says so in its own file: seedzero's briefing reads the channel through
the API and writes the two data files it just read, under a stated cap on how
many reads that may cost. What a source may spend is the project's decision,
written where the rest of that project's intent lives.

Every source starts at once. Each is bounded on its own at 900 seconds, and the
whole run at 1800; both move with `SCUFRIS_BRIEFING_SOURCE_DEADLINE` and
`SCUFRIS_BRIEFING_DEADLINE`. A second asking is bounded at 300 by
`SCUFRIS_BRIEFING_REPAIR_DEADLINE`, never past what its source has left. One project that hangs costs the run its own
deadline and nothing else.

## The run directory

```text
$XDG_STATE_HOME/scufris/briefings/2026-08-31/
├── morning/
│   ├── manifest.json          the index: state, every source, every diagnostic
│   ├── contributions/*.json   one envelope for each source, with its body
│   ├── briefing.md            the prose Scufris wrote
│   └── briefing.html          the page, written when the sources answer
└── evening/
    └── ...                    the same again, and never the same files
```

`briefing.md` appears when Scufris writes the day up. `briefing.html` does not
wait for it: the collection renders the page from the contributions alone, and
renders it again over the prose when there is some. Whether the day has a page
is decided by code, so a write-up that never happens costs the day its prose and
not its briefing.

The last thirty dates are kept, with every profile that ran on them. The
manifest state is the record of what happened, and it is what an opening
session reads:

| state        | what it means                  | what a session does |
| ------------ | ------------------------------ | ------------------- |
| `none`       | nothing was started            | nothing             |
| `collecting` | a run no process owns any more | nothing             |
| `collected`  | gathered, prose never written  | ask for the writing |
| `delivered`  | the owner has it               | nothing             |
| `failed`     | nothing answered               | nothing             |

Only `collected` asks a session for anything, and only when the run has sources
and no prose beside it. Everything else is the timer's business: a session
never decides that a briefing is owed, so it can neither deliver one twice nor
collect one nobody asked for.

A caller that names a date and no profile is resolved against what is on disk:
it means the one run that was gathered and is still waiting for its prose.
Two of those are two briefings, and it refuses rather than guesses between
them. `publish` resolves only to a waiting run, so a wake can put one
briefing's prose neither on another's page nor on one already delivered.
Reading a day where nothing is waiting resolves to the one run there is, and
also refuses to choose between two.

## The schedule

Nix owns when a briefing happens; each project's `.scufris.toml` owns what is
in it. `programs.scufris.agent.briefing.profiles` is an attribute set of
profile name to a schedule:

```nix
programs.scufris.agent.briefing.profiles = {
  morning.schedule = "07:30";
  weekly = {
    schedule = "Mon *-*-* 09:00";
    deadline = 3600;
  };
};
```

Each one renders `scufris-briefing-<profile>.timer` and its `oneshot` service.
The unit collects that profile and carries the result to the conversation with
`scufris-ctl wake`. Nothing in the agent holds a clock, and nothing polls.

`schedule` is a systemd `OnCalendar` specification, checked with
`systemd-analyze calendar` while the module is built, so a schedule nobody can
act on fails the build rather than the morning. systemd reads none of crontab's
syntax: `0 7 * * *` is not a schedule and is refused where a person can still
fix it.

`Persistent=true` is what catches a briefing up. systemd triggers a unit
immediately when it would have fired at least once while the timer was
inactive, so a machine that was off at half past seven collects at login
instead. That is one rule kept by systemd rather than arithmetic written here
about what a late session owes the day. A profile that is only worth having on
time sets `persistent = false`.

`{}` schedules nothing, and the tools still collect a briefing when asked.
Timers are systemd's, so nothing is scheduled off Linux.

A briefing no project declared is not an event. The run is recorded and the
foreground is never woken, so a schedule costs nothing until a project asks for
something.

## Writing and delivery

The collected run wakes the foreground once. The timer's own run does it from
outside the agent, over `control.wake`; a briefing asked for by hand does it in
process. Both send the same words, because both ask the helper for them.
Scufris reads the run, writes one briefing in its own voice from what the
sources reported, publishes that prose, and says it in chat.

The run on disk is the durable half and the wake is only the delivery. A wake
refused with `agent_unavailable` therefore leaves the run `collected`, and the
next session that opens reads what is waiting and asks for the writing. Losing
a gathered briefing because the agent happened to be down is the failure this
does not have.

That session-start read is one file read and not a timer. It happens once, at
`session_start`, and there is no interval behind it.

Everything before the prose is code. The schedule, the sources, the runs and
the record are decided by systemd and `briefing.py`, and no model is asked
whether a briefing should happen. Scufris adds the prose on top of a run that
already exists and already has a page.

The page never opens by itself. Nothing in the stack answers "is the owner at
the desk": the service tracks registered surfaces, but no presence field
reaches the agent, and a graphical session is there whether or not anyone is
home. Chat reaches every surface; the page opens when it is asked for.

### Before the owner speaks

The service associates an answer with the surface that sent the last message,
and a service that has just started holds no association at all. The morning
briefing is the ordinary case of that: it is the first thing said after a
restart, before anyone has typed anything.

Such an answer is displayed rather than refused. It is recorded against the
reserved surface name `unprompted`, so every surface shows it and none of them
matches it: nothing is spoken aloud, and no live widget call runs. That is the
display-only period. Once the owner speaks once, the association is set for the
life of the service and the ordinary rule applies, so a briefing is spoken by
the surface the owner last used.

This is not particular to briefings. Every proactive wake shares it, a finished
job included. `an_unprompted_response_is_shown_by_every_surface_and_spoken_by_none`
in `host/service/src/service.rs` holds the behavior, and no surface may register
that name.

## Tools

| Tool                       | What it does                                                      |
| -------------------------- | ----------------------------------------------------------------- |
| `scufris_briefing_run`     | Collect now. Returns at once; the finished run wakes the session. |
| `scufris_briefing_show`    | Read one run: every contribution, the prose, every diagnostic.    |
| `scufris_briefing_publish` | Keep the prose Scufris wrote and render the page.                 |
| `scufris_briefing_open`    | Open a page on this machine.                                      |

Each takes a `profile`, and the wake names the one it was collected for.
Without one they resolve to the run that is waiting to be written up.

## From a terminal

The same program, for whoever is not the agent:

```bash
scufris-briefing sources --profile morning
scufris-briefing collect --profile morning
scufris-briefing wake --profile morning
scufris-briefing pending --json
scufris-briefing show --json
scufris-briefing publish < prose.md
scufris-briefing open --date 2026-08-30 --profile morning
```

Every subcommand takes `--profile`. `wake` is what the timer runs after a
collection: it carries the run to the conversation and reports rather than
fails when nothing is listening. `pending` is what an opening session reads.

`tools/briefing/page.py` renders and asks nothing: given a finished run it
writes the same page a year from now. Everything a source wrote is escaped
before any markup is applied, and the page carries its own styling, so it
opens from a state directory with no server, no fonts to fetch and no script to
run.

---

Next: [Tmux](tmux.md)
