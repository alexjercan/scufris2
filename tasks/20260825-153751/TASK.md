# Dictation-everywhere pill mode

- STATUS: CLOSED
- PRIORITY: 70
- TAGS: voice, desktop

## Goal

A pill mode that types the transcript into the focused window instead of
submitting it to the agent. This is the strongest daily-habit feature in
the research: it makes the hotkey hourly muscle memory.

## Scope

- A second activation (separate hotkey or a `scufris-ctl` verb) opens
  the pill in dictation mode with a visible mode marker.
- On accept, the transcript is typed into the window that had focus
  (xdotool type on X11), not sent to the daemon.
- The same review flow applies: immediate send and editable review.
- Nothing reaches the Pi session in this mode.
- Non-ASCII text survives the typing path.

## Verification

- Dictate into an editor, a browser field, and a terminal.
- Review-edit-accept types the edited text.
- Cancel types nothing and restores nothing.

Backlog item 1 in `tasks/20260822-132001/RESEARCH.md` section 5.

## Closed (2026-08-25)

Won't do. Alex's backlog triage: dedicated dictation apps already
cover this, and it is not worth building into the pill.
