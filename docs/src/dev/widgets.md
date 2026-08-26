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

The pin tick on a panel's chrome promotes an exhibit into an instrument: it
leaves the shelf for a free edge slot, stops aging, and `scufris_widget_clear`
leaves it standing. It has to leave the shelf's columns and not merely leave the
shelf, because a column it kept is the column the reflow behind it moves a live
exhibit into. A pin with no free edge slot is refused and says so on the badge,
rather than doing nothing. The tick reads both ways, and an exhibit handed back
is the current one.

## Workspaces

An exhibit is sticky: it is on every workspace, the way i3's own scratchpad is,
because it belongs to the layer the pill lives on and that layer follows the
user around. Pinning drops the sticky flag, so the panel comes down onto the
workspace they are looking at - which is what makes it theirs. Instruments are
not sticky for the same reason.

The whole mechanism is `_NET_WM_STATE_STICKY`, asked for the way the
specification says a client asks: a message to the root window rather than a
property written directly, because once a window is mapped the state belongs to
the window manager. Nothing touches i3's real scratchpad. A window manager
unmanages a window when it unmaps, so a panel that goes down with the layer is
told again when it comes back.

## Aging

An exhibit needs no closing, which means something has to take it away. Three
rules do, and all three live in the pure runtime.

- **The turn boundary dims it.** The assistant state falling back to idle after
  working or speaking is one turn of the conversation ending. Every exhibit that
  turn neither opened nor updated is from a subject that is over: it drops to
  forty percent and starts its grace. The signal costs no message of its own -
  the daemon already reports assistant state, and the companion already listens.
- **Sixty seconds of grace retires it.** Silently. A report for every panel that
  went quiet would turn the thing that needs no closing into the thing that
  reports itself.
- **A citation or the pointer brings it back.** An update revives a dim exhibit
  even when the data is unchanged, and so does the pointer arriving over it -
  somebody reading a panel is the same signal as Scufris naming one. Neither
  moves it: the shelf is a row of places, and one that reshuffled under a
  sentence would be harder to follow than one that brightened.

Every clock is stopped while Scufris speaks, while the microphone is open,
while the pointer is over the panel, and while the layer is down. Time the user
is not reading the screen is not time the panel has been up. The reasons are a
set rather than a flag, so a microphone closing mid-answer does not start a
grace the answer is still stopping.

The runtime is handed elapsed time rather than reading a clock, so the whole
lifecycle is a unit test and a stopped clock is a field rather than an
arithmetic correction. The reading is done by one thread that sweeps once a
second and hands its measurement to the event loop, which is where the chrome
ticks already reach the runtime from.

## One layer

The pill and everything the runtime put beside it are one layer. The gesture
that puts the pill away puts the layer away: the shelf goes down with it, and
comes back with it exactly as it was. Nothing is retired, no widget is
unmounted, no backend is stopped, and the grace a panel had left is the grace it
still has. A widget opened while the layer is down is sized and loaded behind
it rather than flashing onto a desktop the user put the pill away from.

Pinned panels and instruments are exempt, because they are not on this layer.
That is what the pin tick does to a panel: it takes it out of the runtime's
hands, and out of the layer along with them. The same rule decides three things
at once - what ages, what `scufris_widget_clear` takes, and what goes down with
the pill - and it is one predicate in the runtime.

`DesktopSurface` carries this out where it already carries the transcript box
down with the pill. A window that comes back comes back through the first-show
path, because i3 places a floating window when it maps it.

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
`not_shown`, `companion_unavailable`, and `timeout` each call for something
different.

`widget_opened` means the panel is on the screen and nothing weaker. A window
the display would not size, would not raise, or would not say which monitor it
is on is retired and answered `not_shown`, because the alternative is Scufris
describing a panel the person cannot see. The same rule runs the other way: an
answer that arrives after its command was given up on is a panel whose
identifier exists nowhere, so the daemon closes it again rather than leaving it
standing with nothing able to reach it.

The daemon's idea of what is open is not authoritative and is not treated as
such. Exhibits age out on their own and a clear leaves whatever the user
kept, so the set drifts ahead of the screen by design. Commands are sent
regardless, and `surface_not_found` is what corrects the drift. A surface the
user closes with its own tick arrives as a `scufris-widget-event` follow-up
message, which `calm.ts` hides from the transcript.

## Summoning one yourself

The tray carries a submenu of the widgets the person can put up without saying
anything to Scufris. It opens an instrument, with no payload, and answers
nobody: a `widget_opened` for a request the daemon never made would be a reply
to a question nobody asked. The desktop is the person's, and a panel they put
there themselves is not a turn in the conversation.

What the submenu offers is the widgets with a backend behind them. A summon
carries no payload, so the widget has to be able to fill itself, and a backend
standing up on its own defaults is what does that. A widget that only ever shows
what Scufris handed it would summon as an empty panel.

A summon that cannot land - every instrument slot taken - leaves a log line and
nothing else. The four full edges are already on the screen in front of the
person who clicked.

## Widgets from elsewhere

`SCUFRIS_WIDGET_PATH` names extra widget roots, separated the way `PATH` is.
Each root is walked at startup for `<id>/widget.toml` and `<id>/widget.js` - the
compiled module, because nothing compiles anything at startup and the
companion's closure has no compiler in it. A project that ships a widget ships
what its own build produced.

External roots are additive and never override. A widget that shipped wins over
one on the search path, and an earlier root wins over a later one, so a name
always resolves to what it always did.

A widget that will not install is reported and passed over rather than stopping
the companion, which is where the search path parts from the shipped widgets. A
shipped widget that is wrong is a build failure and the developer sees it
immediately; one on the search path is a project on the person's own machine
that may be half-installed or gone, and a login session with no companion in it
is the worse of the two outcomes. The name it would have answered to simply
resolves to nothing.

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

For the same reason a widget window is one of the companion's own as far as the
focus tracker is concerned. The pill gives the keyboard back to the window the
person was using, and a window it cannot type into is not one to give it back
to.

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
gone land on whatever took its place. Labels are minted monotonically and carry
a stamp of the run that minted them, because a counter that starts at one each
process would hand `widget-1` out again to a daemon that outlived the companion.
A retired shell is destroyed rather than re-adopted, and the pool builds the
replacement as soon as one is taken rather than once the pool is dry.

A shell that builds but whose page never loads is given ten seconds and then
written off, with a line saying so. Otherwise it would count against the pool's
depth for the life of the process, and two of them would leave every later open
refusing with `no_shell` for a reason nothing said out loud.

Because the label is the surface identifier, the host reserves a shell before it
asks the runtime to open anything. A runtime that then refuses the open leaves
the shell unused, and it is discarded for the same reason.

## The shell page and the widget contract

`shell/shell.html` owns the chrome: corner ticks in the accent, an uppercase
micro-title, a close tick, a pin tick, a restart tick that exists only while a
backend is dead, and a badge naming the life state.
`tokens.css`
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

`ctx.spawn` is the payload the open carried, and `ctx.send(action)` writes one
line back to the backend. The returned view is driven with `update(data)` and
released with `destroy()`. A widget renders into the element it is handed and
nothing else: it draws no chrome, asks who sent nothing, and runs on no clock.

The widget draws its first frame from `ctx.spawn`, inside `mount`. The shell
never hands that payload to `update`, because the two are not the same shape:
a note's spawn payload is its data, while a timer's is the request its data
answers. What the shell does hold is an update that arrived while the module was
still importing, which is that widget's first data rather than a lost message.

`desktop/widgets/widget.d.ts` is the whole contract and the only copy of it.
The shell's own tsconfig reads that same file rather than declaring the types a
second time: two copies of a contract are two contracts, and the day they drift
is a day both projects compile and the panel breaks in front of the person.

A widget that offers its own buttons gives them the chrome's own `tick` class.
That is the one class a widget may wear: a widget's controls and the chrome's
ticks are the same affordance, and a widget that styled its own would be the one
that stops matching. Clicks only, never keys - the window is built unfocusable,
and a click has never needed focus.

`build.rs` compiles every widget as one tsc project and writes a table of
manifests and compiled modules into the binary. What ships is what was built,
and a widget whose TypeScript does not compile fails the build rather than the
first person who asks for it.

The compiled modules land in `desktop/widgets/dist/`, outside the frontend
directory on purpose. Everything under `ui/dist` is bundled into the app
protocol and reachable from any window, so a widget module served from there
would make the per-surface `scufris-widget:` scheme a formality rather than a
gate.

## Backends

A widget that shows a number that changes needs something producing the number.
That something is a backend: a directory under `desktop/backends/` holding a
`backend.py` that writes one JSON line per reading to standard output. The
directory name is the identifier, and a widget names one in its manifest:

```toml
backend = "system"
cadence = 1000
```

`cadence` is how often the widget says a reading should arrive, in
milliseconds. It is only used to decide when silence has gone on long enough to
mark. A widget naming a backend nothing installs is a startup failure, for the
reason a renamed widget is: the alternative is a panel that opens and then never
shows anything.

The first line a backend is handed on standard input is the payload the open
carried. `desktop/backends/system/backend.py` reads one key from it, `every`,
and reports processor load, per-core load, memory in use, and uptime.

A panel can also write back. `ctx.send(action)` puts one JSON line on the
backend's standard input, the mirror of the lines the backend writes, and the
backend answers with the refreshed reading the ordinary way. So a timer paused
from its panel and a timer paused by something else look identical from the
widget's side, and the widget never has to hold a copy of what the backend
knows. The host names the surface from the window the call came from, so nothing
the page sends says which panel it is. A panel with no backend behind it is
refused on the badge rather than left believing the action landed.

A widget whose readings are personal to it declares so:

```toml
shared = false
```

Two panels asking the same question share one process by default, which is what
a machine sampler wants. Two five-minute timers are not one timer counted twice,
so `shared = false` folds the surface into the key and each panel gets a process
of its own.

Three rules make this safe to leave running all day.

- **One process per question.** A backend is found by its identifier and the
  payload it was started with, so two panels asking for the same numbers share
  one process and two asking for different ones do not. The payload is
  canonicalized first, so the same question written in another order is still
  the same question. The last panel to stop reading is what stops the process.
- **Nothing is left behind.** Every backend is spawned into a process group of
  its own with its leader's identifier recorded, and stopping one signals the
  group rather than the leader - so a backend that started a child of its own
  does not leave it running. The word is standard input closing, then `SIGTERM`,
  then `SIGKILL` three seconds later, sent before the leader is reaped rather
  than after, because a reaped identifier is one the kernel may reuse. The same
  happens when the companion exits, because a process group of its own is also
  a process group that does not die with it.
- **Nothing is streamed straight to a window.** Readings are coalesced, latest
  wins, and handed over four times a second. A webview given a raw tick stream
  is the documented way to make one hold gigabytes.

A backend that owns something writable answers an action by changing it and
then reporting what it says. The `tasks` backend owns a plain-text list, one
task per line with `x ` in front of a finished one, at the path the spawn
payload names or in the person's own data directory. It watches the file rather
than holding a copy, so a task an editor added and a task the panel ticked off
arrive on the panel the same way.

An action on that list carries the line and the text the panel is showing, and
is refused when the two no longer agree. The file can change under an open
panel, and a click on a line that now says something else is a click on the
wrong task.

Writing a new task is not the panel's job yet. The window is unfocusable so that
one landing mid-sentence never takes the keyboard, which also means nothing
typed can land in it. Until keyboard routing is settled - the i3 binding mode
task - the panel acts on what is already on the list, and anything that writes
the file puts things there.

A reading is not a citation. Scufris naming a panel is what says the
conversation is still about it; a sampler writing its line every second says
only that the machine is on. A live graph that revived itself would be the one
exhibit that never ages out.

Health is separate from the life state, because the two hold at the same time
and say different things: a panel the user pinned can still be showing numbers
from a process that died. Silence past three of the widget's own cadences is
`stale`, and the badge says so while the number stays up. A process that exited
is `dead`: the accent goes to the alarm colour, what the widget drew recedes,
and a restart tick appears beside the pin. The restart brings the process back
for every panel that was reading it, because the tick is on one panel and the
process may be answering several. A frozen number that looks live is the one
outcome worse than an empty panel.

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
