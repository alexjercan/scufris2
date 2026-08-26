# Using Scufris

## Conversation

Scufris is a pair-programming companion. It keeps the conversation in the
foreground, synthesizes evidence in its own voice, and stops at meaningful
decisions. It answers conversation and narrow project questions directly and
delegates work expected to take minutes.

Every final answer is one short plain-prose paragraph. Optional Markdown
detail is stored privately beside the session, and the transcript shows one
compact command:

```text
/detail 4f8c7a21d3e64b829e93ab10
```

Run that command to open the artifact in Plannotator. Approval and closure
produce one compact transcript row. Actionable feedback returns to Scufris
privately.

## Delegated jobs

Ask for project work and Scufris spawns an independent worker job. For a
configured project it first loads the project's workflow preferences from
`.scufris.toml` and follows them: task tracking, an isolated Sprout worktree,
the implementation harness and model, review, and the landing gate.

Workers report progress events. `working` is quiet by default. `blocked`,
`done`, and runtime-generated `failed` events wake Scufris, which inspects the
job report and decides what follows. Independent review uses the configured Pi
or Claude harness and model against the implementation job's exact workspace.
Both adapters expose only read built-in tools to the reviewer model. This is
enforced at the model-tool boundary, not by an operating-system read-only
filesystem sandbox. The harness executable remains trusted; for Claude,
administrator-managed hook and plugin policy is trusted too. Landing never
happens implicitly; the configured review must approve
first, and Scufris then lands with an explicit guarded operation.

Each worker runs in a named tmux session on the default server. Attach to it
read-only to watch, but do not type into worker panes.

Inspect stored jobs from a shell at any time:

```bash
scripts/scufris-jobs all
scripts/scufris-jobs <id-prefix>
scripts/scufris-jobs all --archived --json
```

## Quick Review

When project preferences select Quick Review, Scufris starts a separate Pi RPC
agent after independent review passes. The agent loads the standalone Quick
Review npm extension, writes an exact-revision walkthrough, opens its browser
page, and answers page questions. Foreground Scufris remains available while
the review is open.

Approval returns to Scufris as the final landing gate. A request for changes
restarts the implementation job with the review feedback. The separate agent
is closed when the review completes, the workflow stops, or the Scufris session
shuts down.

## Commands

- `/speech on|off|once|replay`: control spoken responses in a voice-capable
  package. `once` arms speech for one response. `replay` repeats the last safe
  paragraph.
- `/calm` inspects Calm mode; `/calm on|off` sets it. Calm hides thinking,
  tool execution rows, and job event noise. It is on by default.
- `/wake` inspects the worker wake mode; `/wake minimal|all` sets it.
  `minimal` keeps `working` updates quiet. `all` also wakes on each `working`
  event. Mandatory continuation events cannot be disabled.
- `/detail <id>` opens a private detail artifact in Plannotator.
- `/scufris-prompt` creates a private artifact with the exact assembled system
  prompt and its ordered provenance, without contacting a provider.

Explicit `/speech`, `/calm`, and `/wake` values are restored with the session.

## Voice

The ordinary voice-capable launcher stays silent until speech is enabled. The
popup launcher starts with speech and Calm on and resumes its dedicated
session. Enabled speech plays each safe settled response once, including
automatic wake turns. Speech input inside the popup is Pi configuration, not
Scufris.

## The voice pill

With the desktop companion installed, `Super+D` opens a small pill at the
bottom of the screen and starts recording immediately. The desktop stays
usable and visible around it.

- `Enter` sends what you said. It transcribes, shows the sent text, and
  submits without another confirmation.
- `Super+D` again opens the transcript for editing instead. `Enter` sends the
  edited text; `Escape` discards it.
- `Escape` while recording discards the recording.

Cancelling or sending gives focus back to the window you were using, and the
pill closes as soon as the words are on their way. If transcription fails,
nothing is sent and the pill says so. If Scufris refuses the transcript before
it leaves the pill, the pill comes back with it and `Enter` tries again, so an
accepted transcript is never lost.

If the transcript leaves the pill and Scufris never confirms it, the pill comes
back to say the outcome is unknown, and keeps the words. It does not send them again on its
own, because the request may already have run and running it twice is not
harmless. You choose: `Ctrl+C` copies the words, `Escape` discards them, and
`Enter` tells you what sending again could repeat before a second `Enter` sends
it anyway.

Pill messages and their answers are part of the one conversation the popup
shows. There is no second session.

### Keys that reach the pill from anywhere

`scufris-ctl` presses the pill's keys from outside its window, so a key binding
can be the thing that reads them. It ships with the companion and takes one
verb: `open` brings the pill up and starts recording, `cancel` cancels what is
running or puts a resting pill away, and `accept` accepts what the pill is
showing.

On i3, that turns the pill's three keys into a binding mode. Bare `Escape` and
`Return` belong to the pill only while the mode is on; the rest of the time
they are your editor's.

Two halves, and both are needed. Your configuration enters the mode when you
open the pill:

```
# exec takes the rest of the line, so the command is quoted to chain onto it.
bindsym $mod+d exec --no-startup-id "scufris-ctl open"; mode "scufris"

mode "scufris" {
    bindsym Escape exec --no-startup-id "scufris-ctl cancel"
    bindsym Return exec --no-startup-id "scufris-ctl accept"
    # The way out by hand, if the companion is not running.
    bindsym $mod+d mode "default"
}

# A resting pill is put away from anywhere, without opening the microphone
# on the way.
bindsym $mod+Escape exec --no-startup-id "scufris-ctl cancel"
```

The companion leaves it, and it does that whenever the pill stops wanting those
keys - after a cancel, after a send, and as it exits - so the mode and what is
on screen stay in step even when the pill closed for a reason you never asked
for. Give it the way to say so:

```nix
programs.scufris.desktop.modeCommand = pkgs.writeShellScriptBin "scufris-mode" ''
  exec ${pkgs.i3}/bin/i3-msg mode "$1"
'';
```

Note that the mode bindings do not leave the mode themselves. The pill is what
knows whether it still wants the key: the first `Enter` on an uncertain
transcript answers you and keeps waiting, and a binding that dropped the mode
there would have taken your next `Enter` away from it.

Once i3 owns `$mod+d`, the companion cannot also take it, and it says so in the
log at startup. That is expected here - your binding opens the pill, and it
opens the same pill.

`binding_mode_indicator yes` in your `bar` block shows the mode name while you
are in it, which is a free indicator that the pill is listening. Sway runs the
same configuration with `swaymsg` in place of `i3-msg`.

Without any of this the pill still answers `Super+Escape` and `Super+Enter`
while it is on screen, built from whatever modifier your activation hotkey
uses. That is the fallback on a desktop with no binding modes; on i3 the
bindings above take those keys first, and they reach the same place.

The tray icon carries the state: idle, recording, transcribing, working,
speaking, needs you, and backend unavailable. Recording always shows the red
privacy ring. Left-click opens the full chat. Right-click opens a menu that can
start voice input, show what went wrong, restart an unavailable backend, and
quit the companion. A backend crash leaves the tray running with an error
state; a companion crash leaves the conversation running.
