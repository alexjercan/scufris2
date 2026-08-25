# Pill polish: frame, entrance, record dot

- STATUS: CLOSED
- PRIORITY: 70
- TAGS: desktop, ux

## Goal

Three polish moves on the pill, requested by Alex 2026-08-25 after the
orb landed (`tasks/20260825-222037`):

- A larger pill with larger text.
- A pop-up entrance from the bottom: a tween that rises, grows, and
  ends with a small squish.
- The red record dot removed. The orb's state accent already says
  "recording"; the dot is redundant.

## Scope

- Frame: grow the window and the type together, roughly 15-20 percent
  (taste call; keep every size coherent - paddings, orb canvas, label
  and transcript type). `pill.rs` constants (WIDTH, HEIGHT, maybe
  BOTTOM_MARGIN) and `pill.css` must stay in sync: the window is
  exactly the CSS layout size, min == max hints stay. `bottom_center`
  unit tests update with the constants.
- Entrance: no compositor, opaque window, no alpha - the window cannot
  fade and cannot be resized live (min == max). The pop is therefore
  position motion plus in-page content motion: recommend a short
  Rust-side position tween from below the resting spot with ease-out
  and a slight overshoot, paired with an in-page scale settle (grow to
  1, brief squash-and-settle at landing). Keyboard focus must not wait
  for the tween: place at offset, show, focus, then glide. The
  entrance runs only on hidden-to-visible transitions, never on
  re-presentations while visible. `prefers-reduced-motion` (page) and
  the same idea host-side: reduced motion means appear in place, no
  tween.
- Record dot: remove the `.record` element, its CSS, and any TS
  references. The `--lv` mic-level scale on the orb stays.

## Verification

- cargo test passes (bottom_center updated); tsc via build.rs passes.
- The pill rises from the bottom with an overshoot squish on show;
  a re-render while visible does not replay the entrance; reduced
  motion shows it in place.
- No red dot in any state; recording still reads from the orb accent
  and label.
- Visual sign-off on the live desktop is Alex's.

## Verification evidence (2026-08-25)

Frame: 560x64 to 640x76, with the type and spacing grown to match - label
10 to 12px, transcript 14 to 16px, detail 11 to 13px, timer 13 to 15px, orb
30 to 36px, wave 64x26 to 76x30 at 16 bars, gap 12 to 14px, padding
18/13 to 22/16px, corner ticks 9 to 11px, inset glow 18/3 to 22/4px.
`BOTTOM_MARGIN` stays at 72.

Entrance: a host-side position tween, 13 steps of 16 ms (208 ms), easing
`1 + 2.5u^3 + 1.5u^2` over `u = t - 1`, from 56 logical pixels below the
resting spot. It carries about 8 percent of the rise past the resting spot near
t = 0.6 and settles back; the last step is exactly `bottom_center`. Overlapping
tweens are cut by an `AtomicU64` generation that `arrange` and `hide` bump, so
a repair, a re-placement, or a hide always wins over a tween in flight. The
entrance is asked for only when `is_visible()` says a confirmed "down", so a
publish or a re-render while the pill is up never replays it. Focus never waits
for it: `arrange`, `reveal` (show, verify, always-on-top, set_focus, verify),
then `glide`. The page's half is a 240 ms `arrive` keyframe on `.pill`,
0.90x0.86 to 1.01x1.04 to 1.03x0.95 to 1, started by a `scufris://entrance`
event the host emits as the first step is scheduled. Reduced motion: the page's
`@media (prefers-reduced-motion: reduce)` rule already stops `arrive`, and the
page reports the same preference to the host in `pill_ready`, which the host
starts out assuming until told otherwise.

Record dot: `.record` is gone from `index.html`, and `.record`,
`.pill[data-state="listening"] .record` and `@keyframes privacy` are gone from
`pill.css`. `pill.ts` never referred to it. The tray keeps its own red privacy
ring; `docs/src/dev/desktop.md` was corrected to say so.

Checks run:

- `npx --no-install tsc -p ui/tsconfig.json` (the build.rs invocation): passes.
- `nix develop --command cargo test -p scufris-desktop`: 103 passed, 0 failed,
  including the `bottom_center` tests and two new ones for the frame size and
  the entrance easing.
- `nix develop --command cargo build -p scufris-desktop`: clean, no warnings.
  `cargo fmt --check` and `cargo clippy --all-targets` are also clean.
- `prettier --check` on every touched file, and `npm run format:check` over the
  repository: clean. `ui/orb-engine.js` was neither formatted nor edited.
- `npm run typecheck`: clean. `TMPDIR=/tmp npm test`: 112 passed, 0 failed, so
  the nested nix-shell socket-path failures did not appear at all.

Not verified here: nothing on this branch runs the pill on a display. The rise,
the recoil, the panel squash, the larger type, and the absence of the dot are
Alex's to sign off on the live desktop.

The remaining live visual check for this task is absorbed by task 20260825-231826, which re-tuned the frame and the entrance for the bare orb.
