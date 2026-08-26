# First review of a fresh process loses the keyboard

- STATUS: IN_PROGRESS
- PRIORITY: 85
- TAGS: desktop, bug

## Goal

Fix the remaining keyboard loss Alex hit live (2026-08-26), after the
pre-map claim from task `20260826-102117` landed. It is now confined
to the first review of a fresh process; every later review works.

His flow, near verbatim: run `nix run .#scufris-desktop --
--foreground`, Super+D, speak, Super+D. The review box shows the text
(correct format, caret included) but he cannot type; Esc and Enter do
nothing. Clicking the pill recovers Esc but nothing else. He isolated
the variants across multiple fresh runs:

- run + click + Esc: works (the pill closes).
- run + click + Enter: nothing.
- run + click + anything else (typing, clicking the textbox): nothing,
  only Esc works after the click.
- After a click + Esc recovery, a second Super+D turn in the same
  process works completely, no click needed.

## What the report pins down

- Before the click, even Esc is dead: the pill window holds no X
  keyboard focus at all during the first review.
- After the click, Esc works but Enter does not: Esc must be handled
  at the window or document level in `ui/pill.ts`, while Enter needs
  the invisible transcript field to be the focused element - and the
  click gave the window focus without giving the field page focus.
  Both halves need fixing: the window must hold the keyboard in the
  first review, and a recovered window must put the field back too.
- First review versus every later review is the asymmetry to explain.
  The prime suspect is the one-shot accept-focus restore tao installs
  per window (`connect_draw` -> `set_accept_focus(true)`), which
  fires exactly once, on each window's first paint - the review box's
  fires in the middle of the process's first review, flipping
  `WM_HINTS.input` to 1 while the box is mapped. The box's first
  manage by i3 (a fresh container, possibly focused at manage time
  with `WM_TAKE_FOCUS` offered and never answered - the original pill
  crime, now on the box) is the other first-time-only event in that
  window.

## Scope

- Reproduce on the Xvfb + i3 harness from `20260826-102117`: fresh
  process, first turn straight into review, i3 debug logging at the
  box's first manage and at the moment tao's draw handler flips the
  hint. Confirm where the keyboard actually is during the first
  review.
- Fix the window half: the pill must hold the keyboard through the
  first review of a fresh process. If tao's one-shot restore is the
  mechanism, neutralize it for both windows (the box must never
  advertise input, the pill already claims before map).
- Fix the page half: `ui/pill.ts` must re-focus the transcript field
  when the window regains focus while a review is editable, so any
  recovery (click included) restores Enter and typing, not only Esc.
  Check the Esc/Enter handler asymmetry and make recovery symmetric.
- The claims that hold today stay: pre-map keyboard claim on the pill,
  pre-raise refusal on the box, fresh repair budget at review entry,
  honest verdicts, second-turn behavior.

## Verification

- Harness: a fresh process's first turn holds the keyboard through
  listening and review, Enter sends, Esc discards; a second turn still
  works; zero false verdict warnings.
- Regression coverage for whatever mechanism is found, failing before
  the fix where the layer allows it.
- The full check suite: cargo fmt/clippy/test/build, build.rs tsc,
  npm typecheck, prettier (never `ui/orb-engine.js`), `TMPDIR=/tmp npm
test`.
- Alex's live rerun of his exact flow is the sign-off.
