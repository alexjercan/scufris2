# Research: the Scufris widget runtime

Date: 2026-08-25. Goal: replace dashboardd in this project with a native
widget runtime in scufris-desktop. No compatibility with dashboardd or
with the current `scufris_widget_*` extension; dashboardd is the PoC that
proved the idea and remains its own project for linked browser
dashboards.

## Direction set by Alex

- Every widget may have a backend. Even CPU: Scufris says "CPU is at
  90%" and spawns the widget to watch it live.
- Backends are reusable across widgets (page review, 2026-08-25):
  a backend is a named shared source, not a per-widget process. The
  design keys one process per (backend id, spawn data) pair,
  refcounted, with each stdout line fanned out whole to every
  subscriber.
- Pin and close never sit adjacent (page review, 2026-08-25): Alex
  flagged the misclick risk. Close stays alone at the top right, pin
  moves to the bottom left beside the lifecycle badge, and both get
  corner-sized hit zones rather than glyph-sized ones.
- Placement is slot-based, a tiny window manager (page review,
  2026-08-25, Alex: "scufris HUD is like a window manager"). Named
  slots: shelf (3, exhibits, newest center), edges (4, instruments),
  center stage (session HUD). Every widget knows home and displaced
  slots, so motion is tweening between known positions, never a
  collision solver. The session HUD takeover displaces shelf widgets to
  free edges and restores them on close. Drags are i3's floating
  modifier; the runtime snaps a settled window into a slot within
  reach. An interactive demo of the choreography is section 05 of the
  design page.
- Demo feedback round (page review, 2026-08-25): a shelf widget with no
  free edge slot now hides while the session HUD is open instead of
  showing through it; every widget carries the always-visible close
  tick from the section 04 chrome; pinned widgets read as pinned with
  an accent border, a bottom-left corner mark, and a unified "pinned"
  badge (the separate "yours" state is gone); "clear" closes the
  runtime's exhibits and leaves user-owned widgets standing. Drag
  ownership is an open question with four options in section 05: A
  "touch means yours" (the shelf is the runtime's, everywhere else is
  yours - implemented in the demo, Alex's stated inclination), B only
  edge slots pin, C only the pin tick pins, D hold-to-pin. Alex
  confirms or picks before implementation.
- No links, no inputs between widgets. A Scufris widget shows
  something; connecting widgets stays dashboardd territory.
- Breakage is progress. Reimplementing the agent extension from scratch
  is preferred over adapting it. A new widget costs a couple hours, so
  no contract is worth preserving for reuse's sake.
- The design document opens with a short "the pill spawns these" intro,
  then covers only the widgets and how they work.

## Section 10 answers (Alex, page review)

- Q1 the shelf: decided 2026-08-25, keep as designed. Above the pill,
  centered, capped at 3, newest at center, oldest unpinned retires
  first.

## Settled product behavior (carried over from the design review)

From `tasks/20260822-132001/RESEARCH.md`, design review 2026-08-25.
These are decisions about behavior, not about hosting, so they survive
the rewrite:

- Two postures from one runtime: exhibits (spawned by Scufris as visual
  aids while he speaks) and instruments (summoned by the user,
  interactive, alive until closed). Timers sit in both camps.
- Exhibits age on topic relevance, not on the pill; closing the pill
  changes nothing on screen. A topic change dims to ~40%, a ~60s grace
  window follows, then a quick exit. Nothing disappears straight from
  LIVE. Citation or hover revives. Every aging clock freezes while the
  mic is hot, Scufris speaks, or the pointer is over the exhibit. Only
  the close tick and a "clear" verb exit instantly.
- The pin tick promotes an exhibit into an instrument: aging stops, the
  user owns it.
- Escalation is one spawn interface with increasing prominence: a line
  in the pill, then exhibits beside it, then the session surface
  raised. The session HUD is itself a widget on this runtime.
- The vision boundary stands: i3 and rofi own app workflow. Widgets
  show; they are not the desktop ("everything is a widget" was
  rejected).

## 1. dashboardd runtime lessons (explorer report, 2026-08-25)

Repo `~/personal/dashboardd`, v0.2.0. Native path (runtime + desktop
host + control + bundle + protocol) is ~5,400 non-test Rust LOC; the
browser host adds ~870 Rust + ~5,160 TS. A display-only rewrite needs
roughly 1,000 Rust + 300 TS, about 19 percent of the native path. The
surviving fifth is the boring correct part; the rest is the dashboard.

### Worth taking (ideas, and a few files' worth of code)

- Backend wire protocol (`dashboardd-widget-protocol`, 144 LOC): JSON
  Lines over stdio, `Envelope{version, message}` with tagged kinds:
  ready, initialize, update, ping/pong, shutdown, error. Language
  neutral; an 8-line Python backend is in their docs. Tested with
  shared fixture files both sides read (same pattern as our
  `desktop/control-protocol-v1.json`).
- Backend supervision (`run_backend()` in runtime/src/instance.rs, ~250
  LOC): tokio select over host commands, child stdout lines, and a 10s
  ping; `kill_on_drop` plus `process_group(0)`; 3s graceful shutdown
  then abort; requested exit distinguished from unexpected exit.
- Per-window custom URI scheme (~40 LOC): each widget window's label is
  its surface id; `register_uri_scheme_protocol` serves exactly that
  window's frontend bundle and 403s any other request; CSP whitelists
  only that scheme. The clean answer to letting a webview import an
  on-disk bundle.
- The mount contract: a variant is a self-contained ES module exporting
  `mount(container, ctx) -> {update(payload), destroy()}`, Shadow DOM
  per widget, CSS inlined at build time. The SDK is types only, zero
  runtime code.
- The transport-free seam: runtime exposes handle methods plus a
  broadcast of runtime events; hosts (Tauri, HTTP) are thin drivers.
  Their own history argues for it: they split runtime from HTTP, then
  deleted dashboards from the server entirely.
- Discovery judgment calls: duplicate widget id across search-path
  roots is a hard startup failure (no shadowing); bundle dir name must
  equal manifest id; path-traversal rejected; symlinks allowed for Nix.
- Health worth ~60 LOC of the 223: ping/pong plus "backend died" plus a
  banner with Restart. Their 5-state machine is polish.
- The atomic state writer (~40 LOC: temp file, fsync, rename, fsync
  dir) if any persistence is wanted.

### Deliberately dropped

- All link machinery (~600 TS + ~170 Rust). Telling: their desktop host
  already stubs outputs to a no-op; links only ever ran in the browser.
- The launch-form subsystem (~950 LOC) that exists only to collect
  required inputs before a window opens. No inputs, no drafts, no
  second window type.
- Options validation (~120 LOC), shared state (~285 LOC, used by one
  widget), tile/focus presentation duality, theme config file with hot
  reload (~294 LOC), the browser host entirely, the dashboardctl socket
  (~560 LOC) - Scufris already has a live channel to the Pi daemon and
  does not need a second CLI control plane for widgets.

### Cost note

dashboardd-desktop runs one OS process per visible widget at 1s ticks.
Fine at 3-5 windows; a consideration if we want many. Our backends
should own their tick rate and idle cheaply.

## 2. scufris2 integration inventory (explorer report, 2026-08-25)

### The replace-list

Deleted outright, 1,155 lines: `extensions/scufris/dashboard/index.ts`
(694), `tools/dashboard/scufris-dashboard` (235),
`tests/test_scufris_dashboard.py` (205), `skills/dashboard/SKILL.md`
(21). Edits ripple through: `flake.nix` (the dashboardd input is the
only consumer of rust-flake, crane, and rust-overlay - dropping it
removes ~120 of 327 flake.lock lines and a whole toolchain pin),
`nix/scufris.nix`, `nix/launcher.nix`, `nix/home-manager.nix` (the
`programs.scufris.dashboard` option group goes away, user-visible),
three nix checks with exact-argv or file-presence fixtures,
`package.json`, `scripts/scufris-dev`, `tests/structure.test.ts`,
`tests/dev_helper.test.ts`, `extensions/scufris/calm.ts`
(`scufris-widget-event` in hiddenCustomTypes - the new runtime's custom
type must re-register there), `extensions/scufris/workflow/identity.ts`
(system prompt sentence pinned by `tests/identity.test.ts`), and 8 doc
pages (~24 line refs).

### What the agent can do today (capability list, not wire format)

Five lazily registered tools after a successful discover: open a widget
by (widget, variant) with typed options/inputs and a focus/tile
presentation, returning a surface id; replace inputs or presentation
wholesale; list all surfaces with an `owned` flag; focus and close
owned surfaces. A 1 Hz poll diffs the surface list and emits a
`scufris-widget-event` follow-up when a surface vanishes externally.
Ownership is a process-local Map that does not survive a session
restart. Not possible today: reading data back out of a widget,
positioning or sizing, lifecycle control (dim/age/pin), or any widget
to agent push. Call flow is three nested processes (TS extension ->
Python helper -> dashboardctl) with two nested timeouts and envelopes.

### Protocol and daemon architecture

The Pi daemon (popup Pi process, `SCUFRIS_DAEMON=1`) listens on
`$XDG_RUNTIME_DIR/scufris/daemon.sock`; the Rust companion connects,
with 250ms-5s backoff and a 15s ping thread. One LF-terminated JSON
line, 64 KiB cap, explicit `v: 1` field where any other value is
rejected. Additive fields are tolerated; additive message types are
not. V1 messages: companion sends hello/submit/ping, daemon sends
welcome/ack/uncertain/refused/state/pong.

Two design constraints surfaced early:

- Direction inversion. Every daemon-to-companion v1 message is an
  answer or a broadcast. Widget spawn is the first daemon-originated
  command with a return value, so the daemon side needs targeted,
  correlated request/response machinery it does not have
  (`ControlServer.send` broadcasts; `DaemonLink` has no writer path
  triggered by a daemon message). This is the largest new mechanism,
  not the windows.
- New `DaemonBody` variants hard-break old companions: a decode failure
  drops the connection into a reconnect loop. With compatibility
  explicitly waived, bump `PROTOCOL_VERSION` to 2 (five constants in
  two files) and add a v2 fixture file beside
  `desktop/control-protocol-v1.json`, which both suites already read.

Widget events must not run through the pill's `Event` enum in
`state.rs`; they want a sibling of `App::observe` reaching a new
widgets module.

### Window mechanics available for reuse

- `pill.rs bottom_center()` is a pure, unit-tested anchor function; the
  clamping idiom generalizes to every anchor.
- The `ensure()` comment is load-bearing for every widget window: GTK
  honors exactly one combination - window left resizable with min ==
  max size hints; windows must be opaque (no compositor).
- `show_passive()` (place, show, verify visible, best-effort
  always-on-top, never touch the keyboard) is the widget-shaped show;
  the pill's Ready/Seen/Doubtful tri-state exists for mic privacy and
  widgets do not need it.
- Missing for N windows: runtime labels (label == surface id), window
  destruction, multi-window bookkeeping, per-window content addressing,
  stacking arithmetic. `Emitter::emit` broadcasts to every window;
  widgets need `emit_to(label)` or per-window channels.
- Two config gates: `capabilities/default.json` lists only "pill" (new
  labels get no permissions until a `widget-*` glob or second
  capability file), and the CSP in `tauri.conf.json` must whitelist a
  custom scheme if bundles are served that way.
- `build.rs` and `nix/desktop.nix` are single-frontend today;
  `pkgs.typescript` is already a nativeBuildInput.

### Docs and nix placement

- New chapter `docs/src/dev/widgets.md` right after the desktop
  companion in SUMMARY.md; replace the 7-line "Dashboard widgets"
  section of `guide/using.md` in place. `nix/docs.nix` needs no change;
  options must stay under `programs.scufris.*`.
- Widget assets must live in the desktop derivation, not
  `nix/resources.nix`: `nix/checks/desktop.nix` asserts the desktop and
  launcher closures are disjoint. Backends can be workspace members
  under `desktop/`; frontends either extend `build_frontend()` to walk
  `widgets/*/ui/` or get their own derivation.
- Version drift noted in passing: `tauri.conf.json` still says 0.3.0
  while the package is 0.4.0.

## 3. Prior art: native widget systems (research report, 2026-08-25)

Systems surveyed: eww, AGS/Astal, fabric, conky, Ubersicht, Rainmeter,
GNOME Shell extensions, KDE plasmoids, Hammerspoon, Zebar, SketchyBar.
Full citations in the agent transcript; the durable findings:

### Mechanisms worth stealing

- Zebar is our architecture already built: a Tauri 2 backend manages
  widget windows and pushes reactive provider data to plain HTML/JS
  frontends declared by a manifest (`zpack.json`). It validates the
  stack wholesale and proves manifest + real-code frontend on Tauri 2.
- eww's `deflisten` contract is the minimal live-data protocol: one
  long-running child per source, line-delimited output, one line = one
  update; `defpoll` adds interval/initial/run-while for simple cases.
- Ubersicht's five-export widget contract is the smallest complete
  widget API surveyed; with dashboardd's `mount(container, ctx) ->
{update, destroy}` they agree on the shape.
- Alexa Presentation Language is genuine prior art for exhibits:
  declarative template plus separate data source, returned with the
  voice response, themed by shared styles, shown for the interaction.
  Template/data separation lets the agent reuse a template and fill
  data.
- Cooperate with i3, never bypass it: managed windows with EWMH hints,
  not override-redirect (conky's tracker is two decades of evidence
  that override windows fight compositors, stacking, and repaint).
  Stable per-widget WM_CLASS instances (`scufris-widget-cpu`) let the
  user write their own for_window and no_focus rules.
- One daemon with open/close/update/list IPC verbs used identically by
  the assistant and the user (eww, SketchyBar); host-enforced teardown
  that never trusts widget code to clean up (the GNOME `disable()`
  lesson), plus a panic verb that provably kills everything.
- Theming drift is solved everywhere by inheritance: one host-injected
  token stylesheet (CSS variables) every widget styles against, so one
  file rethemes the fleet (Rainmeter MeterStyle, Plasma Kirigami.Theme,
  eww's single SCSS).

### Failure modes to design against

- Process leaks and zombies: the best-documented widget-runtime
  failure (four separate eww issues). Process groups, recorded PIDs,
  reaping, kill-on-window-close.
- Update storms: fork-exec per tick is the main widget CPU sin
  (SketchyBar measured orders-of-magnitude latency wins by avoiding
  it); central pacing, coalesced pushes, pause-when-hidden.
- Blocking data sources freezing widgets: every read async with a
  timeout; render last-good value plus a staleness marker.
- Focus stealing on spawn is fatal for exhibits that appear
  mid-utterance; never request activation, interaction opt-in.
- Crash blast radius: in-process widget hosts (GNOME Shell,
  plasmashell) let one widget kill the desktop; keep backends
  out-of-process and the host restartable.
- Every declarative widget DSL grew an escape hatch under pressure
  (yuck wants functions, Rainmeter grew Lua, Plasma DataEngines died):
  keep the manifest declarative and small, keep the view real code.

### The gap

No mainstream AI assistant spawns ephemeral native widget windows as
speech accompaniment; the closest patterns are single-overlay meeting
assistants and APL on Echo Show hardware. A multi-window, WM-native
exhibit runtime on a tiling desktop is unoccupied territory.

## 4. Tauri/WebKitGTK multi-window practicalities (research report, 2026-08-25)

Sources: tao 0.35.3, wry 0.55.1, tauri 2.11.5 sources in the local
cargo registry; WebKitGTK 2.52.5 (ABI 4.1); live measurements on this
machine; issue trackers cited in the agent transcript.

### Process and memory

One WebKitWebProcess per WebviewWindow; that is fixed WebKitGTK policy
since 2.26 and Tauri does not expose wry's `with_related_view` sharing.
Measured on this machine (dashboardd-desktop with 4 views): 35-43 MB
RSS but only 10-21 MB PSS per web process, most RSS being shared libs,
plus a shared 13 MB network process. Realistic marginal cost per widget
window: 10-20 MB. Ten widgets is roughly 150-200 MB incremental. Keep
widget pages tiny; heavy pages start near 100 MB.

### Creation latency and the warm pool

Webview init plus page load dominates: the native window maps at
~100-150 ms but painted content lands at ~370-420 ms on Linux, so a
cold spawn misses a sub-300 ms feel. The fix is a warm pool: hidden
windows fully load their page and run its JS (maintainer-confirmed);
`show()` is a cheap map with no reload. `eval` before load-commit is
queued and flushed in order, so the "become widget X" message can be
sent the instant the window is requested. Child webviews in one window
are unstable on Linux and save nothing (each child is still its own
process). The historical create-after-destroy blank-webview bug is
fixed in wry 0.55.1.

### Focus and layering on i3/X11

- `focused(false)` with `focusable(true)` maps as WM_HINTS input=False:
  i3 will not focus the window at map, yet it is clickable afterwards.
  This is the primary no-focus-steal mechanism; an i3 `no_focus` rule
  is defense in depth (i3 ignores no_focus for the first window on a
  workspace). Never call `set_focus()` on a widget - tao maps it to
  present_with_time, which raises and focuses.
- i3 auto-floats windows whose min size equals max size - the pill's
  min==max recipe generalizes to N windows unchanged.
- Always-on-top per window; floating windows sit above tiling windows;
  ABOVE windows stack among themselves by map/raise order and nothing
  reorders them spontaneously.
- Position after show (i3 does its own initial floating placement);
  `i3-msg "[instance=...] move position X Y"` stays as a scriptable
  fallback. `visible_on_all_workspaces(true)` (stick) is available.
- Nothing in the recipe changes with 5-10 windows.

### Per-window IPC

Use one `tauri::ipc::Channel` per widget window: targeted at exactly
one webview, ordered by per-channel sequence numbers, cheap at 1-10
messages per second; keep payloads under 8 KiB (larger switches to a
slow fetch round trip). `emit_to` works but has no ordering numbers and
races listener registration on fresh windows. Coalesce updates in Rust
and never stream raw ticks: a documented case drove a web process from
249 MB to 9.5 GB at ~188 emits per second over 36 hours.

### Long-run constraints

- Known and confirmed: no per-pixel alpha without a compositor (opaque
  rects only) and `WEBKIT_DISABLE_DMABUF_RENDERER=1` must stay in our
  launcher env (wry/tauri no longer set it).
- Day-scale memory growth is the documented WebKitGTK failure mode for
  long-lived pages. Widgets are stateless views, so the remediation is
  cheap: recycle (close and recreate from the pool) any widget whose
  web process PSS passes a budget, polled via /proc smaps_rollup.
  Per-view processes also mean one crashed widget never takes down the
  pill; webkit2gtk exposes web-process-terminated for recovery.
- `background_throttling` is a no-op on Linux. Hidden pages are
  throttled by WebKit itself (do not rely on rAF in pooled pages; act
  on the Channel message), while occluded-but-mapped windows keep
  rendering full rate. Rendering is CPU-side on this GPU path: small
  canvases, 1-2 fps redraws, not 60.

### Recommendation

Per-widget windows fed from a warm pool of generic shell windows.
Pre-create 1-2 hidden shells (visible false, focused false, focusable
true, always-on-top, skip taskbar, no decorations, min==max hints,
shared data dir) that idle at 10-20 MB each. Spawning a widget is one
Channel message plus size hints, position, show - well under 300 ms
because the webview cost was paid at pool-fill time. Refill the pool
asynchronously. Keep one Channel per widget for updates. Reject
single-window compositing (needs alpha), Tauri multiwebview (unstable,
saves nothing), and process-sharing via raw wry (loses crash
isolation).
