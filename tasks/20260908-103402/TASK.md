# Add landing receipts: measured git, remote, and CI facts for every job

- STATUS: OPEN
- PRIORITY: 100
- TAGS: workflow


## Goal

Every completion claim about git, remotes, CI, and tags that Scufris repeats
is a fact the helper measured. When the receipt says the work is not landed,
Scufris says "not landed" in those words. A branch that was not landed is
never deleted, so work is never lost.

Origin: research task 20260908-003207, candidate 1. Round 2 removed the
worker boundary: no new worker commands, workers keep the shell they have.

## Facts

- Nothing in `tools`, `scripts`, `agent`, or `.claude/skills` runs
  `git push`, `git ls-remote`, `gh run`, `gh pr`, `gh release`, or
  `git tag`.
- The `done` report is worker prose end to end:
  `agent/extensions/scufris/workflow/worker-report.ts:33-78`,
  `tools/jobs/scufris-report:12-63`, helper `report` at
  `tools/jobs/scufris-jobs:2111`, wake at
  `agent/extensions/scufris/workflow/orchestration.ts:229-245`.
- Helper `land` already checks ancestry (`tools/jobs/scufris-jobs:2725-2741`)
  and records the landed revision (`:2746-2752`).
- `sprout rm` runs `git worktree remove` then `git branch -D` with no
  merged check. `scufris_job_stop` with `remove_workspace` reaches it
  (`orchestration.ts:1246-1270`, helper `:2763-2790`).
- Landed, pushed, and released are three facts. Pushing master and the tag is
  manual (`RELEASE.md:23-25`); the release workflow runs on a tag push only.

## Direction

- Add a helper `receipt` command that measures, for one job: landed
  revision and `merge-base --is-ancestor` against the base branch; feature
  branch and worktree existence; `origin/<base>` ahead and behind after a
  fetch (`git rev-list --left-right --count`); tag presence local and
  remote (`git ls-remote --tags`); the CI run for the commit
  (`gh run list --commit <sha> --json status,conclusion,url`). Write
  `receipt.json` beside the job. Fail closed on any command that cannot run
  and record the reason as a field, never as prose.
- Return the receipt from `land`, `stop`, and `inspect`. Show it in
  `scripts/scufris-jobs` output.
- Stamp `HEAD`, branch, and `git status --porcelain` into the status line
  at report time so a report is anchored to a revision.
- Foreground policy: quote receipt fields verbatim. A worker claim of push,
  merge, or release with no matching field is said as "claimed, not
  verified". "Not landed" is the sentence when the receipt says so.
- `sprout rm` refuses an unmerged branch without a force flag.
  Stop-with-removal passes force only when the request says abandon.

## Verification

- Integration test: a job whose worker reports "pushed and released" with no
  push produces a receipt with `pushed=false`, `release_run=null`, and the
  foreground text contains "claimed, not verified".
- Test: `sprout rm` on an unmerged branch exits nonzero without force.
- `python3 -m unittest discover -s tests -p 'test_*.py'` and `npm run check`.
