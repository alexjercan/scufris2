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
- Nothing decides before the event loop runs. `RunEvent::Ready` in
  `main.rs` is where `display::the_event_loop_is_running` is set and
  where `App::start` is called, on a thread of its own; a decision made
  inside `setup` reads a world where nothing has happened yet.
- `WM_HINTS.input` is read at manage time, and i3 re-reads it at every
  remap. tao installs a one-shot draw handler that restores
  accept-focus after the first paint - build windows with
  `.focusable(false)`, never `.focused(false)`, or the hint flips while
  the window is mapped and the keyboard moves.
- Whatever a window says about the keyboard, it says before the map,
  and every time. There are three, and two of them say opposite things.
  The pill refuses (`pill::open`: refuse, then show) - it has no key
  handlers at all, so keys it took would land nowhere, and the refusal
  is a warning rather than a reason not to come up, because an
  indicator that will not go up is worse. The textbox and the HUD claim
  (`textbox::raise` and `hud::raise`: accept focus, then place, then
  show) - a field that cannot take the keyboard is worse than no field,
  so the textbox refuses to come up rather than come up mute.
- The window the keyboard goes back to is captured before the raise and
  never over one of our own (`focus::own_windows`), because i3 marks a
  window active on map even when it is `focusable(false)`: capture the
  pill and the keys go back into the window the person just closed. A
  restore only fires if the window being hidden actually held the keys.
- Equal min and max size hints float the window under i3 and cannot
  resize it live. No compositor: no alpha, and `transparent(true)`
  ships a black box. The blob is an X Shape cut, re-applied after
  every show and on any resize.
- In the page, an ordinary `<textarea>` owns the words, and the caret,
  the selection, and every editing key are the browser's own. The box
  that drew its own caret existed because the keys lived in another
  window; they do not any more, so a change that reintroduces
  hand-drawn editing is going backwards. What is still owed: a recovery
  that restores the window restores the field too, and a page never
  clears the field on anything but the host's answer.
- i3 does not stack floating windows by `_NET_WM_STATE_ABOVE`. It
  echoes the state and ignores it; last mapped wins. Two of our windows
  that can be up at once and overlap is a bug the geometry has to solve
  or one of them has to refuse.
- A phase that needs the keyboard watches it. Nothing outside the
  runtime can report a keyboard that landed nowhere, because the keys
  that would report it are the ones that have gone. The watch takes
  the keyboard back only when no window holds it - `PointerRoot` or
  `None` - and leaves a window the person moved to alone.

## Running

Run the harness when the change touches windowing, focus, or the
show/hide paths; otherwise reason from the code and say the judgement
is unharnessed. The harness is Xvfb plus i3 configured like the real
desktop (`focus_follows_mouse yes`, `focus_on_window_activation
smart`, a terminal holding the keyboard, the pointer over it), a fake
service on a scratch socket, and a fake transcriber. Instruments: the
i3 debug log, `xprop`, `xwininfo`, `xdotool`, focus sampled from the X
server. Stop everything by recorded PID.
