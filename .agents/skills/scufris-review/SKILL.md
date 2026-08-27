---
name: scufris-review
description: Review a Scufris change with a panel of parallel reviewer agents. Use only when the user requests Scufris Review.
disable-model-invocation: true
---

# Scufris Review

Dispatch parallel reviewer agents over one change range, then adjudicate
their findings in this session.

The invocation IS the request to dispatch subagents. The standing "no
subagents unless the user asks" directive does not apply inside this
skill.

## Resolve the range first

Never fan out over an unresolved range.

- No argument: the range this session produced. Name it, state it,
  continue. Ask only when the session cannot name it.
- `<base>..<head>`: that range.
- `--task <id>`: the commits named in `tasks/<id>/TASK.md`.
- `--worktree`: uncommitted changes, tracked and untracked.

Stop above 10000 changed lines and offer a narrower range or a
commit-by-commit pass. `origin/master..HEAD` and the last release tag
are both too wide to be a default.

One increment of one task is the unit this is written for, and one of
those runs to several thousand lines. The cap is there for a range
nobody meant to ask for, not for a big change.

## Build the bundle once

Write the evidence to the scratchpad and give every lane the same
paths. A lane must not re-derive the range.

```bash
git log --oneline <range>
git diff --stat <range>
git diff <range>
```

Add the task body when the range belongs to a task.

## Dispatch the lanes

Send every lane in ONE message so they run concurrently. Give each the
range, the bundle paths, and two repo-relative brief paths to read: the
shared `.agents/skills/scufris-review/lanes/reviewer.md`, and its own.

| Lane        | Brief                  | When     |
| ----------- | ---------------------- | -------- |
| Craft       | `lanes/craft.md`       | always   |
| Correctness | `lanes/correctness.md` | always   |
| Desktop     | `lanes/desktop.md`     | always   |
| Contracts   | `lanes/contracts.md`   | always   |
| Red team    | `lanes/red-team.md`    | always   |
| Feel        | `lanes/feel.md`        | `--live` |

- Reviewers are read-only. They report; they never edit, stage, commit,
  or fix.
- One lane owns the X display at a time. Desktop holds the display
  slot; red team and feel wait for it. Two harnesses on one machine
  fight over sockets, displays, and the keyboard they are measuring.
- No lane runs `nix flake check`.

## Adjudicate

Do this in this session, not in a lane.

- Drop every finding that is not grounded in the diff or the tree. A
  plausible smell is not a finding.
- Merge duplicates across lanes and keep the strongest evidence.
- Rank `BLOCKER`, `MAJOR`, `MINOR`. Record why a finding was not raised
  higher.
- Re-derive a load-bearing claim yourself before it reaches the
  verdict.

Budget for this. It is where the value is and it is not cheap: in the
first real run three lanes were wrong on the facts and one blocker had
to be ranked down, and re-deriving each load-bearing claim from the
tree took longer than dispatching the panel did.

## Report

Give the verdict, the findings by severity, what was verified, and what
was skipped. Name each skipped check; a skip is not a pass.

Write `tasks/<id>/REVIEW.md` when the range belongs to a task: round,
reviewer, verdict, findings, verified, proofs rerun, verdict rationale.
Otherwise report inline and write no file.

Fixing is a separate step. Change nothing until the user picks the
findings to act on.
