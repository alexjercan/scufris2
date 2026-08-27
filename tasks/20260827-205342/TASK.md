# Refresh the scufris-review lane briefs

- STATUS: CLOSED
- PRIORITY: 75
- TAGS: review, agents

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

## Outcome (2026-08-27)

### The five, corrected

- `PROTOCOL_VERSION` -> `SERVICE_VERSION`, and the pair note now says
  the protocol is implemented twice, not three times. Added
  `MAX_TRANSCRIPT_TEXT_BYTES` and `MAX_MESSAGE_BYTES` as pairs, and the
  rule that a refusal code written as a literal at its send site is
  invisible to a client author reading the `refusal` module.
- `src/review.rs` and `ui/review.css` -> `textbox`, with `hud` added
  beside them.
- The `pill::open` law was backwards. `open` refuses the keyboard and
  then shows, because the pill has no key handlers and keys it took
  would land nowhere. The claim belongs to `textbox::raise` and
  `hud::raise`, which accept focus, then place, then show. Rewritten as
  one law over all three windows, saying which does which and why.
- The invisible field is gone with the drawn caret. The textbox is an
  ordinary `<textarea>` in a focused window, and the brief now says
  that reintroducing hand-drawn editing is going backwards.
- 8 KiB -> the constant. Both stale numbers were written as literals;
  every cap in the briefs now names its constant and says to read it.

### What else was stale

Reading all six briefs against the tree found four more:

- `runtime.start()` waiting for `RunEvent::Ready` describes a shape
  that is not there. `RunEvent::Ready` in `main.rs` sets
  `display::the_event_loop_is_running` and calls `App::start` on a
  thread.
- Two traps in `correctness.md` died with the drawn caret: the
  `getClientRects` scale-division rule, and the `scufris://draft`
  mirror. Replaced with two this range paid for: a page that clears its
  field before the host answers, and a timer keyed on something two
  attempts share.
- No brief mentioned the conversation window at all, which is the
  window this range added. Added to contracts (frames, capability
  labels), desktop (the keyboard law, the capture rule, the i3
  stacking law), red team (three windows, two of which take keys), and
  feel.
- The focus-capture rule was one clause at the end of an unrelated law.
  It is the fault B2 was, so it is its own law now, with the reason: i3
  marks a window active on map even when it is `focusable(false)`.

### The script

`.agents/skills/scufris-review/check-briefs.py`. It pulls every
backticked token out of the briefs, classifies it, and resolves it: a
path against four bases, a symbol by `git grep` outside `tasks/`.
Prose is skipped and the skipped list is printed, so the classification
can be reviewed rather than trusted. `SKILL.md` runs it before
dispatch.

Two things it got wrong on the way, both fixed and commented: a
substring match reported `focus::every_window` as present because it
sits inside a test named `every_window_label_...`, fixed with `-w`; and
paths a brief writes relative to the desktop crate or to `src/` needed
their own bases.

### Proof

Verified in both directions. With `src/display.rs` and
`focus::own_windows` deliberately renamed in `desktop.md` it exits 1
naming both; restored it exits 0.

- `check-briefs.py`: 46 paths, 28 symbols, 52 skipped, exit 0.
- `npx prettier --check .agents/skills/scufris-review/`: clean.
