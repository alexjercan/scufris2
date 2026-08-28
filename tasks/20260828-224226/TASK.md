# Look at this: the picture, for windows that cannot be named

- STATUS: OPEN
- PRIORITY: 55
- TAGS: voice, desktop, service

## Goal

Rung 3, the floor of the capture ladder: a picture of the window, for
when nothing above it applied. A GUI with no file in `argv`, an image, a
document whose path was not found.

Rung 0 and rung 1 are `20260825-153756`; rung 2 is `20260828-220328`.
This one is last because it is the most expensive and the least often
the best answer - and because rung 0 already carries the identity that
makes a picture legible.

## Delivery

Pi's RPC `prompt` and `steer` both take an optional `images` array of
base64 PNG (`docs/rpc.md:51,90`). That is the destination.

The bytes must not cross the service socket. `MAX_MESSAGE_BYTES` is
64 KiB and a base64 window capture is hundreds of kilobytes; raising the
cap would loosen every message to buy one.

**The path crosses the socket; the bytes do not.** The companion writes
the capture under the runtime directory, sends the path, and the service
reads the file and inlines it into `images` on the prompt it was already
sending. Pi's stdin stream accepts megabytes, so nothing is strained and
no second size regime enters the protocol.

Host-local by construction, which is right today. It is the one part of
the ladder that changes under `20260828-170154`: a remote surface would
send the bytes on a capture message instead. The service-side code that
inlines them into `images` is identical either way, so that upgrade does
not invalidate this.

Housekeeping: captures are written to a private directory under the
runtime directory and removed once delivered. A capture that is never
delivered does not survive the companion.

## What makes a picture legible

Nothing on X11 can see inside another application's window. The pointer
resolves to a top-level window, never to a table row or a cell, and this
does not try.

That is not a limit in practice, because the model reads the picture. The
demonstrative is resolved by the model from the words, the identity block
from rung 0, and the image together. What the capture owes it is the
pointer position, which rung 0 already sends.

## Not doing

**The browser URL.** X11 has no road to it. The clean fix is a browser
extension, which is a separate surface entirely; the hacky fix is
synthesising Ctrl+L Ctrl+C, which disturbs focus and would eventually
fire into the wrong window. A browser gets its title, its selection from
rung 1, and a picture. Scufris can usually name the page from the title,
and that is a guess it should be allowed to make out loud rather than one
the capture pretends to have settled.

## Scope

- Capture the recorded window as a PNG from the `x11rb` connection
  `focus.rs` already owns.
- One capture message on protocol v3 carrying the path.
- The service reads, base64s, inlines into `images`, and deletes.
- The `skills/look/SKILL.md` paragraph: the picture is the last resort,
  and the identity block says what it is a picture of.
- The capture is explicit and single-shot. **No continuous recording,
  ever.**

## Verification

- "Look at this" over a GUI with no file answers from the picture.
- The picture reaches Pi as an image block, not as a path the agent has
  to open.
- A capture file does not outlive its delivery.
- No capture happens without the explicit verb.
