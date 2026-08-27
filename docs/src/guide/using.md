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
configured project it first loads the agent menu from `.scufris.toml`. The menu
is what the project can do, not what a request means: Scufris starts only the
agents the request names, in the order it names them. "Implement X" is the work
agent alone. "Implement X, then review it" is the work agent and then the
review agent. The menu's conventions supply task tracking, the workspace, the
base branch, and the harness when the request is silent; an explicit
instruction such as "do it directly on master" wins over them.

Workers report progress events. `working` is quiet by default. `blocked`,
`done`, and runtime-generated `failed` events wake Scufris, which inspects the
job report and tells you what happened. It queues no follow-on work of its own.
Review uses the configured Pi or Claude harness and model against the
implementation job's exact workspace. Both adapters expose only read built-in
tools to the reviewer model. This is enforced at the model-tool boundary, not
by an operating-system read-only filesystem sandbox. The harness executable
remains trusted; for Claude, administrator-managed hook and plugin policy is
trusted too. Landing never happens implicitly; Scufris lands with an explicit
guarded operation when you ask for it.

Each worker runs in a named tmux session on the default server. Attach to it
read-only to watch, but do not type into worker panes.

Inspect stored jobs from a shell at any time:

```bash
scripts/scufris-jobs all
scripts/scufris-jobs <id-prefix>
scripts/scufris-jobs all --archived --json
```

## Quick Review

Ask for a quick review and Scufris starts a separate Pi RPC agent for the
`quick-review` menu entry. The agent loads the standalone Quick Review npm
extension, writes an exact-revision walkthrough, opens its browser page, and
answers page questions. Foreground Scufris remains available while the review
is open.

The outcome returns to Scufris, which reports it to you. A request for changes
restarts the implementation job with the review feedback. The separate agent
is closed when the review completes, the workflow stops, or the Scufris session
shuts down.

## Commands

- `/calm` inspects Calm mode; `/calm on|off` sets it. Calm hides thinking,
  tool execution rows, and job event noise. It is on by default.
- `/wake` inspects the worker wake mode; `/wake minimal|all` sets it.
  `minimal` keeps `working` updates quiet. `all` also wakes on each `working`
  event. Mandatory continuation events cannot be disabled.
- `/detail <id>` opens a private detail artifact in Plannotator.
- `/scufris-prompt` creates a private artifact with the exact assembled system
  prompt and its ordered provenance, without contacting a provider.

Explicit `/calm` and `/wake` values are restored with the session.

There is no `/speech`. Whether Scufris makes a sound is not a property of the
conversation; it is a property of the speaker, and the speaker belongs to the
desktop companion. Silence it from the tray.

## Voice

Every Scufris answer is one plain paragraph, with the rest of it in a detail
artifact. That is the shape of the assistant, not a speech setting, and it is
the shape with nothing listening. It is also what makes the answer safe to say
aloud, so each settled response, wake turns included, offers its paragraph to
whoever owns the speaker.

The desktop companion is the only thing that owns one. A session with no
companion is silent, and so is a companion with no synthesiser, which is what
`programs.scufris.voice` gives it. To stop Scufris talking without changing the
conversation, use "Mute Scufris" in the tray. Speech input is Pi
configuration, not Scufris.

## The voice pill

With the desktop companion installed, `Super+D` opens a small pill at the
bottom of the screen and starts recording immediately. The desktop stays
usable and visible around it.

- `Super+D` again stops the take. What you said is transcribed and arrives in a
  textbox above the pill, and the textbox takes the keyboard.
- `Enter` in the textbox sends the words. `Escape` discards them, and `Ctrl+C`
  copies them.
- `Super+Escape` cancels a take without opening the microphone on the way, and
  it puts a resting pill away.
- `Super+Delete` stops Scufris. It cuts what is being spoken and ends the run,
  and it changes nothing else: words you are still editing stay where they are,
  and the conversation keeps everything said so far. With nothing running it
  does nothing.

The pill is an indicator and nothing else. It never takes the keyboard, so
those three keys are the only ones that reach it, and they are built from
whatever modifier your activation hotkey uses. Name them yourself with
`cancelKey` and `stopKey` if your desktop already means something by them, or
set either to `"none"` to leave the key alone; see
[Installation](installation.md). The textbox is an ordinary focused window, so
the caret, the selection, and every editing key are its own.

`Super+D` always listens, so interrupting needs no second gesture. Press it
while Scufris is talking and the sentence stops and the microphone opens;
nothing is spoken for as long as the microphone is open. Press it while Scufris
is working and what you send is delivered into the run it is already doing
rather than queued behind it.

Sending gives the keyboard back to the window you were using, and the textbox
goes as the words leave. If transcription fails, nothing is sent and the pill
says so. If Scufris refuses the transcript before it leaves the companion, the
textbox comes back with it and `Enter` tries again, so an accepted transcript
is never lost.

If the transcript leaves the companion and Scufris never confirms it, the
textbox comes back to say the outcome is unknown, and keeps the words. It does
not send them again on its own, because the request may already have run and
running it twice is not harmless. You choose: `Ctrl+C` copies the words,
`Escape` discards them, and `Enter` tells you what sending again could repeat
before a second `Enter` sends it anyway.

Pill messages and their answers are part of the one conversation the service
owns. There is no second session, so the same words are there in a terminal.

### Opening the pill from a key binding

`scufris-ctl open` puts the pill up from outside its window, so a window
manager binding can be the thing that opens it. It ships with the companion and
takes that one verb, which does exactly what the activation hotkey does: it
starts a take, and it stops one that is running.

```
bindsym $mod+d exec --no-startup-id "scufris-ctl open"
```

Once i3 owns `$mod+d`, the companion cannot also take it, and it says so in the
log at startup. That is expected here - your binding opens the pill, and it
opens the same pill. Sway runs the same configuration.

## Reading and typing

The pill says what Scufris is doing. It cannot say what was said, and the
conversation window can. Click the pill to put the window up, and click it
again to put it away. It is the one thing the pill does when you click it.

For a key instead, bind the same verb:

```
bindsym $mod+s exec --no-startup-id "scufris-ctl hud"
```

That key puts the window up, and puts it away again. It shows what has been
said, oldest at the top, with a line at the bottom to type on. `Enter` sends,
`Shift+Enter` starts a new line, and `Escape` closes the window. Everything is
there whoever said it and however it was sent, so a question you typed in a
terminal and its answer are in the window too.

It is a window you work in rather than an indicator, so it does not stay over
what you move to. Press the key again to bring it back. There is no scrollback
beyond the last two hundred lines, which is what the service itself keeps.

Scufris can put it up as well. Ask to see the conversation and it opens the
window, and ask it to put the window away and it does. It never flips the
window on its own: it can only show or close, because it cannot see your screen
and would not know which of the two it had just done.

`scufris-ctl debug` in a terminal is the deeper tool and is not a fallback for
this. It is a whole Pi session - the tools, the thinking, the lot - and this
window is the last few lines and a place to answer them.

The tray icon carries the state: idle, recording, transcribing, working,
speaking, needs you, and backend unavailable. Recording always shows the red
privacy ring. Left-click shows the conversation window - it is the one thing
in the menu that always works, because it ships with the companion. Right-click
opens a menu that can show the conversation window, start voice input, open the
conversation in a terminal, show what went wrong, restart an unavailable
backend, and quit the companion. A backend crash leaves the tray running with
an error state; a companion crash leaves the conversation running.

## Panels on the desktop

Scufris can put a small panel beside the pill instead of only saying something.
A departure board, a timer, the machine's load: anything easier to look at than
to listen to. The panels sit above the pill and the desktop stays usable around
them. They are read, not operated, and the conversation carries on while they
are up.

There are two kinds, and which one you get depends on who asked for it.

- **Ones Scufris puts up.** They stand on a shelf above the pill, three at a
  time, newest nearest the middle. A fourth takes away the one that has been up
  longest. You never have to close one: a panel the conversation has moved on
  from dims and goes on its own after a minute.
- **Ones you keep.** Ask for something to stay and it takes one of the four
  screen edges. It stops fading, and it survives a request to clear the screen.
  There are four edges, so a fifth is refused rather than stacked.

Every panel wears the same three ticks in its corners.

- **Close** takes it away now. Scufris is told, so it stops talking about it.
- **Pin** promotes a panel Scufris put up into one of yours. It leaves the
  shelf, moves to a free edge, and stops fading. Pin again to hand it back. A
  pin with no free edge says so on the badge rather than doing nothing.
- **Restart** appears only while the thing feeding the panel has stopped, which
  is the one moment starting it again is worth offering.

Panels never take the keyboard. One that arrives mid-sentence cannot take your
keys, and the ticks are clicks for the same reason.

A panel you are reading does not go away under you. The clock stops while your
pointer is over it, while Scufris is speaking, while the microphone is open, and
while the pill is put away. Time you are not looking at the screen does not
count against a panel.

`Super+D` puts the pill away and takes the shelf down with it. Nothing is lost:
the panels come back exactly as they were, with the time they had left. Panels
you kept are not on that layer and stay where they are.

You can also put one up yourself. The tray menu carries a submenu of the panels
that can fill themselves, and one you summon is yours: it takes an edge slot,
fades never, and is not part of the conversation. Ask Scufris to clear the
screen and it takes down what it opened and leaves yours standing.
