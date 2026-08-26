# Widgets

A widget is a small panel on the user's desktop, opened by Scufris while it is
answering. The runtime lives in the companion, next to the pill and separate
from it: no widget reaches the pill's state machine, and the pill reaches no
widget.

The daemon side is `extensions/scufris/widgets/index.ts` and the
`skills/widgets` skill. The companion side is
`desktop/scufris-desktop/src/widgets/`, the shell page in
`desktop/scufris-desktop/shell/`, and the widgets themselves in
`desktop/widgets/`.

## Postures

A surface is opened in one of two postures, and the posture is what decides
where it lives and how long.

- **Exhibit.** Scufris showing something. Exhibits sit on a shelf above the
  pill, newest nearest the center, three at a time. A fourth retires the one
  that has been up longest. Nothing has to close an exhibit.
- **Instrument.** A panel the user asked to keep, in one of four screen-edge
  slots. An open with no free edge slot fails rather than stacking two panels
  in one place.

The pin tick on a panel's chrome hands it to the user and back. A pinned
surface leaves the shelf, nothing moves it again, and `scufris_widget_clear`
leaves it standing.

## The tools

The four tools are registered late, from the catalog the companion announces on
each connection, so the widget names the model can use are the widget names that
are installed. A session that never met a companion offers none of them.

- `scufris_widget_open`: widget, posture, and the widget's own payload. Returns
  the surface identifier.
- `scufris_widget_update`: new data for one surface.
- `scufris_widget_close`: one surface off the screen.
- `scufris_widget_clear`: everything Scufris opened, leaving what the user kept.

Every command travels over the control socket and waits for the companion's
answer under a correlation identifier, bounded at five seconds. A refusal
becomes a tool error carrying the companion's own code, and the codes are the
point: `widget_not_found`, `no_free_slot`, `surface_not_found`, `no_shell`,
`companion_unavailable`, and `timeout` each call for something different.

The daemon's idea of what is open is not authoritative and is not treated as
such. Exhibits age out on their own and a clear leaves whatever the user
kept, so the set drifts ahead of the screen by design. Commands are sent
regardless, and `surface_not_found` is what corrects the drift. A surface the
user closes with its own tick arrives as a `scufris-widget-event` follow-up
message, which `calm.ts` hides from the transcript.

## Windows

A widget window is the pill's recipe with one deliberate difference. It is
undecorated, always on top, skips the taskbar, is opaque because bare X11
without a compositor discards per-pixel alpha, and carries equal minimum and
maximum size hints - the one combination i3 floats and GTK honors.

The difference is focus. A widget window is built `.focusable(false)`, not
merely `.focused(false)`. tao restores accept-focus from a one-shot draw handler
after the first paint, so a window that is merely unfocused advertises
`WM_HINTS.input = True` from its second map onward and i3 gives it the keyboard.
A panel that landed mid-sentence would then take the keys out of whatever the
user was typing into. Clicks and the two chrome ticks work without focus.

Placement is arithmetic over the monitor the window reports, in the same style
as the pill's `bottom_center`, and it is unit-tested without a desktop session.
Position is set after the window is shown, because i3 places a floating window
when it maps it.

## Warm shells

Building a webview window and waiting for its page to load takes long enough to
be seen, and a widget arrives in the middle of a sentence. Two shell windows are
therefore kept built, loaded, and hidden. Opening a widget takes one and sends
it a single message on its own `tauri::ipc::Channel`.

A shell is used once. Its label is the surface identifier the daemon is answered
with, so a label handed out twice would let an update meant for a widget that is
gone land on whatever took its place. Labels are minted monotonically, a retired
shell is destroyed rather than re-adopted, and the pool builds the replacement
in the background.

Because the label is the surface identifier, the host reserves a shell before it
asks the runtime to open anything. A runtime that then refuses the open leaves
the shell unused, and it is discarded for the same reason.

## The shell page and the widget contract

`shell/shell.html` owns the chrome: corner ticks in the accent, an uppercase
micro-title, a close tick, a pin tick, and a live or pinned badge. `tokens.css`
holds the `--sw-*` palette every widget styles against. One file rethemes the
fleet; a widget that reaches for a hex value instead stops matching the first
time the palette moves.

The page draws nothing on a clock of its own. WebKitGTK throttles a hidden page
and a pooled shell is hidden by definition, so everything happens because a
message arrived.

A widget is a directory under `desktop/widgets/` holding `widget.toml` and
`widget.ts`. The directory name is the widget identifier; a manifest that
disagrees is a startup failure, and so is a duplicate. The module exports one
function:

```ts
export function mount(root: HTMLElement, ctx: WidgetContext): WidgetView;
```

`ctx.spawn` is the payload the open carried. The returned view is driven with
`update(data)` and released with `destroy()`. A widget renders into the element
it is handed and nothing else: it draws no chrome, asks who sent nothing, and
runs on no clock.

`build.rs` compiles every widget as one tsc project and writes a table of
manifests and compiled modules into the binary. What ships is what was built,
and a widget whose TypeScript does not compile fails the build rather than the
first person who asks for it.

## The two config gates

A widget window can be built correctly and still show nothing, because two
settings outside the Rust source decide whether the page can run at all. Both
fail at runtime rather than at build time, so both are asserted by Rust tests
that read the files they guard.

- `capabilities/default.json` must carry a `widget-*` label glob. Without it the
  shell page's `invoke` calls are denied and the window never reports itself.
- The `tauri.conf.json` content policy must list `scufris-widget:` under
  `script-src`. Without it the page cannot import its widget.

The `scufris-widget://` scheme serves exactly one thing: the module of the
widget the requesting window is currently holding, keyed by the window label the
request carries. The URL path is ignored, so a page cannot ask for a widget it
was not given however it writes the request. The response carries
`Access-Control-Allow-Origin: *`, because a dynamic `import()` from the page's
own origin to this scheme is a cross-origin fetch.
