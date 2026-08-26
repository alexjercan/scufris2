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

The tray icon carries the state: idle, recording, transcribing, working,
speaking, needs you, and backend unavailable. Recording always shows the red
privacy ring. Left-click opens the full chat. Right-click opens a menu that can
start voice input, show what went wrong, restart an unavailable backend, and
quit the companion. A backend crash leaves the tray running with an error
state; a companion crash leaves the conversation running.
