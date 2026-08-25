# Focus-free pill keys via i3 binding mode

- STATUS: OPEN
- PRIORITY: 80
- TAGS: voice, desktop, ux

## Goal

Escape and Enter control the pill without mouse focus. The user keeps
typing in their editor while recording.

## Scope

- i3 binding mode: `bindsym $mod+d exec scufris-ctl open; mode
  "scufris"`. Inside the mode, bare Escape and Return exec `scufris-ctl
  cancel|accept; mode "default"`. Bare keys are the review verdict;
  $mod variants stay the documented fallback.
- The pill window gets a `no_focus` floating rule. No focus to restore
  on close.
- The app runs `i3-msg mode default` whenever it closes the pill for any
  other reason, so mode and UI stay in sync.
- i3bar shows the mode name as a free state indicator.
- Provide the `scufris-ctl` verbs this needs on the existing control
  socket.
- Fallback for non-i3 X11: tauri-plugin-global-shortcut, registered only
  while the pill is visible.
- Document the i3 config snippet in the user guide. Sway runs the same
  config.

## Verification

- Record, cancel, and accept with the editor keeping focus throughout.
- Kill the pill by another path and confirm the mode resets.
- The fallback path works without i3.

Decided in `tasks/20260822-132001/RESEARCH.md` section 3 and the design
review decisions.
