# Look at this: the window under the pointer into the session

- STATUS: OPEN
- PRIORITY: 60
- TAGS: voice, desktop, service

## Goal

"Look at this" puts a picture of the window you are pointing at into the
session, beside what you said. This kills the copy-paste loop for
anything that is not text.

Split from the original task on 2026-08-28. The Kitty scrollback half is
`20260828-220328` and shares this trigger; it is text and needs none of
the image road below.

## What the pointer actually answers

**"Hovering" and "focused" are the same window on this desktop.** i3
defaults `focus_follows_mouse` to `yes` and nothing in `nix.dotfiles`
overrides it, so the pointer entering a window focuses it. `focus.rs`
already records the top-level window that had focus before the pill
opened, because it has to give focus back afterwards. That record is the
answer to "what am I pointing at", and it already ships.

So the capture does not need `query_pointer` to choose a window. It
should still read the pointer for a second reason - see below.

**X11 answers "which window", never "which data".** The pointer resolves
to a top-level window, not to a table row, a cell, or a paragraph inside
it. Nothing on X11 can see inside another application's window, and this
task will not try.

That is not a limit in practice, because the model reads the picture. The
demonstrative in "look at this" is resolved by the model from the words
and the image together, not by the capture. What the capture owes it is a
hint: **the pointer position, in window coordinates**, said in the text
beside the image. A dense window plus "the pointer was at 812,455"
resolves "this" precisely, and it costs one `query_pointer` call on a
connection `focus.rs` already holds.

## The agent never touches X11

The companion captures. The agent has no display, is built with no
graphical dependency, and does not get one. This is the same boundary the
widgets keep: the agent asks, the companion draws.

## Delivery

Pi's RPC `prompt` and `steer` both take an optional `images` array of
base64 PNG (`docs/rpc.md:51,90`). That is the destination.

The bytes must not cross the service socket. `MAX_MESSAGE_BYTES` is
64 KiB and a base64 window capture is hundreds of kilobytes; raising the
cap would loosen every message to buy one.

**The path crosses the socket; the bytes do not.** The companion writes
the capture under the runtime directory, sends the path, and the service
reads the file and inlines it into `images` on the prompt it was already
sending. The Pi stdin stream accepts megabytes, so no bound is strained.

This is host-local by construction, which is correct today and is the one
thing that would change under `20260828-170154`: a remote surface would
send the bytes on a capture message instead. The service-side code that
inlines them into `images` is the same either way, so that upgrade does
not invalidate this one.

Housekeeping: captures are written to a private directory under the
runtime directory and removed once delivered. A capture that is never
delivered does not survive the companion.

## Scope

- Capture the recorded focus window as a PNG, from the `x11rb`
  connection `focus.rs` already owns, or by `maim` if the direct road
  proves worse.
- Read the pointer and convert it to window coordinates. Say it in the
  submission text.
- One capture message on protocol v3 carrying the path, sent by the
  companion beside the submission it belongs to.
- The service reads the file, base64s it, and sends it as `images` on the
  prompt. It deletes the file afterwards.
- Triggered by a verb in the textbox or a `scufris-ctl` verb. Not by a
  pill take.
- The capture is explicit and single-shot. **No continuous recording,
  ever.**

## Verification

- "Look at this" plus a question about the visible window answers from
  the capture.
- The pointer position reaches the agent and resolves a demonstrative in
  a window with more than one thing in it.
- No capture happens without the explicit verb.
- A capture file does not outlive its delivery.

## Origin

Backlog item 3 in `tasks/20260822-132001/RESEARCH.md` section 5, and
feature 3 of that task's `research/product-features.md`: "screenshot into
chat is the quietly high-frequency winner of both vendor apps".
