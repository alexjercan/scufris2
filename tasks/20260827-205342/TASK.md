# Refresh the scufris-review lane briefs

- STATUS: OPEN
- PRIORITY: 75
- TAGS: review,agents

## Source

Review round 1 of `20260827-081702`, finding m17. Full record:
`tasks/20260827-081702/REVIEW.md`.

## Fault

The `scufris-review` lane briefs were edited in the range they were then
used to review, and half-updated. A lane that greps for a file that does
not exist reports a pass. This round two lanes did exactly that and
caught it themselves; the next round may not.

- `lanes/contracts.md:14` names `PROTOCOL_VERSION`, which exists nowhere
  in the tree. Both sides spell it `SERVICE_VERSION`.
- `lanes/contracts.md:25` names `src/review.rs` and `ui/review.css`, both
  renamed to `textbox` in this range.
- `lanes/desktop.md:26` describes `pill::open` claiming the keyboard. The
  pill is built `focusable(false)` and `pill::holds_the_keyboard` is the
  inverse predicate.
- `lanes/desktop.md:34` describes an invisible field in `ui/index.html`
  that has no input at all.
- `lanes/red-team.md:30` says a transcript caps at 8 KiB. The shipped
  `MAX_TRANSCRIPT_TEXT_BYTES` is 4 KiB.

## Work

Correct the five, then read all five briefs against the tree rather than
only these lines - the ones found were found by lanes that happened to
grep. Prefer naming a symbol over naming a path where the brief allows
it, because a rename then breaks loudly at the grep instead of silently.

This is cheap and it gates the value of every later review round,
including the ones the other queued tasks will ask for.

## Proof

Grep each path and symbol the briefs name and confirm it resolves. That
check is the deliverable; consider leaving it as a script under
`.agents/skills/scufris-review/` so the next edit is checked too.
