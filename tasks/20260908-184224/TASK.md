# Review the day's commits nightly and fix what is worth fixing

- STATUS: OPEN
- PRIORITY: 90
- TAGS: workflow,review,schedule

## Goal

Every night at 23:00, scufris2 and nova-protocol review the day's own commits
and fix what is worth fixing. One job for each repository, working on master.
The night is long and narrow on purpose: groups are reviewed one after another
rather than all at once, so eight hours costs a handful of agents rather than a
crowd.

The morning briefing says what the night did, what it left, and what needs
Alex. A leftover becomes a numbered offer he can start by number.

Supersedes `20260908-140011`, which declared a read-only nightly briefing
source in nova. A briefing source reports and cannot repair, and it is bounded
at 900 seconds. This does the work as a job and lets the morning report it.

## Facts

- Both review skills dispatch read-only lanes and adjudicate in the calling
  session: "Reviewers are read-only. They report; they never edit, stage,
  commit, or fix" (`.agents/skills/scufris-review/SKILL.md`,
  `~/personal/nova-protocol/.agents/skills/nova-review/SKILL.md`). Review and
  repair are already two phases.
- Scufris has five lanes, always on. Nova has four always plus Red team and
  Feel under `--play`, and those want a rendered run through `probe`'s
  throwaway X server (`nova-review/lanes/feel.md`).
- Nova serialises its own measurement: "One lane measures at a time.
  Performance holds the measurement slot; red team and feel wait for it. A
  shared GPU turns a 93ms frame into 291ms."
- Nova refuses a range above 2000 changed lines; scufris above 10000. A day is
  routinely 18 commits in each repository, so grouping is required and not a
  refinement.
- A job is a tmux pane on the server the foreground session shares
  (`tools/jobs/scufris-jobs:350-362`). `spawn` is in the helper's dispatch
  table (`:3794`), so a timer starts a job without the agent being awake.
- `WORKSPACES` includes `project`, which is the checkout itself
  (`scufris-jobs:41`). Working on master means `project`, not `sprout`.
- `agent/skills/` is copied to `share/scufris/skills` (`nix/resources.nix:10`),
  so a skill there reaches every deployment.
- The machine is a desktop and does not sleep, but `Linger=no`: the user
  manager stops at logout, and a night in flight dies with it.

## Direction

### When

- A `scufris-nightly-review` timer at 23:00, built the way the briefing timers
  are. Nix owns the schedule and which repositories are in it; nothing about
  the method lives there.
- Not a briefing profile. A briefing collects reports inside 1800 seconds; this
  works for hours and commits. Sharing the unit generator is fine; sharing the
  concept is not.

### What starts

- Exactly two jobs, one for each repository, in the `project` workspace on
  master. This is the whole of the parallelism the schedule decides. Each job
  may fan out lanes inside a group, as its skill intends.
- A `nightly-review` skill in `agent/skills/` carries the method, so the spawn
  instruction stays short and the prose is versioned with the code.

### The task is the state

- The job's first act is `tatr new` in its own repository. Every group, every
  finding, every decision is appended to that `TASK.md` as it happens.
- Appended as it happens, not written at the end. Compaction is expected over
  eight hours, and so is a logout that kills the night at 03:00. A half-written
  task is a readable night; a task written at the end is nothing.

### The order

1. **Group.** Read the day's commits and put them in groups that are worth
   reviewing together. Write the groups and their ranges into the task before
   reviewing any of them.
2. **Review, one group at a time.** Invoke the repository's own review skill on
   that group's range. Append its adjudicated findings to the task. Then the
   next group. This is the batching, and it is what keeps the night narrow.
3. **Fix, only when every group is reviewed.** Repairing from the whole night's
   findings beats repairing from the first group's.

### Fixing

- Commits go straight to master. Alex asked for this directly after being told
  it is the largest unattended surface in the system; it is his decision and it
  is recorded here so nobody re-opens it as an oversight.
- Fix what the findings support and the tests can show. Leave anything that
  needs a judgement Alex has not made, and say so by name rather than
  attempting it.
- Run the repository's own checks before each commit. A night that leaves
  master red is worse than a night that fixed nothing.

### `--play`

- Decided by the group, not by the schedule. A group that touches rendering,
  UI or feel gets it; a group of protocol, docs or benchmark work does not.
- The job says in the task which groups got it and why, so the choice is
  reviewable in the morning.

### The morning

- Each repository's `[briefings.morning]` guidance reads the night's task and
  reports what was fixed, what was left, and what needs Alex.
- A leftover becomes an offer, so it arrives numbered and can be started by
  number. That is the feature that landed today and this is its first real use.
- A night that did not run, or died partway, is reported as that. An absent
  task is not a quiet morning.

## Verification

- One real night in each repository that produces a task with groups, findings,
  and a fix phase that ran after the last group.
- A night where a group is refused for size, and the job narrows it rather than
  giving up on the group.
- A night killed partway leaves a task that says how far it got.
- One morning briefing that reports a night, with leftovers as numbered offers.
- One morning where no night ran, reported as that rather than as silence.
- Nova gets `--play` on a rendering group and not on a docs group, in one night.

## Risks

- `Linger=no` means a logout mid-night kills the job. Either
  `loginctl enable-linger` becomes part of this, or a dead night is accepted
  and made legible by the append-as-you-go task. Decide before the first run.
- Peak concurrency is two jobs times a group's lanes. Narrow over the night,
  wide for the minutes a group is under review.
- Two unattended agents committing to master on two repositories is the largest
  unattended surface in the system. The receipts machinery is what makes its
  claims checkable; use it.
