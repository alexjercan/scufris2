# The reported case, on the fixed companion

Staging, built from this working tree, on the real display.

```
scufris-staging: service pid 1788214, desktop pid 1788215
socket=/run/user/1000/scufris-staging/service.sock
```

## The report, verbatim

"open both the codex usage and the claude usage widgets"

Both came up. Neither is the gray rectangle: the first is 320x195 and the
second 320x215, so both got through `fit`, `raise`, `monitor` and `settle`
rather than stopping at the 120x60 placeholder a window that never finished
placing would keep.

```
39845915 1068 395 320 195     codex usage
39845934  800 375 320 215     claude usage
```

Screenshots of both are in the session scratchpad. The claude panel reads
`WEEKLY 86%`, `SESSION 16%`, `FABLE 60%`, with `LIVE` in the corner.

## A third and a fourth, while the first two feed

This is the shape that froze: an open asked for while backends are already
writing, so the event loop is taking the turn five times a second rather than
once.

```
"also open the cpu and the memory widgets"   -> idle
```

Three widgets, and the codex panel had moved from x=1068 to x=532. That is a
shelf reflow carried out while another open was being decided, which is the
interleaving the turn exists to stop and the case the loop used to deadlock on.

## Four rounds of close-and-reopen

```
"close all widgets, then open the claude usage, the codex usage and the cpu widgets"
round 1 -> idle
round 2 -> idle
round 3 -> idle
```

Fresh window identifiers each round, so the shells were retired and new ones
adopted rather than reused.

## The tray summon, which is the path that changed most

`summon` runs on the event loop, so it no longer decides there at all: it hands
the whole open to the queue, which is also what moves the up-to-three-second
wait for a warm shell off that loop. Driven over the tray's own dbusmenu, which
is what a click does:

```
busctl --user call :1.7725 \
  /org/ayatana/NotificationItem/tray_icon_tray_app_scufris_desktop/Menu \
  com.canonical.dbusmenu Event isvu 15 clicked s "" 0

before: 3 widgets
after:  4 widgets
```

Then four summons back to back with no pause, which is the queue's open arm
taking them one at a time while three backends feed:

```
7 widgets
  39846343 320 215
  39846359 320 195
  39846387 340 225
  39846630 280 185
  39846816 340 225
  39846832 280 185
  39846874 320 215
```

## The loop was alive throughout

`scufris-ctl hud` is a companion-side request: the service relays it and the
event loop is what carries it out. It answered before the hammering and after
it. A screenshot taken after the seven windows were up shows the claude panel
at `WEEKLY 87%`, `SESSION 17%` - it had moved a point since the first shot, so
the panel was still being fed and still repainting.

The whole run logged nothing from the widget runtime: no
`a widget never reached the screen`, no `no widget window could be made ready`,
no `a widget decision had nowhere to wait for its turn`. The only two warnings
in the log are the dirty-tree note from nix and a GTK settings key, both of
which predate this.

Ctrl+C tore it down with exit 0 and both children gone.

## What this run does not prove

It does not catch the old deadlock in the act. The freeze is a race, and the
whole point of the fix is that the losing side no longer exists, so there is no
arrangement of this stack that can still show it. What the run proves is the
other half: that the loop keeps its own decisions when the turn is free, that a
handed-over decision is still made, and that the two paths that now always go
through the queue - the tray summon, and anything the loop cannot decide now -
open widgets the same way they did before.
