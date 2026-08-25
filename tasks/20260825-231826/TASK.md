# Bare orb pill: the orb is the pill

- STATUS: IN_PROGRESS
- PRIORITY: 75
- TAGS: desktop, ux

## Goal

Implement the bare-orb pill Alex approved 2026-08-25 ("it actually looks
amazing"). The visual spec is the Orb Study, section 03 "The pill is the
orb" (`orb-study.html` in this directory): the pill window shrinks
to a small square around the 64px orb, and the orb's shape and accent
alone carry the state. In review and uncertain, a transient text window
appears above the orb with the transcript. The listening timer stays as
a small dim line under the orb - useful because whisper has a limit,
not necessary.

## Scope

- Orb window: a small square-ish frame around the orb at the 64px
  preset (`resolvePreset(state, 64)` exists and is tuned). The label,
  transcript, detail line, and wave leave this window. The `--lv`
  mic-level scale on the orb stays. No compositor and no alpha: the
  window is an opaque panel, so keep the frame reading as "just the
  orb" on a dark desktop (chrome minimal or gone - taste call, record
  it).
- Timer: visible only in listening, small and dim below the orb.
  Decide whether the frame always reserves the timer line or resizes
  per state; min == max hints cannot resize live without re-applying,
  so prefer one fixed frame. Record the choice.
- Review window: a second programmatic window shown above the orb
  window in review and uncertain, hidden in every other state. Same
  recipe idiom as the pill (min == max size hints so i3 floats it,
  decorations off, always-on-top, skip taskbar, opaque,
  visible(false)) and never focused - the orb window keeps today's
  focus and key handling exactly (Enter sends, Esc dismisses,
  uncertain re-enter forces). Content: the transcript, a blinking
  caret, and a one-line hint ("enter sends - esc discards"; the force
  hint in uncertain). It pops in place with a ~200 ms rise-and-settle
  like the study's boxpop; a fixed size with an overflow fade is
  acceptable for long transcripts.
- Entrance: keep the pop-up entrance from `tasks/20260825-224811`
  (position tween + in-page arrive) on the orb window with constants
  adapted to the new frame. The review window needs no position tween.
  `prefers-reduced-motion` disables every new animation on both sides.
- Config: `capabilities/default.json` scopes permissions to the single
  label "pill"; cover the new window's label. `"windows": []` stays;
  creation is programmatic.
- Pill-only work. No widget-runtime machinery, no protocol, no
  extension changes.

## Verification

- `cargo fmt --check`, `cargo clippy --all-targets`, `cargo test -p
scufris-desktop`, and the build.rs tsc pass; prettier check clean
  (never `ui/orb-engine.js`); `TMPDIR=/tmp npm test` passes.
- Every state shows only the orb (plus the timer in listening);
  review and uncertain raise the text window above the orb; Enter and
  Esc behave exactly as before; the text window never takes focus.
- The entrance replays only on hidden-to-visible transitions; reduced
  motion shows everything in place.
- Visual sign-off on the live desktop is Alex's.
