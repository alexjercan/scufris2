# Keep widget scrollbars clear of row actions

- STATUS: CLOSED
- PRIORITY: 90
- TAGS: desktop, widgets, bug

## Goal

Keep vertical scrollbars clear of end-side row actions in the desktop widgets,
including long food and workout lists in the macros panel. Preserve the compact
panel layout and apply one reusable rule to every affected widget list.

## Audit

The desktop widget fleet has four independently scrolling lists: agenda, notes,
and the food and workout columns in macros. All four can contain a red `x` at
the inline end of a row, so all four are affected. CPU, Claude, Codex, and timer
have no scrolling list. The conversation hides its scrollbar furniture, and its
attachment actions are therefore not exposed to an overlay. The form's fields,
textareas, and candidate list scroll, but they have no end-side row action. The
textbox has no adjacent action. The iOS scroll views hide their indicators and
have no equivalent vertical end-action overlap.

## Decision

Use one shell-owned `scroll-list` class for widget lists. Reserve a narrow
inline-end lane in every such list and bound the WebKit scrollbar width inside
that lane. Logical end padding works in either text direction. It protects both
WebKitGTK overlay scrollbars and non-overlay rendering without moving or
widening individual delete controls.

## Implementation

- Added shared scrollbar width and clearance tokens to the widget shell.
- Added one `scroll-list` shell rule that reserves 12 px at the inline end and
  bounds the supported WebKitGTK scrollbar to 8 px.
- Applied the class to agenda, notes, and the macros column constructor. The
  constructor supplies both the food and workout lists.
- Added a focused source audit that requires every shipped scrolling widget
  list to use the shared class and requires the clearance to exceed the bounded
  scrollbar width.

## Verification

- `node --experimental-strip-types --test tests/widget-ui.test.ts`: 2 passed.
- `tsc -p surfaces/desktop/widgets/tsconfig.json` and
  `tsc -p surfaces/desktop/shell/tsconfig.json`: passed.
- `env -u PI_PACKAGE_DIR npm run check`: product and protocol versions passed,
  TypeScript passed, all 89 Node tests passed, and Prettier passed. The inherited
  Pi harness variable was removed so the installed dependency resolves its own
  packaged themes instead of the harness package directory.
- `nix build "path:$PWD#checks.x86_64-linux.desktop-closure" --no-link -L`:
  passed. It built the WebKitGTK desktop package with the shell and widget
  TypeScript and passed all 319 desktop Rust tests plus closure assertions.
- `git diff --check`: passed.
