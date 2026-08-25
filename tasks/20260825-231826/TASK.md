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

## Verification evidence (2026-08-25)

### What changed, file by file

- `src/pill.rs`: the frame is 76x92 logical pixels (was 640x76) and the
  entrance rises 28 pixels (was 56). The module doc, the constant docs, and
  the frame-size test say what the numbers are for. `glide` now emits the
  entrance with `emit_to(LABEL, ...)` rather than a broadcast.
  `bottom_center`, `arrange`, `glide`, `ease`, `show`, `show_passive`,
  `hide`, the generation mechanics, and the `Shown` reporting are untouched.
  The off-screen placement test moved to a 60x40 monitor, because a 76 pixel
  pill fits on the 320x120 one the old test used.
- `src/review.rs` (new): the transcript window. `LABEL` is `"review"`,
  460x108 logical pixels, 12 pixels above the orb window. `above_pill` is
  pure and unit-tested against `pill::bottom_center`. `ensure` builds it with
  the pill's recipe - opaque, undecorated, on top, off the taskbar, equal min
  and max hints, `visible(false)`, `focused(false)` - and nothing in the
  module ever calls `set_focus`. `follow(state)` shows it for the states in
  `RAISED` and hides it for every other; `hide` confirms it is down.
- `src/main.rs`: `mod review`; `review::ensure` beside `pill::ensure` in
  setup; `Surface::present` raises or lowers the box before emitting the
  presentation; `Surface::hide_pill` takes the box down before the pill; a
  new `review_ready` command that republishes, so a transcript recovered at
  startup cannot be published before the box's page is listening.
- `capabilities/default.json`: `"windows": ["pill", "review"]`.
  `tauri.conf.json` is unchanged; both windows are still created
  programmatically.
- `ui/index.html`: the orb, the timer, and an invisible focusable transcript
  field. The label, the wave canvas, and the detail line are gone.
- `ui/pill.css`: rewritten for the bare orb. No border, no glow, no corner
  ticks, no panel gradient; the background is `#101010`, which is exactly the
  `PANEL` the orb painter mixes from, so the far dots dissolve into the
  window. The orb box is 64px carrying the `--lv` scale; the timer is a
  reserved 14px row; the transcript is a 1x1 transparent absolutely
  positioned input. The per-state accents, the attention ping, the error and
  disconnected filters, and the reduced-motion rule stay.
- `ui/pill.ts`: orb at `resolvePreset(state, 64)` drawn at 64px; the wave,
  the label map, and the detail line are gone; `mirror()` sends
  `{ text, caret }` to the review window over `scufris://draft` on input,
  keyup, select, `selectionchange`, and after every render, and only while
  the field is neither hidden nor read-only. Key handling, earcons,
  baselines, the rAF discipline, the tick handler, and `pill_ready` are
  unchanged.
- `ui/review.html`, `ui/review.css`, `ui/review.ts` (new): the box, the two
  transcript runs either side of a blinking caret, and the hint line.
- `ui/tauri.d.ts` (new): the Tauri globals both pages share, moved out of
  `pill.ts` because the two scripts share one global scope in one tsc
  project.
- `ui/tsconfig.json`, `build.rs`: the new sources are compiled and copied.
- `docs/src/dev/desktop.md`: the "Pill design" section now describes the bare
  orb, the review window, the reserved timer row, and the real rAF and
  reduced-motion behavior instead of the wave, the label shimmer, and the
  glow.

### Constants chosen

- Frame 76x92. Six pixels either side of the 64px orb, which is what the
  `--lv` scale needs at its loudest (`0.82 + 0.3 * 1` = 1.12, so 71.7px).
  Vertically: 8 top, 64 orb, 2 gap, 14 timer row, 4 bottom.
- Timer: one fixed frame with the row reserved in every state, per the task's
  preference. Resizing per state needs the min and max hints re-applied while
  the window is up, and a frame that resizes under the orb moves the orb. The
  cost is about 16 pixels of near-black under the orb when the microphone is
  closed, which on a dark desktop is close to invisible.
- Chrome: gone entirely on the orb window. No border, no corner ticks, no
  glow. The taste call is that any frame at all reads as a box with an orb in
  it rather than as an orb, and matching the window background to the
  painter's panel color is what makes the square disappear.
- Entrance: `RISE` 28 pixels, still 13 steps of 16 ms (208 ms) and the same
  easing and recoil from `tasks/20260825-224811`. The page's half is now a
  uniform pop - `scale(0.72)` to 1.06 to 0.96 to 1 over 240 ms - with the
  origin at `50% 44%`, the orb's own center, rather than the old panel
  squash.
- Review window 460x108, centered on the monitor, 12 pixels above the orb
  window's top. 108 is three lines of 13.5px text at 1.45 plus the caret, the
  8px gap, the 14px hint row, and the padding; longer takes scroll under a
  mask that fades both ends, and the caret is kept in view.
- Hints, verbatim from the study: `enter sends - esc discards`, and
  `the daemon is unsure - enter again forces it`.

### Decisions worth knowing

- The words are still the orb window's. The transcript field lives there,
  invisible and focusable, so Enter still sends exactly what the person
  edited and Escape still discards. The box is a mirror of that field,
  updated on every edit and every caret move, so what is read is what would
  be sent.
- The caret is drawn only where the field is editable. In uncertain the words
  are frozen, and a blinking caret there would offer an edit Enter would not
  carry.
- A box that will not come up is a warning, not a refusal. Failing `present`
  would keep the presentation from the orb as well, and the orb is what the
  runtime rests on.
- **Open, and the one place this costs something**: `retained` - a refused,
  still editable transcript - does not raise the box, because the task scope
  says review and uncertain only. Those words are therefore editable and
  unread. Adding `"retained"` to `RAISED` in `review.rs` and a third hint to
  `review.ts` is the whole change if Alex wants it.
- The state name, the error reason, and the notice on a retained transcript
  have left the pill entirely. The tray still carries all three.

### Checks

- `nix develop --command cargo fmt --check`: clean.
- `nix develop --command cargo clippy --all-targets`: clean, no warnings.
- `nix develop --command cargo test -p scufris-desktop`: 108 passed, 0
  failed (103 before, plus five in `review.rs` for the frame size, the raised
  states, and three placement cases).
- `nix develop --command cargo build -p scufris-desktop`: clean.
- `npx --no-install tsc -p ui/tsconfig.json` (the `build.rs` invocation):
  passes under `strict` and `noUncheckedIndexedAccess`. `npm run typecheck`:
  clean.
- `npx prettier --check` on every touched file and `npm run format:check`
  over the repository: clean. `ui/orb-engine.js` was neither edited nor
  formatted.
- `TMPDIR=/tmp npm test`: 112 passed, 0 failed.
- Headless smoke run of the compiled `ui/dist/pill.js` and
  `ui/dist/review.js` against a stub DOM, driving the real listeners: all
  twelve states paint the orb at 128x128 backing store (ratio 2) in their own
  accent; the draft is mirrored on render and on a caret move and is not
  mirrored from a read-only field; the timer reads `1:05` on a tick and
  clears off listening; the entrance class is added on `scufris://entrance`
  and removed on `animationend`; Enter and Escape still invoke `pill_submit`
  with the edited text and `pill_cancel`. Under reduced motion no frame is
  ever scheduled, one still frame is painted per state, the entrance never
  replays, and `pill_ready` reports `reducedMotion: true`. On the box: the
  review hint and the uncertain hint are exact, the caret shows only where
  the words are editable, a draft splits the runs at the caret and scrolls it
  into view, the overflow mask appears only when the text overflows, the pop
  runs once per arrival and not on a re-render, leaving a boxy state empties
  the box, and a draft arriving while the box is down is ignored.
- Tauri event targeting confirmed against `tauri-2.11.5` source rather than
  assumed: `emit_to` with an `AnyLabel` target reaches a JS `listen()`
  registered without a target, because `match_any_or_filter` passes every
  `EventTarget::Any` listener, and a plain `emit` reaches every webview. So
  the broadcast presentation reaches both pages and the addressed draft
  reaches the box.

### Not verified here

Nothing on this branch runs either window on a display. Unverified until Alex
looks at it: whether a 76 pixel wide window is honored by GTK under i3 the way
the 76 pixel high one already is (the same equal min and max hints do the work,
and only the axis is new); whether the orb reads as an orb rather than a small
black square on the real desktop; whether the reserved timer row is
unnoticeable; whether the review box sits at a comfortable distance above the
orb and is wide enough for a real take; whether the pop and the shortened rise
still read as arriving; and whether the states are still distinguishable with
neither label nor detail line. The visual sign-off is Alex's.
