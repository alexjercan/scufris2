# Review Scufris master for what blocks or slows its use

- STATUS: OPEN
- PRIORITY: 80
- TAGS: review

## Scope

A standing review of Scufris as it stands on `master` at `db32817`, not a
review of one change. No range, no changed-line budget, no `--live` lane and
no X display.

Alex asked for it after two findings on 2026-09-08 that were the same shape:
the machinery and the thing driving it disagreed about what was possible, and
he paid for it.

- The foreground acknowledgment gate blocked every tool except the final
  response after a successful action. He asked for a job, said "open the
  brief" while it started, and got "I still need to open today's brief in a
  separate action". Fixed in `db32817`.
- A briefing source ended its one-shot `claude --print` turn with "I'll wait
  for the two remaining G1 lanes". The lanes died with the process and 1676
  seconds produced nothing. Fixed by guidance in `315b931`.

What this review is for: more of that shape. Things that block Scufris or
make it awkward to use, ranked by what they cost him rather than by how
interesting they are.

## Method

Two reviewer agents at a time, over eight components. Every agent reads
`.agents/skills/scufris-review/lanes/reviewer.md` and the run brief. Agents
are read-only; adjudication happens in the session that dispatched them.

| G   | Component                              | Paths                                                              |
| --- | -------------------------------------- | ------------------------------------------------------------------ |
| G1  | Agent extension                        | `agent/extensions/scufris/**`, `agent/skills/**`                   |
| G2  | Host service and control protocol      | `host/service/src/*.rs`, `shared/control/**`                        |
| G3  | Surface gateway, attachments, content  | `host/service/src/bin/**`, `attachment.rs`, service attachments     |
| G4  | Briefings                              | `tools/briefing/**`, `agent/extensions/scufris/briefing/**`         |
| G5  | Jobs helper                            | `tools/jobs/scufris-jobs`                                           |
| G6  | Desktop core                           | `surfaces/desktop/src/*.rs`                                         |
| G7  | Desktop widgets, UI, den               | `surfaces/desktop/{src/widgets,ui,widgets,shell,backends}`, `tools/den` |
| G8  | End to end and deployment              | cross-cutting seams; `nix/**`, `flake.nix`, `RELEASE.md`, CI        |

G8 is not a directory. It follows whole paths across components: a surface
message from keypress to spoken answer, a briefing from timer to published
page to wake, a job from spawn to pane to event to land, and whether a landed
fix actually reaches the running system.

## Rules for what happens after

- Fix what is solvable without asking. Do not stop for confirmation on those.
- Keep architectural findings - anything that would change how Scufris works,
  the kind of change that warrants a major version - for Alex to decide.
  Collect them and ask at the end of the session, never mid-run.
- Findings are recorded here as each group is adjudicated, so a session that
  ends early still leaves the evidence behind.

## Findings

Appended per group as each is adjudicated.
