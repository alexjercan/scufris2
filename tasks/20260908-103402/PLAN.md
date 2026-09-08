# Plan: landing receipts

Plan for task `20260908-103402`. Agreed with Alex on 2026-09-08.

## Scope

Three of the four parts of the task, in this repository:

1. Measure. A helper `receipt` command measures git, remote, and CI facts for
   one job and appends them to `receipts.jsonl` beside the job.
2. Return. `land`, `stop`, and `inspect` return the receipt.
   `scripts/scufris-jobs` shows it. The extension measures on a terminal
   event so a wake carries the receipt.
3. Protect, this side. `remove_sprout_workspace` passes a force flag only for
   an explicit abandon, and probes whether the installed `sprout` accepts it.

Part 4 in `sprout` is a parallel change in `~/personal/nix.dotfiles`
(`home/modules/scripts/sprout.sh`, `cmd_rm` at line 485). It is not in this
plan. This repository must keep working against a `sprout` that does not know
the flag yet.

## Dropped: the status line stamp

The task asked to stamp `HEAD`, branch, and `git status --porcelain` into the
status line at report time. Dropped, by decision, for two reasons.

The `status` file is one JSON record per event and its value is that a person
can read it. The stamp takes a 95 character line to 215, on every line
including `working` chatter, and `inspect` renders events as
`g<n> <event>: <summary>`, so the stamp would not even appear there.

The fact survives without it. The extension measures the receipt when the
terminal event arrives, while the worktree still exists, so `head`, `branch`,
and `dirty` are captured at report time in `receipts.jsonl` instead. No change
to a durable format that four functions already parse.

## Decisions

- Facts are measured or absent. `false` means measured and false. A fact that
  could not be measured is `null` and its reason is recorded in `unavailable`.
  A failed fetch must never read as "not pushed".
- `receipts.jsonl` is append-only. The done-time receipt is the evidence that
  a worker claimed a push that did not happen; a later land-time measurement
  must not overwrite it.
- `receipt` measures. `inspect` reads. Listing jobs calls `inspect` once per
  job, so measurement there would mean one network fetch per row.
- `receipt` is not in the helper's `mutating` set. A network fetch inside the
  global workflow lock would stall every other job. It appends atomically
  instead.
- The claim check is Python, in the helper, beside the facts it compares. One
  implementation serves the extension and `scripts/scufris-jobs`.
- `git` and `gh` stay on PATH. Fail-closed handling covers absence, so neither
  goes into `nix/launcher.nix` runtimeInputs.

## The receipt

One JSON object per line in `<job_dir>/receipts.jsonl`. A job that landed and
was never pushed, whose worker said it had been:

```json
{
  "job_id": "01639a63cdc6",
  "root_job": "01639a63cdc6",
  "measured_at": "2026-09-08T10:41:02Z",
  "trigger": "done",
  "workspace": "sprout",
  "project_root": "/home/alex/personal/scufris2",
  "base_branch": "master",
  "feature": "briefing-notification",
  "commit": "9f2c1ab4e77d0c3b1a5e6f8290d4c7b1e3a20d95",
  "facts": {
    "head": "9f2c1ab4e77d0c3b1a5e6f8290d4c7b1e3a20d95",
    "dirty": false,
    "landed": true,
    "landed_revision": "9f2c1ab4e77d0c3b1a5e6f8290d4c7b1e3a20d95",
    "branch_exists": false,
    "worktree_exists": false,
    "remote": "origin",
    "pushed": false,
    "ahead": 3,
    "behind": 0,
    "tags_local": [],
    "tags_remote": [],
    "release_run": null
  },
  "claims": [
    {
      "claim": "pushed",
      "said": "pushed and released to origin",
      "field": "pushed",
      "measured": false,
      "verdict": "claimed, not verified"
    }
  ],
  "unavailable": {}
}
```

Degraded measurement records the reason as a field, never as prose:

```json
  "facts": {"pushed": null, "release_run": null},
  "unavailable": {
    "pushed": "git fetch origin failed: could not resolve host github.com",
    "release_run": "gh is not on PATH"
  }
```

### How each fact is measured

| Fact              | Command                                                                                            |
| ----------------- | -------------------------------------------------------------------------------------------------- |
| `head`, `dirty`   | `git rev-parse HEAD`, `git status --porcelain`, in the worktree while it exists                    |
| `landed`          | `git merge-base --is-ancestor <commit> refs/heads/<base>`                                          |
| `branch_exists`   | `git show-ref --verify refs/heads/<feature>`                                                       |
| `worktree_exists` | `git worktree list --porcelain`                                                                    |
| `pushed`          | `git fetch <remote> <base>`, then `merge-base --is-ancestor <commit> refs/remotes/<remote>/<base>` |
| `ahead`, `behind` | `git rev-list --left-right --count refs/heads/<base>...refs/remotes/<remote>/<base>`               |
| `tags_local`      | `git tag --points-at <commit>`                                                                     |
| `tags_remote`     | `git ls-remote --tags <remote>`                                                                    |
| `release_run`     | `gh run list --commit <commit> --json status,conclusion,url`                                       |

`commit` is the landed revision when the job landed, and the feature branch tip
otherwise. The fetch updates remote-tracking refs in the real checkout. Nothing
here touches the working tree, a local branch, or the index.

Every git and `gh` call uses a 15 second timeout, not the helper's 120 second
default, because the extension runs this inside a wake. A timeout is recorded
in `unavailable` like any other failure.

### The four workspace kinds

- `sprout`: the full receipt above.
- `project`: no feature branch and no worktree. Measures `head`, `dirty`,
  `pushed`, `ahead`, `behind`, and tags against the project checkout.
- `review`: returns the source job's receipt instead of measuring its own,
  because a reviewer shares the source workspace.
- `temporary`: no git at all. One record with `workspace: "temporary"` and
  the reason in `unavailable`, not a wall of nulls.

### The claim check

The `claims` block is derived, not measured, and stays separate from `facts`
for that reason. It reads the newest `report.md` entry, matches claim words
for push, merge or land, release, and tag, and compares each against its
field.

- Measured and true: no entry. The claim is verified.
- Measured and false, or `null`: `verdict` is `claimed, not verified`.
- `landed` false produces the sentence `not landed` in those words.

`SKILL.md` then tells foreground Scufris to quote the verdict, rather than
asking the model to do the comparison itself.

## Build order

Each step is checkable on its own.

1. Measurement core and the `receipt` command in `tools/jobs/scufris-jobs`.
   New handler, registered in `main()` (`:2958`) outside the `mutating` set.
   Reuses `run()` (`:298`), `load_job()` (`:1478`), `canonical_path()`
   (`:320`), and `atomic_write()` (`:272`). New bound for the receipts file.
2. The claim check, in the same file, over `report.md`.
3. Return the receipt from `land` (`:2689`), `stop` (`:2763`), and `inspect`
   (`:2368`). `land` and `stop` re-measure after they act. `inspect` reads the
   newest stored record and never measures.
4. `scripts/scufris-jobs`: a receipt block in `print_detail` (`:225`). No new
   table column; `--json` passes the field through already.
5. `agent/extensions/scufris/workflow/orchestration.ts`: call `receipt` in the
   drain loop before `deliverWorkerEvent` (`:653`) when the event type is
   terminal, and put the facts and claims into the wake `details` (`:222-250`).
   A helper failure degrades to no receipt and must never break the drain loop.
   `CleanupResult` (`:459`) gains the receipt.
6. `agent/skills/workflow/SKILL.md`: the foreground policy. Quote receipt
   fields verbatim. Say `claimed, not verified` for an unbacked claim. Say
   `not landed` in those words.
7. `remove_sprout_workspace` (`:2592`) and `cleanup_workflow` (`:2660`): pass
   force only when the stop request says abandon, and probe `sprout rm` for
   flag support first so an old `sprout` still works.
8. `docs/src/dev/jobs.md`: a Receipts section, and edits to Reporting and to
   Land, stop, and archive. `CHANGELOG.md`: an Unreleased entry.

## Verification

Tests in `tests/test_scufris_jobs.py`, in the existing `ReplacementJobsTest`
fixture, which already builds a real git project and a fake harness:

- A job whose worker reports "pushed and released" with no push produces a
  receipt with `pushed=false`, `release_run=null`, and a claim whose verdict is
  `claimed, not verified`.
- An unlanded job produces `landed=false` and the sentence `not landed`.
- A failed fetch produces `pushed=null` with a reason in `unavailable`, and
  never `pushed=false`.
- Absent `gh` produces `release_run=null` with a reason, and the rest of the
  receipt is still measured.
- Each of the four workspace kinds produces its documented shape.
- `receipts.jsonl` keeps the done-time record after a later land-time
  measurement appends to it.
- Stop with removal refuses an unmerged branch unless the request says
  abandon, and is skipped when the installed `sprout` has no force flag.

Commands:

```bash
python3 -m unittest discover -s tests -p 'test_*.py'
npm run check
nix flake check
```

## Out of scope

- The status line stamp. Dropped above.
- `sprout rm` itself. Parallel work in `nix.dotfiles`.
- The scheduler, the verify agent, and the HUD. Tasks `20260908-103403`,
  `20260908-103404`, and `20260908-103406`, in that order, after this one.
