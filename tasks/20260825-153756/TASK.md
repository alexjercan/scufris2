# Look-at-this context capture into the session

- STATUS: OPEN
- PRIORITY: 60
- TAGS: voice, desktop

## Goal

"Look at this" snapshots what the user sees into the session. This kills
the copy-paste loop, which is the actual current friction.

## Scope

- Capture the focused window as an image (maim) or, for Kitty, the
  scrollback text plus cwd.
- Deliver the capture into the authoritative Pi session next to the
  spoken prompt, through the existing control protocol. The protocol
  gains a capture message; v1 messages stay valid unchanged.
- Triggered by a verb during a pill take or by a `scufris-ctl` verb.
- The capture is explicit and single-shot. No continuous recording,
  ever.

## Verification

- "Look at this" plus a question about the visible window answers from
  the capture.
- Kitty capture carries scrollback text, not a screenshot.
- No capture happens without the explicit verb.

Backlog item 3 in `tasks/20260822-132001/RESEARCH.md` section 5.

## Still wanted, plan is stale (2026-08-27)

The feature stands: the copy-paste loop is real friction and nothing in
`20260827-081702` addresses it. The delivery in Scope does not. There is
no daemon and no v1 protocol to extend compatibly; the capture message
belongs on protocol v3, sent by the companion to the service, which puts
it in the session beside the transcript. The trigger is the textbox or a
`scufris-ctl` verb, not a pill take.
