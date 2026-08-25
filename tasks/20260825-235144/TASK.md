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

## Verification evidence (2026-08-26)

### What changed, file by file

- `desktop/scufris-desktop/src/blob.rs` (new): the mask. `ellipse(width,
height)` returns one `Rectangle` per scanline of the ellipse inscribed
  in the frame, and `cut(window, width, height)` sends it to the X Shape
  extension as the window's bounding region. The display connection is
  opened once and kept in a `OnceLock`. Six tests cover the area, the
  frame bounds, the corners, the symmetry, a zero or oversize frame, and
  a scaled monitor.
- `desktop/scufris-desktop/src/pill.rs`: the frame and the entrance grew
  (see the constants below), and the window is cut down to the blob.
  The cut is asked for after every show and again on every window event,
  because a window has no X window to cut until the event loop has shown
  it. A resize or a scale change clears the flag so the mask is cut
  again for the new frame. Three frame tests were re-measured.
- `desktop/scufris-desktop/src/review.rs`: the box grew with the type;
  the frame test now also asserts that three lines fit and a fourth does
  not.
- `desktop/scufris-desktop/src/main.rs`: `mod blob;`.
- `desktop/scufris-desktop/Cargo.toml`, `desktop/Cargo.lock`:
  `raw-window-handle` for the window's XID, and the `shape` feature on
  `x11rb`.
- `desktop/scufris-desktop/ui/pill.css`, `ui/index.html`: the orb, its
  padding, the attention ring and the timer row at the new scale.
- `desktop/scufris-desktop/ui/pill.ts`: the orb is drawn at 160 from the
  64 preset, and the mirror carries the caret. Word and line deletions
  are handled where the field would otherwise lose them.
- `desktop/scufris-desktop/ui/tauri.d.ts`: the draft carries `caret`
  beside `text`, `start` and `end`.
- `desktop/scufris-desktop/ui/review.html`, `ui/review.css`,
  `ui/review.ts`: the words render in one run, and the caret and the
  selection are absolutely positioned blocks in a layer under it.
- `tests/desktop-ui.test.ts` (new): eleven tests that run the compiled
  pages against a stub DOM.
- `desktop/scufris-desktop/ui/orb-engine.js` is byte-identical.

### The numbers

- Pill frame 76x92 -> 190x230 logical pixels (x2.5). Bottom margin
  stays 72: it is a distance to the edge of the screen, not part of the
  frame.
- Orb 64 -> 160 pixels, padding 8/6/4 -> 20/15/10, attention ring inset
  -6 -> -12, timer row 11px type in a 14px row -> 20px type in a 35px
  row. The timer grew by less than the frame did, because a caption
  that scales with its container stops reading as a caption.
- Entrance `RISE` 28 -> 70 (x2.5), over the same 13 steps of 16ms.
- Review box 460x108 -> 620x140, gap 12 -> 24, transcript type 13.5px
  -> 18px, hint 10.5px in a 14px row -> 13px in an 18px row, padding
  12/16/10 -> 16/24/14. The box grew by less than the pill did in
  width, because it is sized by the line length that reads well.
- The orb is drawn at 160 from the engine's 64 preset, not upscaled
  from a 64 canvas. `resolvePreset` has entries for 64 and 20 only, so
  a 160 preset does not exist; the frame functions take the size as an
  argument. Measured over all twelve states, native drawing at 160
  keeps the dot count identical and scales the dots by (size/300)^0.6:
  mean dot radius 0.64 -> 1.10 pixels, ink coverage 0.337 -> 0.162 of
  the canvas for the ring, 0.311 -> 0.150 for the ribbon. So it is a
  finer, sparser sphere rather than a blurry zoom, which is the
  direction the engine's own 64-to-20 tuning points in reverse. The
  upscale route was rejected: it keeps the study's proportions but
  paints 160 pixels of 64-pixel drawing.

### The blob: X Shape, route 1

The pill window is cut to the ellipse inscribed in its frame (95 x 115
logical pixels), so the part of the window outside the ellipse does not
exist: what shows there is whatever is really behind the window, not a
picture of the wallpaper. Nothing blends and nothing needs a
compositor. The orb and the timer sit on the same opaque `#101010`
panel they always did.

Verified live on `Xvfb :95` at 1920x1080 running i3 with no compositor,
with a full-screen image window under the pill and a patterned root
behind that:

- the client window reports `shaped=true` with 121 bounding rectangles
  (the server merges the 230 scanlines into YX-sorted bands);
- i3 propagates the child's bounding shape onto its own reparenting
  frame, so the frame is cut too and no border box is left behind;
- the screenshot shows the dark ellipse with the yellow listening orb
  and the `0:05` timer inside it, surrounded by the window underneath -
  not by the wallpaper, which is what makes it a real cut-out.

Edge cases and costs, all real:

- The mask is one bit per pixel, so the edge is a hard staircase with no
  anti-aliasing. Dark on dark it is inconspicuous; over a bright window
  it is visible at the widest rows of the ellipse.
- The cut is static. It is made once per frame size, never per animation
  frame.
- There is no X window to cut until the event loop has mapped the
  window, which is why the cut is retried from the window's own events.
  On the first show of a process the window can therefore be a rectangle
  for the frame between the map and the cut. Afterwards the shape is a
  property of the X window and survives hide and show.
- A pill that cannot be cut stays the rectangle it was built as and says
  so once per attempt. Nothing else rests on the shape.
- The transcript box is deliberately not shaped: an ellipse would cut
  the ends off the lines it exists to show.

### The caret and the editing keys

The review page renders the whole transcript in one text run and draws
the caret and the selection as absolutely positioned blocks in a layer
with a lower z-index. Positions come from `Range.getClientRects()` on
the run itself, so the browser does the line breaking and no advance is
assumed: the caret block is the box of the character it sits on, or the
right edge of the last character plus one probe advance at the end of
the text. A selection becomes one band per line. Marks are drawn only
where the field is editable, so an uncertain transcript stays frozen and
carries none.

What the native field already gives, kept as it is: word jumps
(ctrl-arrows), home and end, plain backspace and delete, shift
selection, select-all, and the clipboard. All of them reach the mirror
through the existing `input`, `keyup`, `select` and `selectionchange`
triggers, which now carry the caret end as well as the range.

What was added, because the pill's own key handling is in front of the
field and these are the destructive ones: ctrl-backspace (delete the
word before the caret), ctrl-delete (delete the word after it), ctrl-u
(to the start) and ctrl-k (to the end). Each one runs
`document.execCommand("delete")` first so the field's undo history
survives, checks that the value actually changed, falls back to
`setRangeText`, and mirrors either way. A word is "the run the caret is
in, then the next run", which is what stops a word delete leaving a
double space behind.

This was measured against the compiled page in a stub DOM rather than
against WebKitGTK itself: the headless session never gave the pill the
keyboard (see below), so what WebKitGTK does with ctrl-backspace on
this desktop is still Alex's to confirm. The implementation is
deliberately independent of it - it owns the four destructive keys
outright and calls `preventDefault`, so the result does not depend on
which of them the port also implements.

### Checks

- `cargo fmt --check`: fails first on `blob.rs` (one wrapped assert),
  passes after `cargo fmt`.
- `cargo clippy --all-targets`: no warnings, no errors.
- `cargo test -p scufris-desktop`: 116 passed, 0 failed, including the
  six new `blob` tests.
- `cargo build -p scufris-desktop`: ok.
- `npx --no-install tsc -p ui/tsconfig.json`: ok.
- `npm run typecheck`: ok.
- `npx prettier --check` on every touched file: passes;
  `tests/desktop-ui.test.ts` needed one `--write` first.
  `ui/orb-engine.js` was neither formatted nor checked.
- `npm run format:check`: passes.
- `TMPDIR=/tmp npm test`: 123 passed, 0 failed. (The nested nix-shell
  `TMPDIR` is longer than the 108-byte cap on a unix socket path, which
  fakes about 48 failures; `/tmp` is the fix, not a workaround for a
  real failure.)
- Headless smoke, in `tests/desktop-ui.test.ts`: the caret sweeps every
  index without changing the run's text or adding a child to it; a
  selection draws one band per line; a frozen transcript draws nothing;
  an empty one still carries a caret one advance wide; ctrl-backspace,
  ctrl-delete, ctrl-u and ctrl-k round-trip through the mirror, through
  the port's `execCommand` and through the fallback; a word jump
  reaches the mirror; nothing is edited where nothing is editable; all
  twelve states paint dots inside the 160-pixel canvas.

### What stays unverified

- The live look is Alex's sign-off: the size of the pill on a real
  screen, how the staircase edge of the ellipse reads against real
  windows, and whether the timer wants more room under the orb.
- The transcript box was never seen on a real display at the new scale.
  In the headless session the pill never took the keyboard, which is
  environmental - Xvfb with i3 and no other client to focus - and a
  control run with the shape disabled behaved the same way, so it is
  not caused by the cut. Without the keyboard the box cannot be driven
  into review, so its three-line fit is only asserted arithmetically
  and its caret only in the stub DOM.
- Found on the way, not fixed, and not caused by this work: a
  transcript restored at startup is always abandoned. `runtime.start()`
  runs inside Tauri's `setup`, before the event loop, so the pill's
  `show` has not been carried out yet, `is_visible` answers false, and
  the state machine treats that as "the pill did not come up". The
  window is cut correctly on that path now, but the restored words are
  dropped. This deserves a task of its own.
