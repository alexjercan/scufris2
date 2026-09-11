# Briefings

[Previous: Messages](messaging.md)

```text
systemd timer -> bounded collectors -> generation-fenced run directory
                       |                         |
                       +-> quiet lifecycle rows +-> terminal wake
                                                    |
                           service durable inbox -> one correlated Pi turn
                                                    |
                                      canonical replay acknowledgment
```

One briefing is one run. Every run is a directory named for its local date and
its profile, and it holds everything the briefing was built from: the manifest,
one contribution for each source, the prose Scufris wrote, and the page
rendered from the same run. Chat and the page are two readings of one artifact,
so neither can say something the other does not.

The date and profile locate the current run. An opaque generation ID is its
identity. A morning and an evening on one day use different directories, and a
late writer from an older generation cannot reopen, complete, or finalize the
current generation.

## What a source is

A source declares `[briefings.<profile>]`. A Git project under
`SCUFRIS_PROJECT_ROOTS` does it in its own `.scufris.toml`:

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

### Sources the machine declares

Not everything worth reporting belongs to a checkout. What Scufris did
overnight is read out of the job records and the receipts beside them, and no
project owns those. The machine declares such a source for itself, in
`$XDG_CONFIG_HOME/scufris/config.toml`:

```toml
[briefings.morning.jobs]
description = "What Scufris did overnight."
keywords = { harness = "pi", thinking = "medium" }
root = "/home/you/personal"
guidance = """
Run `scufris-jobs history --since <the moment above> --json`. Quote what the
receipt says; do not judge whether the work is done.
"""
```

`[briefings.<profile>.<name>]`. The name is required here, because there is no
project name to take one from. The entry is validated by the same reader that
validates a project's, so a source is one thing to learn and not two, and an
optional `root` says where it runs. Without one it runs in the home directory,
which is what anything reading XDG state wants.

This is not a built-in kind of source. It is asked the same question, held to
the same deadlines, repaired the same way, and laid out on the same page. The
first one reports the jobs, and the next needs nothing built.

The file is briefings-only, and a file that declares anything else is refused
whole. Conventions and agents stay in the project that declares them, because
a checkout has to work for someone whose machine has none of this.

A source the machine declares is named `@<name>`, and that name is also the
file its answer is kept in. A project ID cannot start with `@`, so a machine
section called `scufris2` cannot take the `scufris2` project's contribution:
the reader assigns every slug, and the collision is not expressible rather
than checked for.

`--config` and `SCUFRIS_CONFIG` name another file, the flag winning over the
variable. A file somebody named and is not there is a refusal, because
`--config typo.toml` quietly reporting no sources is a morning discovered too
late. The default path being absent is not: that is a machine that declared
none. A file that is there and is malformed costs itself and nothing else -
one diagnostic naming it, and every project still contributes.

The helper reads a path. Home Manager generating this file from a typed option
is one way to have one, and writing the same TOML by hand is another.

### Since the last run

Every source is told when its own profile last began a run, read from the runs
still on disk. A weekly source then reports on a week and a morning source on a
night, and no source, project or machine, carries a window setting.

The start and not the finish. The last run's sources were asked at about its
start and reported the world as they found it then, so measuring from its
finish would leave everything that happened during the collection after what
that briefing said and before what this one is told to look at. Measuring from
the start overlaps instead, and a job named in two briefings is better than one
named in neither. It also gives a run that crashed while collecting a usable
moment: the one it asked its sources at.

It is given as a fact and not as an instruction. Guidance that names its own
window - yesterday's macros, the last three sessions, the last twelve commits -
keeps it; the moment answers "since when" only for a source that asked the
question. A profile that has never run says so, rather than leaving a model to
invent a period it cannot measure.

## What a source answers

One JSON envelope with a Markdown body:

```json
{
  "title": "The Den",
  "status": "attention",
  "headline": "Two tasks are left over from yesterday.",
  "facts": [{ "label": "Restant", "value": "2 tasks" }],
  "body": "### Yesterday\n\n- call the dentist\n",
  "offers": [
    { "label": "Call the dentist", "detail": "Left over from Tuesday." }
  ]
}
```

`status` is `ok`, `attention` or `stale`. `facts` is at most six measured
values. Free Markdown would read well and lay out badly: the page needs a
title, a state and a few values it can put in a row without a model in the
loop. The body and synthesized prose use the same CommonMark pipeline, with
GitHub-style tables in addition to headings, paragraphs, emphasis, code,
links, lists, block quotes, fenced code and horizontal rules.

`offers` is at most three things the owner could do next about what this
source found, each a label saying what to do and a detail saying what and why.
A label and a detail is the whole shape. A stored prompt for a worker to run
would only make sense for a delegated coding job, so it would quietly restrict
offers to code sources: a briefing about a calendar or a house has a next step
too and no prompt to give. Whoever picks one writes the words for it then,
knowing it was picked.

Only the source that read the project can propose from it, which is why offers
are in the envelope rather than written afterwards. The merge is the other way
round: every source runs at once, so nothing collected can see what another
source found, and the numbered list is assembled by code at the end of the run.
Numbers are assigned in source order, stored in the manifest, and never
reassigned - a pick made hours later resolves from the file rather than from
what the model remembers saying. Merging a list is concatenation, and a model
in that seat would only add a way to reword an entry or lose one.

The envelope is read for its own end, not for a closing fence. A body is
Markdown and may fence a diff or a status listing of its own, so the first
fence after the opening one is usually inside the answer rather than after it.
Each fenced block is decoded from its opening brace, a block that starts inside
one already read was quoted by it, and the last block left is the answer.

Every limit the reader holds a source to is in the prompt it was given: the
title, headline, label, value, body and offer lengths. A contribution dropped for a
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

A source killed at its deadline keeps what it had already written, which used to
be thrown away with the process: an eight-hour night that answered slowly left
nothing behind at all. If that partial output already holds the whole envelope -
the source finished its answer and only the process was late - it is read as the
answer.

Nothing a source does ends the run. The reader refuses every malformed answer
by name rather than raising, including one nested past the decoder's stack or
written in bytes that are not text; a source that finds a way past that is
caught at the fan-out and named; and a page that cannot be laid out leaves the
collected run standing, with the reason kept in the manifest.

## How a source runs

One bounded headless run in the source's own root, not a job:

|           | job                  | briefing source |
| --------- | -------------------- | --------------- |
| lives in  | a tmux pane          | one process     |
| owned by  | a foreground session | the run         |
| steerable | yes                  | no              |
| session   | restored by ID       | none            |

`pi --print --approve` or `claude --print --permission-mode bypassPermissions`.
That is an intent boundary and not a sandbox: a source that runs a refresh
command runs it with the owner's own hands, exactly as a review workspace does.
The project's guidance is what keeps it honest.

Each source gets exactly one turn. `--print` ends the process when the model's
turn ends, so anything a source dispatched and did not wait for dies with it.
`contribution_prompt` states that, because `harness_argv` is what makes it
true.

Both harnesses answer without asking on purpose. Nobody is watching a source
run, so a question it cannot ask is a refusal. A sandbox it cannot see decides
nothing but whether the morning is empty.

A source runs with every tool its harness has, the writing ones included, and
nothing here narrows that. There was a tool allowlist once and it was never the
thing it looked like: `bash` was always in it, so a source that meant to write
could always write, and the list only decided how awkwardly. Guidance is the
control, and the prompt says so in as many words - a source is told that it has
everything, that nothing is watching, and that its guidance is the whole of its
permission.

That puts the decision where the intent already lives. A source is asked to
change nothing and spend nothing unless its own guidance names it, and then only
what it names. A project whose morning is worth a refresh says so in its own
file: seedzero's briefing reads the channel through the API and writes the two
data files it just read, under a stated cap on how many reads that may cost.

The one exception is the second asking. A repair is handed the source's own
answer to say again correctly, so it reads nothing and runs nothing and is given
no tools at all.

Every source starts at once unless the profile caps it with `parallel`. Each is
bounded on its own at 900 seconds, and the whole run at 1800; both move with
`SCUFRIS_BRIEFING_SOURCE_DEADLINE` and `SCUFRIS_BRIEFING_DEADLINE`, which the
profile's `sourceDeadline` and `deadline` set. A source is held to whichever is
smaller, so raising one alone changes nothing. A second asking is bounded at 300
by `SCUFRIS_BRIEFING_REPAIR_DEADLINE`, never past what its source has left. One
project that hangs costs the run its own deadline and nothing else.

A profile's bounds reach a run through `$XDG_CONFIG_HOME/scufris/briefing-profiles.json`,
which Home Manager generates from the profile options. Every entry point reads
it, so a briefing asked for by hand is held to the same numbers as the one the
timer starts. The environment still wins where it is set, which is how a run
asking for a number on the command line keeps it. Without the file - an ordinary
checkout - the built-in defaults hold, and a file that cannot be read or parsed
reads as if it said nothing rather than refusing the morning. Home Manager
installs generated configuration as a symlink into the Nix store. The reader
allows that deployment shape, but checks the opened target is a bounded regular
file before reading it. A symlink to a device, FIFO, or oversized file is
refused without consuming its contents.

## The run directory

```text
$XDG_STATE_HOME/scufris/briefings/2026-08-31/
├── morning/
│   ├── .collect.lock          excludes a second collector for this run slot
│   ├── .publish.lock          serializes publication retries for the generation
│   ├── manifest.json          generation, owner, collection/delivery state,
│   │                          bounds, sources, offers, events, diagnostics
│   ├── contributions/*.json   one durable envelope for each source
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

A manifest is version 2. `generation` is an opaque identifier. `owner` is the
collector PID plus its Linux process start tick, so PID reuse cannot make an
abandoned run look live. Collection state is `collecting`, `collected`, or
`failed`; artifact delivery is independently `pending` or `prepared`.
`prepared` means `briefing.md` exists. It does not mean the conversation has
durably recorded the answer.

Each valid contribution is atomically written before the manifest publishes
its `source_finished` event. If the collector is killed after the write, an
external finalizer can recover that contribution instead of marking it lost.
The manifest event sequence moves only forward, terminal collection cannot be
reopened, and every write checks that its generation still owns the run.

The manifest's `bounds` are the source deadline, run deadline, and parallel
limit the run actually used. This makes a cutoff explainable and confirms that
a scheduled, tool-started, or shell-started run read the intended profile.

Every source is in the manifest from the moment the run starts, with the status
`asking` until it answers and its own entry written over that as it does. The
index used to be empty until the last source returned, which made an eight-hour
night in flight read exactly like a night nothing had declared. Only a
`collecting` manifest carries `asking`; a run that is over has one entry for
each source and nothing else.

The last thirty dates are kept, with every profile that ran on them. Collection
and delivery are two state machines:

| collection   | meaning                                       |
| ------------ | --------------------------------------------- |
| `collecting` | an identified owner is asking named sources   |
| `collected`  | at least one source answered, or none existed |
| `failed`     | every declared source failed                  |

| service delivery | meaning                                                               |
| ---------------- | --------------------------------------------------------------------- |
| `pending`        | the terminal wake is durably queued                                   |
| `in_progress`    | one correlated proactive model slot is reserved                       |
| `failed`         | the proactive circuit stopped it until a user turn or service restart |
| `delivered`      | the correlated answer is in canonical replay                          |

A failed run is delivered because the absence of a briefing is itself the
news: one source failing out of five was reported and five out of five used to
be silence. A run with no declared sources is terminal but creates no model
turn. Historical version-1 `delivered` manifests import as already delivered,
so an upgrade does not replay an old briefing.

A caller that names a date and no profile is resolved against what is on disk:
it means the one run that was gathered and is still waiting for its prose.
Two of those are two briefings, and it refuses rather than guesses between
them. `publish` resolves only to a waiting run, so a wake can put one
briefing's prose neither on another's page nor on one already delivered.
Reading a day where nothing is waiting resolves to the one run there is, and
also refuses to choose between two.

## The schedule

Nix owns when a briefing happens; each source owns what is in it.
`programs.scufris.agent.briefing.profiles` is an attribute set of profile name
to a schedule:

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
The runner records the generation outside the collection process before it
starts. Its `OnFailure` unit runs outside the failed collection cgroup, reads
systemd's measured result (including `oom-kill`), and finalizes only that exact
generation. Persisted contributions survive; unanswered sources become failed.
A delayed failure handler cannot finalize a newer run. The collector, failure
finalizer, and reconciler all use the same `MemoryHigh=3G` and `MemoryMax=4G`
cgroup bounds behind their bounded readers.

Collector announcements call `scufris-ctl briefing` after each durable event.
They are latency hints, not the authority. Session startup scans every retained
run, and `scufris-briefing-reconcile.timer` repeats the scan every minute. Thus
a stopped service, a missed filesystem event, or a machine restart cannot
strand terminal state. The agent extension holds no schedule or interval.

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

A briefing nothing declared is not an event. The run is recorded and the
foreground is never woken, so a schedule costs nothing until a source asks for
something.

`programs.scufris.agent.briefing.sources` is the sibling option that renders
`$XDG_CONFIG_HOME/scufris/config.toml` with `pkgs.formats.toml`. It is a
sibling and never a key inside a profile, because `profiles` carries the
timer's shape and nothing else. Build-time validation is the whole reason to
generate the file, so the known keys are declared over a freeform type: a
missing `guidance` or a nested keyword fails the build, and a key the reader
learns before the module does still renders.

## The night and the morning after it

Two profiles run on this machine, and one reads the other. `nightly` collects
at 23:00 with an eight-hour deadline; `scufris2` and `nova-protocol` each
declare a source for it that groups the day's commits, runs the project's own
review skill over one group at a time, and reports what is worth fixing. It
changes nothing. A source is owned by a schedule: it has no job record, no tmux
pane, no receipts, and nobody awake to steer or cancel it, so its commits would
be the only unverifiable ones in the system.

The durable half of a night is a tatr task in the reviewed project, opened
before anything is read and appended to as each group is adjudicated. The
envelope is only written when the source returns, so a night that is killed
returns nothing at all, and the task is what remains.

Each project's `morning` source then reads yesterday's `nightly` run - its
manifest, its own contribution, and the task named in the body - and reports
what the night found, what still stands, and what needs the owner. A finding
that survives that check becomes a numbered offer, so a fix is picked by number
and then run as a job in a Sprout worktree: a branch to read before it lands,
and a pane to watch.

## Writing and delivery

`control.briefing` carries one quiet row and, only on a terminal generation
with sources, one stable terminal wake. The service atomically stores both at
`$XDG_DATA_HOME/scufris/briefings.json` before it acknowledges the control
request. No connected agent is required. Replaying the same event ID is
idempotent.

Start and source-completion updates only replace `surface.briefings`; they never
enter Pi and never create speech or a conversation line. The desktop HUD and
iPhone draw one compact `BRIEF` drawer with profile, collection/delivery state,
count, and summary. The collapsed drawer shows all active runs and at most the
newest delivered row that needs attention. Expanding it shows every relevant
row in stable `since`, then generation-ID order. The rows have screen-reader
labels and narrow-width layouts.

When the terminal item is pending, the service waits until Pi is idle and no
user turn owns the response association. It marks the item `in_progress` before
sending `agent.wake` with its stable `proactive_id`. While that slot is held, a
new surface message is refused and retained in the surface's composer rather
than being captured by the briefing. The extension stores `proactive_id` on the
exact custom message Pi queued, captures it when Pi delivers that message,
sends `agent.proactive_started`, and adds the ID only to that turn's atomic
`agent.response`. It then sends `agent.proactive_settled` on the same ordered
socket. If Pi settles before `message_end`, the extension settles the exact ID
it put in the now-empty follow-up queue; the host releases and retries that
slot. The extension also deduplicates a host redelivery while the same ID is
still queued across a socket reconnect. An unrelated follow-up cannot
acknowledge the item.

Scufris reads the run, writes one briefing in its own voice from measured
contributions, publishes the same prose into the artifact, and returns the
correlated response. The service first records that response in canonical
conversation replay with its delivery ID. Only then does it mark the inbox item
`delivered`. This is the acknowledgment boundary. A crash before the replay
write retries the pending item; a crash after the replay write recovers it as
delivered before an agent can connect, so no second visible answer is made.
Duplicate or stale correlated responses are ignored. Publication is also
idempotent. The first publish fixes `briefing.md`; the same prose is a no-op or
repairs an interruption between the prose, manifest, and page writes. A retry
that proposes different prose keeps and returns the generation's fixed prose so
the response can use the canonical words. `.publish.lock` is taken without
waiting and serializes concurrent retries; a missing or symlinked run is refused
before that lock file is created.

Consecutive proactive turns use bounded exponential backoff. The count resets
when the queue stays empty through that backoff or a surface opens a user turn.
After three starts, another generation for a date/profile pair already seen in
the sequence opens the circuit. A backlog of distinct dates or profiles drains;
a producer repeatedly minting one logical run stops. All queued rows become
`failed`, retain their measured summaries, and show recovery instructions. They
cannot be dismissed. A surface user turn or service restart returns their
durable wakes to `pending`.

A delivered success then disappears from the drawer. A delivered run needs
attention only if collection is `failed`, or if collection is `collected` with
the measured source count `failed > 0`. The latter is the complete definition
of partial; no summary or prose is classified. Failed and partial rows stay
visible until a person sends `briefing.dismiss` with the opaque generation ID.
The service allows dismissal only after terminal collection and delivered
response, stores it atomically, and publishes the resulting whole list to every
surface. Repeating it is harmless.

Dismissal is presentation state, not delivery acknowledgment or deletion. The
service keeps the canonical response, collection artifacts, and row in its
bounded audit. State format 2 stores up to 64 audit rows and a bounded set of
dismissed IDs; format 1 loads with no dismissals. A format-2 file written under
the original 128-row limit is compacted rather than rejected on upgrade. Active
and undismissed delivered attention rows never yield to ordinary eviction. The
oldest delivered success,
dismissed attention row, or circuit-stopped row yields first when audit space
is needed, so stopped deliveries cannot fill the store and block new ingress.

Filesystem state remains the collection authority and service state remains
the delivery authority. Preparing `briefing.md` does not acknowledge delivery,
a service acknowledgment does not rewrite collection state, and dismissal
does neither.

Everything before the prose is code. The schedule, the sources, the runs and
the record are decided by systemd and `briefing.py`, and no model is asked
whether a briefing should happen. Scufris adds the prose on top of a run that
already exists and already has a page.

The page never opens by itself. Nothing in the stack answers "is the owner at
the desk": the service tracks registered surfaces, but no presence field
reaches the agent, and a graphical session is there whether or not anyone is
home. Chat reaches every surface; the page opens when it is asked for.

### Nobody asked for a briefing

The service associates an answer with the surface that asked for it, for as
long as that turn is open. A briefing closes no turn: nobody asked for it. So
it is recorded against the reserved surface name `unprompted`, and every
surface shows it while none of them matches it: nothing is spoken aloud, and no
live widget call runs.

That holds whether or not the owner has ever spoken, and whichever surface he
used last. The phone he answered from at midnight neither speaks the morning
briefing nor swallows it by being switched off, because the turn it opened was
closed by its own answer hours earlier.

This is not particular to briefings. Every proactive wake shares it, a finished
job included. `an_unprompted_response_is_shown_by_every_surface_and_spoken_by_none`
and `a_wake_owns_its_answer_only_when_no_owner_turn_is_open` in
`host/service/src/service.rs` hold the behavior, and no surface may register
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
scufris-briefing sources --config ./config.toml
scufris-briefing collect --profile morning
scufris-briefing wake --profile morning
scufris-briefing reconcile --json
scufris-briefing finalize --profile morning --generation ID --cause "systemd result: oom-kill"
scufris-briefing pending --json
scufris-briefing show --json
scufris-briefing publish < prose.md
scufris-briefing open --date 2026-08-30 --profile morning
```

Every subcommand takes `--profile` and `--config`. `wake` replays one terminal
row into the durable service inbox. `reconcile` scans all retained runs, and
`finalize --generation ... --cause ...` closes one dead collector without ever
touching a newer generation. `pending` remains a local artifact query for runs
whose prose has not been prepared.

`tools/briefing/page.py` renders and asks nothing: given a finished run it
writes the same page a year from now. It uses markdown-it-py's CommonMark
parser and table rule for every Markdown field. Raw HTML stays visible as text;
only credential-free HTTP and HTTPS destinations with a host become links;
and image syntax keeps its alt text without fetching a resource. The page
carries its own styling, so it opens from a state directory with no server, no
fonts to fetch and no script to run.

---

Next: [Tmux](tmux.md)
