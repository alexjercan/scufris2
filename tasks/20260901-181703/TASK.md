# Improve the desktop and phone conversation UI

- STATUS: CLOSED
- PRIORITY: 95
- TAGS: desktop, ios, ui

## Goal

Make the conversation read consistently and scroll predictably on both
surfaces. Separate consecutive messages without spending the compact HUD's
space, put every message body in one column, follow the newest line only while
the reader is at the end of it, and offer an accessible way back when they are
not. Keep each surface native to itself.

## Audit

Rendered the HUD in headless Chromium at its shipped 760x560 before changing
anything.

- `.line` was `display: flex; flex-wrap: wrap` with `.what` at `flex: 1 1 auto`.
  A flex item keeps its content width as its flex basis, so any message wider
  than the window wrapped onto a row of its own and began at the window's 14 px
  padding while shorter messages began at the 102 px column. The attachment and
  details blocks reached the same column through
  `flex: 1 0 calc(100% - var(--gutter) - var(--step))` with a matching
  `margin-left`, which held only because the two calculations cancelled.
- Every message boundary was one 10 px gap, so a run of messages from one
  speaker read as a single block.
- `atBottom()` was already consulted before appending a line, but nothing else:
  a thumbnail finishing after its line, the composer growing under the list, and
  a replay all moved the conversation without settling the state, and there was
  no way back to the newest line once a reader had scrolled away.
- The window's `keydown` handler took Enter whatever held the keyboard, so the
  attach control, an attachment's Save and any future control could not be
  worked from the keyboard at all.
- The transient `thinking...` row was appended, so a message sent while Scufris
  was working landed under the row that stands for its reply.
- The phone always scrolled to the newest message on every arrival, dragging a
  reader out of what they were reading. Its message separation was already
  strong and did not need changing.

Nearby elements checked and left alone: the pill, textbox and form pages, the
widget shell's scroll lists (covered by task 20260901-175001), the empty
conversation state on the desktop, and the phone's composer, dictation and
attachment controls.

## Decisions

- **One grid, not a wrapping row.** `.line` is a two-column grid: the speaker
  marker in `var(--gutter)`, and the words, attachment cards and details all in
  `grid-column: 2`. A grid column cannot break to the next row, so the body of a
  message starts in one place whatever it is made of and however long it runs.
  The two cancelling `calc()`s are gone with it.
- **A hairline for a run, space for a reply.** A change of speaker gets 13 px
  and no rule, because the marker's colour already says it. One speaker saying
  two things gets 9 px and a 1 px rule drawn across the body column only. A rule
  through the gutter would read as a divider between speakers. This costs one
  pixel per boundary and takes four back.
- **Measure, do not remember.** Both surfaces read the scroll geometry rather
  than tracking an intent. The desktop settles on `scroll`, on append, on
  replay, on a captured `load` from a thumbnail, and from a `ResizeObserver` on
  the scroller. The phone settles from one `ConversationGeometry` value fed by
  two geometry probes, and re-pins when the content grows or the window shrinks
  under a follower. Both use the same threshold: nearer than 24 px to the end is
  reading the end.
- **The way back is the only new control.** It appears when there is somewhere
  to go and hides on arrival, is accented while messages the reader has not
  reached are waiting, states the count in its accessible name, and hands the
  keyboard back to the composer when used, because a control that removes itself
  cannot hold the focus. The desktop mirrors the count in an offscreen
  `role="status"`; the phone posts one accessibility announcement on the
  transition into having unseen messages, and none after, so it does not talk
  over what is being read.
- **The glyph is computed.** `surfaces/desktop/tools/generate_glyphs.py` writes
  `ui/latest.svg` from named numbers, the way `surfaces/ios/tools/generate_icon.py`
  writes the app icon. It is painted in the window's ground colour and sits on a
  filled control, so it needs no mask and cannot fail to render. The phone uses
  the native `arrow.down` symbol instead: the surfaces stay native to themselves.
- **Enter belongs to whoever holds the keyboard.** The page sends the composer
  on Enter only when a button is not focused.
- **The phone follows the reader's text size.** The message body and the details
  block scale with Dynamic Type; the markers around them do not, because a
  marker that grew would take the column the words start at.

## Implementation

Desktop:

- `surfaces/desktop/ui/hud.css`: the grid, the run separation, the `.stream`
  block the control is positioned against, the control itself, and an
  `.offscreen` class for the status region.
- `surfaces/desktop/ui/hud.html`: the `.stream` wrapper, the control, its
  generated glyph, and the offscreen `role="status"` region.
- `surfaces/desktop/ui/hud.ts`: `atBottom`/`pin`/`settle`/`drawLatest`, the run
  mark on each drawn line, the thinking row kept last and re-marked, and the
  Enter rule.
- `surfaces/desktop/tools/generate_glyphs.py` and `surfaces/desktop/ui/latest.svg`.
- `surfaces/desktop/build.rs`: the glyph is copied into `ui/dist` and watched.

Phone:

- `surfaces/ios/Sources/ConversationFollow.swift`: `ConversationFollow` and
  `ConversationGeometry`, the whole of the decision, apart from the view.
- `surfaces/ios/Sources/ContentView.swift`: the two geometry probes, `settle`,
  `arrived`, `pin`, `scrollToLatest`, the way-back control, the empty state
  centred with `containerRelativeFrame`, one shared `speakerGutter`, Dynamic
  Type on the message body, and the speaker marker read as part of the words it
  marks rather than as an element of its own.

Documentation: `docs/src/dev/desktop.md` for the window, and
`docs/src/dev/surfaces.md` for the follow contract both surfaces hold.

## Verification

- `tsc -p surfaces/desktop/ui/tsconfig.json`: passed.
- `node --experimental-strip-types --test tests/desktop-ui.test.ts`: 46 passed,
  12 of them new. They cover the run marks including the thinking row's
  re-marking, the control appearing and hiding, its counted label and status
  text, the keyboard returning to the field, a short conversation offering no
  control, a replay landing at the newest line, content growing under a follower
  and under a reader, Enter on a control, and two stylesheet audits of the
  layout contract.
- Mutation check: removing `grid-column: 2` from `.what` fails the layout audit.
- `python3 -m unittest discover -s tests -p 'test_*.py'`: 281 passed, 4 of them
  the new glyph generator tests.
- `env -u PI_PACKAGE_DIR npm run check`: version check, TypeScript, 99 Node
  tests and Prettier all passed.
- `nix build .#checks.x86_64-linux.desktop-closure`: passed. It compiled the
  frontend and the widgets under WebKitGTK and ran 319 desktop Rust tests.
- `nix flake check -L`: all checks passed.
- `git diff --check`: passed.
- Visual: the HUD screenshotted at 760x560 at the bottom, scrolled up, scrolled
  up with unseen lines, with an attachment card and a details block, and empty.
  A DOM probe confirms every `.what` starts at x=103 whatever its length or
  speaker, and that the run marks alternate correctly.
- Screenshot: `tasks/20260901-181703/hud-conversation.png`, the window at its
  own 760x560 rendered at 2x from the compiled `ui/dist/hud.js` over a stub
  host. One frame holds all of it: a speaker changing with space and no rule, a
  run of three separated by hairlines, an attachment card and a details block
  both starting in the body column, and the way back accented yellow at the
  bottom right with the reading position held where the reader left it. The
  count is not painted on the control; at that moment it reads
  `aria-label="Jump to the latest message, 2 new"` with `2 new messages below`
  in the status region.

## Recovery

The original Sprout was removed after its staged, uncommitted work was falsely
reported as landed. On 2026-09-01, the implementation was recovered from Git
objects and the managed Scufris screenshot attachment, restored on current
`master`, and verified again. The focused TypeScript suite passed 46 tests, the
full Node suite passed 99 tests, and the Python suite passed 281 tests. The
first desktop closure build correctly exposed that Nix excludes untracked
files; the recovered files were then added to the index before the packaging
check was repeated.

Not run here: the iOS build and its tests. This host has no Swift toolchain and
the `iOS` workflow builds and tests on `macos-15` with Xcode 26.3. The Swift
changes use only iOS 17 APIs, which is the declared deployment target:
`onChange(of:initial:)`, `coordinateSpace(.named:)`,
`GeometryProxy.frame(in:)` with a `CoordinateSpaceProtocol`,
`containerRelativeFrame(_:_:)`, `defaultScrollAnchor(_:)` and
`AccessibilityNotification.Announcement`.
