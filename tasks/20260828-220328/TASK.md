# Look at this: Kitty scrollback and cwd into the session

- STATUS: OPEN
- PRIORITY: 60
- TAGS: voice, desktop, service

## Goal

When the window you are pointing at is a terminal, "look at this" puts
its scrollback and its working directory into the session as **text**,
not as a picture.

Split from `20260825-153756` on 2026-08-28. Same trigger, same window
detection, entirely different delivery. The original research rated this
half cheaper _and better_ than what the vendor desktop apps do by
scraping the accessibility tree.

## Why this is the half that pays

The copy-paste loop this feature exists to kill is a terminal loop. What
gets copied out of a window and pasted into an assistant is command
output, a stack trace, a failing test. Text is what that is, and text is
what the agent reads best - it can be quoted, diffed, and grepped, none
of which a screenshot allows.

It is also the cheaper half by a wide margin: no image road, no size
regime, no base64.

## What it needs from the desktop

`kitty @ get-text` and `kitty @ ls` give the scrollback and the working
directory. `allow_remote_control = true` is already set in
`nix.dotfiles/home/modules/kitty/default.nix:17`.

**One prerequisite, one line, in a repo you own:** `allow_remote_control`
alone permits control from a process running _inside_ kitty, over its
tty. The companion is outside it, so it needs `listen_on` set in the same
module and `kitty @ --to` on the call. Add that before building this.

Window detection is shared with `20260825-153756`: `focus.rs` already
records the top-level window that had focus before the pill opened, and
i3's default `focus_follows_mouse` makes that the window under the
pointer. The window class tells the two halves apart - a Kitty window
takes this road, anything else takes the picture.

## Delivery

Text, so the choice is only how much of it. Scrollback runs past the
8 KiB `MAX_SUBMISSION_TEXT_BYTES` and past the 64 KiB message cap for a
long session.

Decide between:

- **Bounded to the last screenful or few**, sent inside the submission.
  Nothing new on the wire. Most of the value, since what you are pointing
  at is what is on screen.
- **The path road**, shared with `20260825-153756`: the companion writes
  the text to a file and the service reads it. Carries the whole
  scrollback, and reuses whatever that task builds.

Recommended: the bounded road first. "Look at this" means the screen, and
a screenful is what the demonstrative refers to. The path road is there
if it turns out not to be enough.

## Scope

- Detect that the pointed window is Kitty.
- Read its scrollback and cwd through `kitty @ --to`.
- Bound the text and send it with the submission, marked as captured
  context rather than as something the person said.
- The capture is explicit and single-shot. **No continuous recording,
  ever.**

## Verification

- "Look at this" over a Kitty window with a failing command answers from
  the output, quoting it.
- The capture carries text, not a screenshot.
- The working directory reaches the session.
- No capture happens without the explicit verb.

## Origin

Backlog item 3 in `tasks/20260822-132001/RESEARCH.md` section 5.
