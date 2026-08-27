# Add the scufris-review skill

- STATUS: CLOSED
- PRIORITY: 75
- TAGS: review, skills

## Goal

A review panel skill for this repository, modeled on nova-protocol's
`nova-review` and approved by Alex (2026-08-26) after a design
discussion: parallel read-only reviewer lanes over one resolved change
range, adjudication in the dispatching session, findings ranked
BLOCKER/MAJOR/MINOR, and `tasks/<id>/REVIEW.md` when the range belongs
to a task.

## Shape

`.agents/skills/scufris-review/` (symlinked from `.claude/skills/`
like the other development skills): `SKILL.md` orchestrates - resolve
the range (session range, `<base>..<head>`, `--task <id>`,
`--worktree`; stop above 2000 changed lines), build the bundle once in
the scratchpad, dispatch all lanes in one message, adjudicate, report.
`lanes/reviewer.md` is the shared contract; each lane brief only says
where to look.

Lanes, per the approved design (red team promoted to always-on; feel
gated behind `--live`):

- Craft: the AGENTS.md house rules and the simplest correct shape.
- Correctness: state machine and durability edges, plus the proven
  test traps (a test that can never fail, TMPDIR socket-path lies,
  visual-versus-layout geometry, the draft mirror's triggers).
- Desktop: the X11/i3/GTK laws, one per live bug from tasks
  20260826-094501/102117/131704; holds the display slot.
- Contracts: the lockstep pairs (protocol fixtures and both codecs,
  identity, launcher argv, capabilities labels, frame constants,
  versions, the nix ripple), CHANGELOG for breaking module options,
  mdBook drift, tasks/ as append-only history.
- Red team: drive the states to their limits (daemon dead, kill and
  restore, store corruption, keys at the wrong time, caps, focus
  predators including the known mid-review hole).
- Feel (`--live`): orb legibility, entrance, caret, timer, earcons;
  the live desktop stays Alex's sign-off.

Adaptations from nova-review: the contended resource is the X display
(not a GPU), the machine rules are `TMPDIR=/tmp npm test`, no
`nix flake check` in lanes, the `orb-engine.js` prettier exception,
PID-only process stops, and tasks/ archives are never flagged.

## Verification

- The skill and every lane brief are prettier-clean; the symlink
  matches the existing development-skill wiring.
- First real run: the widget runtime increment 1 range (task
  20260825-215520) when it lands, as agreed in the design discussion.

## First real run (2026-08-27)

Done, against `50c6f90..f0e56a8`. Verdict and findings in
`tasks/20260825-215520/REVIEW.md`. What the run showed about the skill:

- **The 2000-line cap is wrong for a subsystem increment.** That range
  is 4853 insertions across 47 files, which is one increment of one
  task rather than an oversized change. The cap would have refused the
  first thing the skill was written for. Raise it, or measure the
  range in increments rather than in lines.
- **Five lanes at one increment each is the shape that worked.** Every
  lane found something no other lane found, and the two blockers came
  from different lanes.
- **Adjudication is where the value is, and it is not cheap.** Three of
  the lanes' findings were wrong on the facts and one blocker had to be
  ranked down. Re-deriving each load-bearing claim from the tree took
  longer than dispatching the panel did. The skill should say so.

Open until the cap and that note land in `SKILL.md`.

## Closed (2026-08-27)

Both open items landed in `SKILL.md`.

- The cap is 10000 changed lines, with a line saying what it is for: one
  increment of one task runs to several thousand lines, so the cap is
  there for a range nobody meant to ask for rather than for a big
  change. Alex set the number.
- The adjudication section now says to budget for it, and why: three
  lanes wrong on the facts, one blocker ranked down, and re-deriving the
  load-bearing claims took longer than dispatching the panel did.

The skill works. Alex on the first real run: "the review I did last was
ok so it's working".
