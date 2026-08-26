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

This is also what settles the focus contest recorded in task
`20260826-143950`: today the pill takes the keyboard back only when no
window holds it, and a window that takes it and stays keeps it. Keys
that never need focus remove the contest instead of deciding it.

## The resting pill is dismissed here too (2026-08-26)

Live finding: once the pill rests on screen it cannot be put away with
Escape. `Phase::Resting` is `Posture::Passive` - on screen without the
keyboard (`state.rs:230`) - and Escape reaches the app only through the
pill page's keydown (`ui/pill.ts:588` -> `pill_cancel`), which needs
focus. `app.rs:1005` states the same: a passive pill "cannot even be
sent an Escape". Today the only road down is `$mod+d` then Escape,
which opens the microphone on the way.

`scufris-ctl cancel` is the mechanism this needs, so it belongs to this
task rather than to a second accelerator in the app. Two additions to
the scope:

- A bare `$mod+Escape` binding outside the scufris mode, so a resting
  pill is dismissed from anywhere in one press.
- `(Phase::Resting, Event::Escape)` in the state machine. The arm does
  not exist, so the verb would reach the runtime and do nothing.

Alex decided to leave the gap until this task lands rather than add an
app-side accelerator for it.
