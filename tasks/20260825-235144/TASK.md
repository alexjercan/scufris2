# Pill scale, block caret, and see-through blob

- STATUS: IN_PROGRESS
- PRIORITY: 70
- TAGS: desktop, ux

## Goal

Three follow-ups on the bare orb pill (`tasks/20260825-231826`),
requested by Alex 2026-08-25:

- A larger pill, roughly 2-3x the current 76x92 frame, with the orb
  scaled to match.
- A transcript box that behaves like a normal textbox: a block caret
  that sits under the letter (z-index), never between letters pushing
  them apart, plus the standard editing keys (ctrl-backspace and
  friends).
- A see-through or blurred surround - the desktop visible around the
  orb like a blob, with the orb itself staying fully opaque and very
  visible.

## Scope

- Scale: grow the orb window and the orb together, 2-3x. The vendored
  engine (`ui/orb-engine.js`) stays byte-identical; scale through
  `resolvePreset` size or canvas upscaling in the painter, whichever
  the engine supports cleanly. The review box, its type, the timer
  row, the entrance constants (`RISE`, frame tests), and the window
  placement all scale coherently. Record the chosen numbers.
- Caret: the review box caret becomes an overlay - the full text
  renders in one run, the caret is a block positioned at the caret
  column and layered UNDER the glyph, so letters never shift when the
  caret moves. Selections render the same way (a background run under
  the glyphs). The caret only shows where the field is editable, as
  today.
- Editing keys: the real field is the invisible input in the orb
  window. First measure what WebKitGTK already gives it natively
  (ctrl-backspace word delete, ctrl-arrow word jump, home/end,
  delete, shift-selection, ctrl-a); implement only what is missing or
  swallowed by the pill's key handling. Every operation must reach
  the mirror (`scufris://draft` already fires on input, keyup,
  select, selectionchange).
- Blob: there is NO compositor - `transparent(true)` alone does not
  blend on this desktop and must never ship as a black box. Two
  honest routes:
  1. Preferred: an X Shape cutout. Shape the window region to a blob
     around the orb via the GTK window handle
     (`gtk_widget_shape_combine_region` on the window Tauri exposes).
     Hard-edged mask, true see-through to whatever is really under
     the window. Dark-on-dark the jagged edge should be subtle;
     judge and record.
  2. Fallback ("at least"): fake blur - read the root wallpaper
     pixmap (`_XROOTPMAP_ID`), crop the window rect, blur, paint it
     as the page background. It shows the wallpaper, not the windows
     under the pill, which on tiled i3 workspaces is a lie; record
     that cost if chosen.
     If neither is acceptable in practice, keep the opaque `#101010`
     ground, record why, and recommend what would unlock it (for
     example a compositor like picom).
- Keyboard focus, Enter/Esc semantics, the passive review window, and
  the reduced-motion behavior are untouched contracts.

## Verification

- `cargo fmt --check`, `cargo clippy --all-targets`, `cargo test -p
scufris-desktop`, `cargo build`, the build.rs tsc, `npm run
typecheck`, prettier check (never `ui/orb-engine.js`), and
  `TMPDIR=/tmp npm test` all pass.
- Headless smoke: the caret overlay never displaces glyphs; a word
  delete and a word jump round-trip through the mirror; every state
  paints at the new scale.
- The blob route chosen is recorded with its real behavior and its
  edge cases, not just its intent.
- Visual sign-off on the live desktop is Alex's.
