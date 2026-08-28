# Look at this: the verb, the window's identity, and the selection

- STATUS: OPEN
- PRIORITY: 60
- TAGS: voice, desktop, service

## Goal

"Look at this" tells Scufris what you are pointing at, and Scufris
decides what to do about it.

This is rung 0 and rung 1 of the capture ladder: the verb itself, the
window's own identity, and the selection. Both are universal and need no
per-application code, and together they answer a large share of real
"look at this" moments without a screenshot ever being taken.

Rung 2 is `20260828-220328`, the per-application deep roads. Rung 3 is
`20260828-224226`, the picture. Each is useful on its own and they
compose; this one comes first because the other two are rungs on a ladder
that has to exist.

## The division of labour

**The capture describes the situation. The model decides what to do about
it.** That is the same split the rest of Scufris uses: deterministic
helpers gather, the agent chooses. Nothing here tries to guess intent
from a window class.

**The companion captures, never the agent.** The agent has no display, is
built with no graphical dependency, and does not get one. This is the
boundary the widgets already keep.

## Rung 0 - the window's identity

Free, universal, and always sent.

`WM_CLASS`, `_NET_WM_NAME`, and `_NET_WM_PID`; then the PID gives
`/proc/<pid>/cmdline` and `/proc/<pid>/cwd`. Against a running GUI with no
file and no text, that already yields:

```
WM_CLASS      "wfc_arena"
_NET_WM_NAME  "NovaProtocol - 0.11.0"
argv          target/debug/examples/wfc_arena
cwd           /home/alex/personal/nova-protocol
```

which is enough for the agent to go and read the source of the thing you
are looking at.

`argv` generalises further than it looks: a large class of applications
names the document in its own arguments - image viewers, PDF readers,
`soffice <path>`. Where it does, the real file is in hand and any later
picture is a supplement rather than the subject.

**Which window.** `focus.rs` already records the top-level window that
had focus before the pill opened, because it has to give focus back. i3
defaults `focus_follows_mouse` to `yes` and nothing in `nix.dotfiles`
overrides it, so the window under the pointer is the focused window.
Hovering and focusing are the same act on this desktop, and the record
already ships.

**Also read the pointer**, converted to window coordinates, and say where
it was. A dense window plus "the pointer was at 812,455" resolves the
demonstrative that a window alone cannot.

## Rung 1 - the selection

The X11 PRIMARY selection is the cheapest "this" that exists: select text
in a browser, a document, a PDF viewer, anything, and the companion reads
it with no per-application code at all.

`GetSelectionOwner(PRIMARY)` names the window that owns the selection.
Use the selection only when that owner is the window being captured;
otherwise it is something selected somewhere else, an hour ago.

**PRIMARY, never CLIPBOARD.** The selection is what you just pointed at.
The clipboard is a private buffer you did not offer.

## Delivery

All text, and small once bounded. It rides the submission the companion
was already sending, in a delimited block the skill teaches the model to
read. Pi's RPC has no separate context field, so a fenced block in the
message is the road.

Bound the selection. A whole selected document must not become the
prompt.

## The skill

`skills/look/SKILL.md`, created here and extended by each later rung. It
says how to read the block: prefer a file path over anything else, prefer
the selection over a description, and use the title and `argv` to name
the thing when there is nothing better. This is where "figure out what to
do" lives, and it is model-facing prose rather than code.

## Consent

- The capture is explicit and single-shot. **No continuous recording,
  ever.**
- The conversation window says what was captured, by name - "captured:
  firefox / Ada Lovelace - Wikipedia". You should be able to see what
  left.

## Open

The transcript ring takes what Pi echoes as the user message, so a
capture block glued into the submission would be drawn in full by the
conversation window. Decide whether the window renders its own one-line
summary at submission time instead. The consent line above wants a
summary either way, so this may answer itself.

## Scope

- A capture verb in the textbox and a `scufris-ctl` verb. Not a pill
  take.
- Window identity from the `x11rb` connection `focus.rs` already owns,
  plus the PID's `cmdline` and `cwd`.
- The pointer in window coordinates.
- The PRIMARY selection, owner-checked and bounded.
- A delimited block in the submission, and `skills/look/SKILL.md`.

## Verification

- "Look at this" over a GUI with no file names the process and its
  directory, and the agent finds the source.
- A selection in a browser reaches the agent as text.
- A selection made in another window is not sent.
- No capture happens without the explicit verb.

## Origin

Backlog item 3 in `tasks/20260822-132001/RESEARCH.md` section 5, and
feature 3 of that task's `research/product-features.md`. Restructured
into rungs on 2026-08-28, replacing the earlier picture/text split.
