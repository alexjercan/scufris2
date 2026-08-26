# Focus-free pill keys via i3 binding mode

- STATUS: IN_PROGRESS
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

## What landed (2026-08-26)

Verified live on Alex's i3 4.25.1 session: the pill opens from an i3
binding, the bar shows the mode name while it is up, and the mode clears
on its own when the pill stops wanting the keys.

- `scufris-control/src/command.rs` - the command protocol. `Command` /
  `Verb` / `Answer` / `Outcome`, version 1, socket `desktop.sock` beside
  the daemon socket.
- `scufris-control/src/bin/scufris-ctl.rs` - the client. One verb per
  run, exit 0 reached the pill, 1 did not, 2 the run was wrong.
- `scufris-desktop/src/command.rs` - the listener. The companion is the
  server here, which is the opposite of `daemon.rs`. One verb per
  connection, a stale socket replaced rather than refused.
- `scufris-desktop/src/keys.rs` - the arrangement. Mode hook for the
  bare keys, `$mod+Escape` / `$mod+Enter` grabs for the rest, keyed off
  the posture rather than off a phase.
- `(Phase::Resting | Phase::Sent, Event::Escape)` sets `dismissed` and
  keeps the phase, so a passive pill is put away without cancelling
  anything.
- `nix/desktop.nix` ships `scufris-ctl`; `modeCommand` in the module.

### Three deliberate deviations

- **The `Keys` port carries a `Posture`, not a boolean.** Bare keys
  belong to a focused pill and a modified accelerator is safe for as
  long as the pill is on screen at all, so one predicate could not serve
  both. Passive is what makes `$mod+Escape` dismiss a resting pill on a
  desktop with no binding mode.
- **The companion owns the mode, and the mode bindings do not leave
  it.** The pill is what knows whether it still wants the key: the first
  `Enter` on an uncertain transcript answers and keeps waiting, and a
  binding that dropped the mode there would take the next `Enter` away.
- **The command socket is optional.** A session with no
  `XDG_RUNTIME_DIR` gets none and starts anyway, unlike the daemon
  socket. Found by the build sandbox, which has no runtime directory.

### Two faults found live, both mine

- **Deadlock on the first activation.** `global-hotkey` runs the
  shortcut handler and processes grab requests in one loop on one
  thread (`x11/mod.rs:256`), so a grab asked for from inside the handler
  waits on the thread that is waiting for the handler. The hotkey opens
  the pill, so this was the ordinary road in, not a corner. Grabs now go
  through a single serialising worker thread of their own.
- **Panic at startup once i3 owns `$mod+d`.** X reports a key another
  client has grabbed as `BadAccess`, which `global-hotkey` surfaces as
  `AlreadyRegistered`; registering the activation accelerator was a hard
  `?`. Under this recipe the window manager owning that key is the
  intended case, so it now warns and starts.

### Still open, both needing Alex

- **The pill's `no_focus` floating rule**, in the original scope. Left
  out: with the mode the keys work regardless of focus, so `no_focus`
  buys nothing and costs the transcript field - a pill i3 will not focus
  is one you cannot type an edit into. It would also leave
  `falls_short(Focused)` permanently true, so every decision would start
  a repair chain.
- **Making the recording phase passive**, so the editor keeps the
  keyboard while the person talks. This is what the task goal actually
  asks for and it is not done. It needs a fourth distinction in
  `Posture` - up, wants the bare keys, does not want the keyboard -
  because the mode must be on while recording and off while resting.
