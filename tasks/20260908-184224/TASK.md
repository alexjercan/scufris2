# Review the day's commits nightly and fix what is worth fixing

- STATUS: OPEN
- PRIORITY: 90
- TAGS: workflow,review,schedule

## Goal

A `nightly` briefing profile at 23:00, built and deployed exactly as `morning`
is: a schedule in Nix, a source declared by each project in its own
`.scufris.toml`, one run directory, the same envelope, the same page. It
reviews instead of summarising, and it fixes what it finds.

The morning reads the night's run and reports what was fixed, what was left,
and what needs Alex. A leftover arrives as a numbered offer he can start by
number.

Supersedes `20260908-140011`, which declared a read-only nightly source. This
keeps that shape and gives the source the tools to repair.

## Facts

### The profile machinery is already generic

- A profile is `schedule`, `persistent` and `deadline`
  (`nix/home-manager.nix:141-181`). Nothing in it names morning.
- `deadline` is `ints.positive` and reaches the run as
  `SCUFRIS_BRIEFING_DEADLINE` (`nix/briefing-unit.nix:38-40`), so 28800 is
  expressible today.
- `TimeoutStartSec = profile.deadline + 300` (`home-manager.nix:549`), so an
  eight-hour deadline already gets an eight-hour unit. systemd never kills a
  run halfway.
- `max_workers=len(sources)` (`briefing.py:939`). Two repositories declaring a
  `nightly` source is two agents and no more. The parallelism Alex asked to cap
  is the source count, and it caps itself.
- Every source's run directory, manifest and contribution files are per
  profile (`briefing.py:144-150`), so a night and a morning never collide.

### Three flags forbid the method

A briefing source is invoked one-shot with, for `claude`
(`briefing.py:497-543`):

- `--disallowed-tools Edit,Write,NotebookEdit,Task`. No repair, and no `Task`
  means no lanes: both review skills dispatch read-only lanes and adjudicate
  in the calling session.
- `--disable-slash-commands`. `/scufris-review` and `/nova-review` cannot be
  invoked at all.
- `PI_TOOLS` / `CLAUDE_TOOLS` are module constants, not per source.

These are the whole of the gap. They are stated as policy, not as a sandbox:
"The edit tools are off because nothing here asks for a change, not because
this is a sandbox" (`briefing.py:94-97`).

### Bounds

- `SOURCE_DEADLINE = 900` is a constant the unit never exports. Only the run
  deadline is per profile (`briefing.py:83`, `briefing-unit.nix:38-40`).
- Both review skills refuse a range that is too large: nova above 2000 changed
  lines, scufris above 10000. A day is routinely 18 commits in each
  repository, so grouping is required and not a refinement.
- Nova serialises its own measurement: performance holds the slot, red team
  and feel wait for it. A shared GPU turns a 93ms frame into 291ms.
- The machine is a desktop and does not sleep, but `Linger=no`: the user
  manager stops at logout and a night in flight dies with it.

## Direction

### What is built here

1. **`sourceDeadline` on the profile submodule**, exported as
   `SCUFRIS_BRIEFING_SOURCE_DEADLINE` beside the run deadline. A night needs
   both raised; today only one can be.
2. **A per-source tool policy**, declared in `.scufris.toml` beside `harness`,
   `model` and `thinking`, where the freeform keywords already live. A source
   says what it may do; the constants become the default a source that says
   nothing gets. This is the decision the rest depends on: it inverts "a
   source reads its project and reports" into a per-profile claim, and it is
   what Alex asked for when he asked for a review that fixes.
3. Nothing else. No new timer, no job, no second concept.

### What is configured

- `nix.dotfiles` gets `profiles.nightly = { schedule = "23:00"; deadline =
  28800; sourceDeadline = 28800; }`, next to `profiles.morning.schedule =
  "08:00"`. That is the whole of the schedule, and it is the only place 23:00
  is written.
- `scufris2` and `nova-protocol` each get `[briefings.nightly]` in their own
  `.scufris.toml`, carrying the method as guidance. A repository that wants no
  night declares no source and costs the schedule nothing.

### What the guidance says

1. **Group.** Read the day's commits and put them in groups worth reviewing
   together. Write the groups and their ranges into a tatr task before
   reviewing any of them.
2. **Review, one group at a time.** Invoke the repository's own review skill on
   that group's range. Append its adjudicated findings to the task. Then the
   next group. This is the batching, and it is what keeps the night narrow.
3. **Fix, only when every group is reviewed.** Repairing from the whole night's
   findings beats repairing from the first group's.

### The task is the durable half

- The source's first act is `tatr new` in its own repository, and every group,
  finding and decision is appended as it happens.
- Appended as it happens, not written at the end. The envelope is only written
  when the source returns, so a night killed at 03:00 by a logout returns
  nothing. A half-written task is a readable night; a task written at the end
  is nothing.

### Fixing

- Commits go straight to master. Alex asked for this directly after being told
  it is the largest unattended surface in the system; it is his decision and it
  is recorded here so nobody re-opens it as an oversight.
- Fix what the findings support and the tests can show. Leave anything that
  needs a judgement Alex has not made, and name it rather than attempt it.
- Run the repository's own checks before each commit. A night that leaves
  master red is worse than a night that fixed nothing.

### `--play`

- Decided by the group, not by the schedule. A group that touches rendering, UI
  or feel gets it; protocol, docs or benchmark work does not.
- The source says in the task which groups got it and why, so the choice is
  reviewable in the morning.

### The morning

- Each repository's `[briefings.morning]` guidance reads the night's run at
  `briefings/<date>/nightly/` and its tatr task, and reports what was fixed,
  what was left, and what needs Alex.
- A leftover becomes an offer, so it arrives numbered and can be started by
  number. That is the feature that landed today and this is its first real use.
- A night that did not run, or died partway, is reported as that. An absent
  run is not a quiet morning.

## Verification

- One real night in each repository that produces a task with groups, findings,
  and a fix phase that ran after the last group.
- A night where a group is refused for size, and the source narrows it rather
  than giving up on the group.
- A night killed partway leaves a task that says how far it got.
- One morning briefing that reports a night, with leftovers as numbered offers.
- One morning where no night ran, reported as that rather than as silence.
- Nova gets `--play` on a rendering group and not on a docs group, in one
  night.
- A source that declares no tool policy is invoked exactly as today. The
  morning does not change.

## Risks

- A per-source tool policy means a `.scufris.toml` in any discovered project
  can ask for the edit tools. The project roots are Alex's own checkouts and
  this is the same trust a job already gets, but it is a new thing for a file
  to be able to say.
- `Linger=no` means a logout mid-night kills the run. Either
  `loginctl enable-linger` becomes part of this, or a dead night is accepted
  and made legible by the append-as-you-go task. Decide before the first run.
- Peak concurrency is two sources times a group's lanes. Narrow over the night,
  wide for the minutes a group is under review.
- Two unattended agents committing to master on two repositories is the largest
  unattended surface in the system. The receipts machinery is what makes its
  claims checkable; use it.

## Rejected

- **A separate `scufris-nightly-review` timer that spawns two jobs.** What this
  task said before. It is a second scheduling concept, a second unit generator
  and a second run record, for something the profile machinery already
  expresses. A job buys a steerable tmux session that nobody is awake to steer.
- **A read-only nightly source** (`20260908-140011`). Reports candidates it is
  not allowed to act on, which is half a night.
