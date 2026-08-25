# Pill polish: frame, entrance, record dot

- STATUS: IN_PROGRESS
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
