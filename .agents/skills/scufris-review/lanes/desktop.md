# Lane: desktop and X11

Judge whether the change respects how windows, focus, and the keyboard
actually behave on this desktop: bare X11 under i3, GTK through Tauri
and tao, no compositor. Every law below was paid for with a live bug;
the diagnosis records under `tasks/20260826-*/TASK.md` are the case
files.

You hold the display slot: no other lane brings up an X server while
you work.

## The laws

- A Tauri window call is a message onto the GTK main loop, not an act.
  Never read `is_visible` or `is_focused` straight back; verdicts come
  from the X server through `src/display.rs` (`Verdict::Yes/No/Unsure`,
  and `Unsure` never abandons anything).
- Nothing decides before the event loop runs. `runtime.start()` waits
  for `RunEvent::Ready` on its own thread; a decision made inside
  `setup` reads a world where nothing has happened yet.
- `WM_HINTS.input` is read at manage time, and i3 re-reads it at every
  remap. tao installs a one-shot draw handler that restores
  accept-focus after the first paint - build windows with
  `.focusable(false)`, never `.focused(false)`, or the hint flips while
  the window is mapped and the keyboard moves.
- The keyboard is claimed before the map (`pill::open`: accept, then
  show) and refused before every raise of the review box; a box that
  cannot refuse stays down. Entering review re-acquires the keyboard
  with a fresh repair budget.
- Equal min and max size hints float the window under i3 and cannot
  resize it live. No compositor: no alpha, and `transparent(true)`
  ships a black box. The blob is an X Shape cut, re-applied after
  every show and on any resize.
- In the page, the invisible field owns the words: Enter and Escape
  are window-level, typing is field-level, and a mousedown's default
  steals the field (refused while a review is editable). A recovery
  that restores the window must restore the field too.
- A phase that needs the keyboard watches it. Nothing outside the
  runtime can report a keyboard that landed nowhere, because the keys
  that would report it are the ones that have gone. The watch takes
  the keyboard back only when no window holds it - `PointerRoot` or
  `None` - and leaves a window the person moved to alone. A capture
  never records a window of the companion's own.

## Running

Run the harness when the change touches windowing, focus, or the
show/hide paths; otherwise reason from the code and say the judgement
is unharnessed. The harness is Xvfb plus i3 configured like the real
desktop (`focus_follows_mouse yes`, `focus_on_window_activation
smart`, a terminal holding the keyboard, the pointer over it), a fake
daemon on a scratch socket, and a fake transcriber. Instruments: the
i3 debug log, `xprop`, `xwininfo`, `xdotool`, focus sampled from the X
server. Stop everything by recorded PID.
