# Live run

Three panels on the real screen, against a copy of the-den. The real journal
was never opened.

## The rig

`native/target/debug/scufris-desktop --foreground` with its own
`SCUFRIS_RUNTIME_DIR`, `XDG_STATE_HOME` and `XDG_DATA_HOME`, the packaged
wrapper's `LD_LIBRARY_PATH` and `GDK_PIXBUF_MODULE_FILE`, and `Super+Y` so it
could not answer the deployed companion's key. Nothing in the deployment was
started, stopped, or read.

A Python stand-in for the service binds the socket, answers `hello`, and pushes
one `widget.open` per panel as an instrument. The companion is a client, so a
stand-in that binds is all a panel needs to exist.

`the-den` was copied whole into the scratchpad and filled through the real
`today` command pointed at the copy: two tasks, a habit ticked, two food rows,
a weight, a note, and four tasks spread over three later days.
`SCUFRIS_TODAY_COMMAND` named a wrapper that runs `~/personal/today` from
source, which is what carries the new `foods` key.

Everything was stopped by recorded PID and the runtime directory removed.

## What was driven

| Did                                      | Expected                                     | Seen                                                              |
| ---------------------------------------- | -------------------------------------------- | ----------------------------------------------------------------- |
| Open all three                           | three panels, filled                         | agenda, macros, notes, all filled from the copy                   |
| Read the agenda                          | day in full, then what follows               | habits, both tasks, then 30 Aug, 2 Sep x2, 9 Sep with their dates |
| Read the month                           | a dot per day carrying an incomplete task    | dots under 28 and 30; September's are in September                |
| Click `Learn`                            | the journal changes, the panel reads it back | `- [ ] Learn` became `- [x] Learn`, struck through on the panel   |
| Click `Learn`, watch the focus           | the keyboard does not move                   | `getactivewindow` identical before and after                      |
| Click 30 August                          | that day in full, then what follows it       | that day's habits, `Call the dentist`, then 2 Sep and 9 Sep       |
| Click 31 August, which has no entry      | an empty day, and no file made               | `Nothing for this day.`, no habits, and no `2026-08-31-*.md`      |
| Page to October, then click the title    | looking is not selecting, the title is home  | October drawn, selection unmoved; the title put both back         |
| Read the macros panel                    | the day's intake, the weight, a trend        | 566 kcal, `p 56 g c 54 g f 14 g`, 81.4 kg +10.2, two food rows    |
| Read the notes panel                     | the day's notes                              | the date, then the note's heading and body                        |
| Point `SCUFRIS_TODAY_COMMAND` at nothing | every panel says why                         | see below                                                         |

## What it caught

The missing-command run is what the screen was for. The agenda said the
command was not on the path, and **notes said "No notes today."**

`build()` swallowed the failure from asking `today` for the day's path and read
on with `exists: false`. Agenda and macros ask a second question - `upcoming`,
`weight` - and that one failed loudly, so both reported it. Notes asks nothing
else, so a journal it could not reach and a day with no notes in it looked the
same from the panel.

Fixed by letting that failure through: a day with no entry is a full frame with
`exists: false`, and a command that will not run is trouble. The test now walks
all three views rather than the one that happened to work.

The same run showed the macros panel reading `p 0 g c 0 g f 0 g` under its own
trouble sentence. A day nobody logged and a day of nothing are not the same
day, so absent readings are dashes.

Both fixed and driven again: all three panels say the sentence, and none of
them shows a number it does not have.

## Not covered here

The tray's summon submenu. The panels were opened over the service instead,
which is the same `opening()` either road ends at - and the road the manifest's
`spawn` table exists for is the tray one, which the catalog test covers.

Writing a task from the panel. Nothing on screen sends that action yet.
