# Review caret starts offset from the end of the text

- STATUS: IN_PROGRESS
- PRIORITY: 50
- TAGS: desktop, ux

## Goal

Alex's nit after signing off the keyboard fixes (2026-08-26): the
review box caret sometimes starts visibly offset from where it
belongs - the end of the text - and snaps to the right position on the
first arrow key. It should always start at the end, on the monospaced
grid.

## Diagnosis and fix (2026-08-26)

The box pops in through a 200ms transform (`boxpop`: translate plus
scale 0.94 to 1). The orb window mirrors a draft as soon as it renders
review, so the first drafts arrive mid-pop, and `draw()` measured
their marks from `getClientRects()` while the box was scaled: visual
coordinates, laid as the box's own untransformed pixels. A caret near
the end of a line landed up to about six percent short and stayed
there - nothing re-measures until the next draft, which is exactly the
first-key snap Alex saw. The translation half always cancelled (both
the rect and the frame shift); the scale half did not.

Fix in `ui/review.ts`: `draw()` derives the current uniform scale as
visual width over layout width (`getBoundingClientRect().width /
offsetWidth`, 1 when the page is hidden and measures zero) and
`place()`/`caretMark()` divide every measured distance by it, so marks
are laid in the box's own coordinates and ride the transform instead
of freezing a mid-pop frame.

## Verification

- New regression test in `tests/desktop-ui.test.ts` ("a draft measured
  mid-pop still lands the marks on the letters"): geometry reported at
  0.94 scale, the caret must land at the end of the words on the
  layout grid. Fails against the unfixed page (caret drawn at 172.6
  instead of 183.6), passes with the fix; suite 124 passed, 0 failed
  under `TMPDIR=/tmp npm test`.
- `tsc -p ui/tsconfig.json` (the build.rs invocation) and `npm run
typecheck` clean; prettier clean on both touched files.
- The live look - the caret starting at the end with no snap - is
  Alex's.
