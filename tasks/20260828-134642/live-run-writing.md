# Live run: writing from the panels

The four writes on the real screen, against a copy of the-den. The real journal
was never opened.

## The rig

The same shape as [live-run.md](live-run.md):
`native/target/debug/scufris-desktop --foreground` with its own
`SCUFRIS_RUNTIME_DIR`, `XDG_STATE_HOME` and `XDG_DATA_HOME`, the packaged
wrapper's `LD_LIBRARY_PATH` and `GDK_PIXBUF_MODULE_FILE`, and `Super+Y` so it
could not answer the deployed companion's key. A Python stand-in for the
service binds the socket and pushes one `widget.open` per panel.

`the-den` and `macros.csv` were both copied into the scratchpad, and
`SCUFRIS_TODAY_COMMAND`, `DEN_PATH` and `MACROS_DATABASE` all named the copies.
Everything was stopped by recorded PID and the runtime directory removed.

## What was driven

| Did                         | Expected                                 | Seen                                                          |
| --------------------------- | ---------------------------------------- | ------------------------------------------------------------- |
| Click the agenda's task `+` | a box, over the panel, with the keyboard | 420x131 box, `TASK FOR 28 AUG`, holding the keyboard          |
| Type a task and press Enter | the journal changes, the panel reads it  | `- [ ] Ship the writing panels`, on the panel and in the file |
| Watch the month             | the day now carries an incomplete task   | a dot appeared under 28                                       |
| Press Escape                | the box goes with nothing written        | box down, journal unchanged                                   |

The box's height is `Ask::height` for one one-line field, which is the number
the unit test asserts. It arrived on screen as that number.

## What it caught

Two, both in the companion and both fixed.

**The box was placed from a window that had never been mapped.** `place` read
`window.outer_size()`, and placement runs before the show because a window
manager places a floating window when it maps it. A window that has never
mapped answers no size, so the box was centred as if it were nothing wide and
nothing tall: half its width and height down and to the right of where it
belonged. Seen as 1726,284 against an arithmetic that says 1500,218. Fixed by
sizing the placement from the question, the way the conversation window sizes
its own from constants, and pinned by
`the_box_is_sized_from_the_question_and_never_from_the_window`, which asserts
both the right answer and the wrong one the window used to give.

**The keyboard did not go back.** A click on a panel makes i3 name that panel
the active window while the keyboard stays in the window the person was typing
in, because the panel is built refusing it. `FocusTracker::capture` read only
`_NET_ACTIVE_WINDOW`, found one of our own, and correctly declined to record
it - but that is every capture the form takes, since a tick is what raises it.
So the box took the keys out of the editor with nowhere to put them back, and
closing it left the desktop with no keyboard at all. Measured directly: with a
holder window focused, ticking a habit left X input focus on the holder and
`_NET_ACTIVE_WINDOW` on the panel.

Fixed by asking the keyboard when the active window is the companion's, walked
up to its top level and filtered through the same `own_windows` list. The
active window is still asked first and the keyboard is only read when that
answer will not do, so the pill and the conversation window pay nothing for it.
Pinned by `a_panel_named_active_is_answered_by_the_keyboard_instead` and
`the_active_window_is_asked_first_and_the_keyboard_only_after_it`.

## Not covered here

The weight, food and note writes, and the refusal over a take in the textbox.
The user asked for the screen work to stop once the two defects above were
found and fixed, so those four are covered by their tests rather than by a
screen: ten in `tests/test_today_backend.py` for what the backend does with
each action, and the form's own for what reaches it.
