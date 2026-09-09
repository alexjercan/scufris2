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
run, so a question it cannot ask is a refusal. Under `claude`'s `dontAsk` the
shell is sandboxed, and a source told to read CI or refresh its numbers spent
its run reporting that `gh` or `python3` had been denied. A sandbox it cannot
see decides nothing but whether the morning is empty.

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
reads as if it said nothing rather than refusing the morning.

## The run directory

```text
$XDG_STATE_HOME/scufris/briefings/2026-08-31/
├── morning/
│   ├── manifest.json          the index: state, the bounds the run was given,
│   │                          every source, the numbered offers, every
│   │                          diagnostic
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

The manifest's `bounds` are the numbers that run was actually given: the source
deadline, the run deadline, and how many sources were asked at once. A profile's
own numbers live in its timer unit's environment, so a run started any other way

- the `scufris_briefing_run` tool under the service, or a shell - gets the code
  defaults instead. Recording them is what makes a source cut off at fifteen
  minutes distinguishable from one cut off at the eight hours the profile asks
  for.

Every source is in the manifest from the moment the run starts, with the status
`asking` until it answers and its own entry written over that as it does. The
index used to be empty until the last source returned, which made an eight-hour
night in flight read exactly like a night nothing had declared. Only a
`collecting` manifest carries `asking`; a run that is over has one entry for
each source and nothing else.

The last thirty dates are kept, with every profile that ran on them. The
manifest state is the record of what happened, and it is what an opening
session reads:

| state        | what it means                  | what a session does |
| ------------ | ------------------------------ | ------------------- |
| `none`       | nothing was started            | nothing             |
| `collecting` | a run no process owns any more | nothing             |
| `collected`  | gathered, prose never written  | ask for the writing |
| `delivered`  | the owner has it               | nothing             |
| `failed`     | nothing answered               | say there is none   |

`collected` and `failed` ask a session for something, and only when the run has
sources and no prose beside it. A failed run is asked about because the absence
of a briefing is itself the news: one source failing out of five was reported
and five out of five used to be silence. It closes the same way a collected run
closes, by Scufris publishing the prose - which for a failed run is the
sentence saying there is none. Everything else is the timer's business: a
session never decides that a briefing is owed, so it can neither deliver one
twice nor collect one nobody asked for.

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
scufris-briefing pending --json
scufris-briefing show --json
scufris-briefing publish < prose.md
scufris-briefing open --date 2026-08-30 --profile morning
```

Every subcommand takes `--profile` and `--config`. `wake` is what the timer
runs after a collection: it carries the run to the conversation and reports
rather than fails when nothing is listening. `pending` is what an opening
session reads.

`tools/briefing/page.py` renders and asks nothing: given a finished run it
writes the same page a year from now. It uses markdown-it-py's CommonMark
parser and table rule for every Markdown field. Raw HTML stays visible as text;
only credential-free HTTP and HTTPS destinations with a host become links;
and image syntax keeps its alt text without fetching a resource. The page
carries its own styling, so it opens from a state directory with no server, no
fonts to fetch and no script to run.

---

Next: [Tmux](tmux.md)
