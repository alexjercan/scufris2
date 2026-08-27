# Review: Scufris as a service, the architecture inversion

- ROUND: 1
- REVIEWER: five scufris-review lanes (craft, correctness, desktop,
  contracts, red team), adjudicated in one pass
- RANGE: `185034a..a13cb38` - 15 commits, 169 files, 15391 insertions,
  12631 deletions
- VERDICT: CHANGES REQUESTED
- READABLE: https://claude.ai/code/artifact/14ec7c5e-53a4-43f3-88f5-d3c96ba78331

## Verdict rationale

The inversion itself holds. One service owns the conversation, three
roles are clients of one socket, and the v3 protocol is implemented
twice and agrees on every tag, role name and byte bound. The contracts
lane checked all four caps on the wire and found them exact. The panel
found no defect in the architecture the task set out to build.

What fails is everything the new conversation window touches, and one
string the service adopts without a bound.

The window is the newest surface and it is the least finished. Two of
the three blockers are its own, and both are the kind the desktop lane
exists to catch: it strands the keyboard, and it draws itself over the
textbox. The third is worse in effect - an oversize agent error turns
the service into one that welcomes every client and immediately hangs
up on it, with nothing above `debug` to say so.

B2 is a recurrence. Round 1 of `20260825-215520` raised the identical
defect against `DesktopSurface::windows()` and it was fixed there. The
HUD was given its own `FocusTracker` and made the same mistake again.

The range is 28022 changed lines, 2.8x the skill's cap. It was reviewed
whole because the commits were named explicitly.

## Findings

### BLOCKER

**B1. An oversize agent error string kills every client connection, for
good.**
`service.rs:381` adopts the agent's `error` into `detail` with no bound,
and `service.rs:415` does the same for a refusal. The service accepts up
to 4 MiB from the agent (`service.rs:57`), while `write_message` refuses
anything over 64 KiB (`scufris-control/src/lib.rs:154`). `set_state`
stores the string, and `register` replays `State { detail }` to every
frontend that connects (`service.rs:538`), so the poison is re-sent
forever. The writer fails, logs at `debug!` (`server.rs:102`) under a
default `info` filter (`logging.rs:13`), breaks, and shuts the socket
down. Every client gets `welcome` and then EOF: the pill, the
conversation window, and `scufris-ctl` alike.

The companion makes it worse. A clean EOF returns `Ok(())`, which resets
the backoff to `MIN_BACKOFF` (`link.rs:98`), so it reconnects four times
a second and clears the conversation window on each `Connected` - and
the replay that would refill it is the message that cannot be written.

Executed by the red team lane against a real service with a stand-in
agent. Bound every string the service puts into a `ServiceBody` at the
moment it adopts it, reusing the char-boundary `truncate` at
`rpc.rs:215`. The invariant is that the service never emits a message
its own reader would reject.

**B2. The conversation window strands the keyboard when it goes away.**
`Hud::windows()` (`hud.rs:281`) passes only the HUD to
`FocusTracker::capture`. `DesktopSurface::windows()` (`main.rs:239`)
passes the pill, the textbox and every widget shell, and its doc comment
says exactly why: a capture that records an unfocusable window hands the
person's keys somewhere they cannot type.

`capture` records `_NET_ACTIVE_WINDOW` unless it is in the list
(`focus.rs:97`). i3 marks the pill active on map even though the pill is
built `focusable(false)`, so the pill is what gets recorded, and
`restore()` activates it. Measured on Xvfb with i3 4.25.1:
`XGetInputFocus` returns `PointerRoot` after the window is put away, and
stays there. `App::look` only watches while `Posture::Editing`, so
nothing recovers it.

The HUD's own comment reasons about which window holds the keyboard.
`capture` reads which window is active. That is the gap.

Give the two surfaces the same window list, or one shared tracker.

**B3. The conversation window is drawn over the textbox and clips the
pill.**
`hud.rs:301` builds the window `always_on_top(false)` on the reasoning
that the pill and textbox, which are `always_on_top(true)`, will stay
above it. i3 does not stack floating windows by `_NET_WM_STATE_ABOVE`;
it echoes the state and ignores it. Last mapped wins.

The geometry is arithmetic, not a measurement. With `BOTTOM_MARGIN = 72`
and `GAP = 24`, on 1920x1080:

| Window  | x span   | y span   |
| ------- | -------- | -------- |
| HUD     | 580-1340 | 260-820  |
| textbox | 650-1270 | 614-754  |
| pill    | 865-1055 | 778-1008 |

The textbox is entirely inside the HUD. The pill's top 42 px are
covered; on 1366x768 it is 198 px, and the privacy ring can be behind
another window with the microphone open.

The worst of it is in `Posture::Editing`. A pill click raises the HUD
over the take the person is editing, takes its keyboard, and leaves
`state.rs` in `Editing` and `App::screen()` at `Screen::Ready`. The
repair chain does not fire because `falls_short` is false at `Ready`,
and the watch does not fire because the HUD holds the keys. The
companion sits there with an invisible, keyless textbox.

Re-assert the pill's and the textbox's stacking after the HUD maps, or
refuse the raise while `Posture::Editing`, or place the window clear of
the pill band. Whichever is chosen, `hud.rs` needs the placement test
`textbox.rs:483` already has.

### MAJOR

**M1. Duplicate submission suppression was dropped, and five comments
still promise it.**
`command()` (`service.rs:733`) keys `pending` by its own `c-{N}`
correlation. The client's `id` is carried for the reply and never looked
up. v2 suppressed by identifier: `accepted: Map<string, Set<string>>`
and `SubmissionConflictError` at `desktop/server.ts:514`, deleted in
this range with nothing to replace it.

Six places still say the service suppresses: `app.rs:546`,
`app.rs:1335`, `app.rs:2365`, `state.rs:105`, `state.rs:358`,
`conversation.rs:57`. Two of them are load-bearing. `clear_pending`
declines to reopen the pill on a failed removal *because* it believes a
resend cannot reach the conversation twice. `process_prefix` spends 16
bytes of OS randomness on the stated grounds that a collision would have
a genuine request refused.

Not a blocker: no path resends on its own. Every resend goes through
`Delivery::Uncertain`, which is non-editable and needs two Enters past
an explicit warning, so the person is always asked.

Either add an accepted-identifier set and refuse a repeat, or rewrite
the six comments to say the identifier is a correlation handle and the
warning is the only guard - and then revisit whether `clear_pending`
should still stay quiet.

**M2. A stale acknowledgement timer freezes a live retry.**
`submit()` (`app.rs:1499`) spawns one 15 s timer per call, keyed only by
`id`, with no cancellation and no generation guard. `take_recording` and
`transcription` both use `capture_generation` for exactly this
(`app.rs:1370`). `state.rs:105` says the identifier is reused by every
retry, and the receiving guard at `state.rs:780` is `*id == uncertain`
alone, so the timers cannot be told apart.

Submit at t=0, refused at t=2, retry at t=3. At t=15 the first timer
fires, matches the second attempt's phase, and freezes a 12-second-old
live submission into `Delivery::Uncertain` with "The backend did not
confirm delivery." and a forced-send warning it has not earned. A retry
issued 14 s after the original gets a one-second deadline.

The existing test `a_daemon_refusal_keeps_the_words_editable_and_
ordinarily_retriable` (`app.rs:2596`) builds this state and stops one
line short. `QueueExecutor::spawn_after` discards the delay and
`expire()` drains in push order, so one added `expire()` shows it.

Not a blocker: a genuine late acknowledgement still retires the pill
through `Retained + Acknowledged` (`state.rs:699`), so the wrong state
is usually transient.

The red team lane reported the opposite - that the identifier and
`Phase::Sent` together make a stale timer harmless. That is true of a
later take, which gets a new identifier. It is not true of a retry,
which reuses one, and the retry is the case that matters.

**M3. The conversation window reads the toolkit for whether it is up,
and gives the keyboard back whether or not it had it.**
`up()` (`hud.rs:318`) reads `window.is_visible()`. The desktop lane's
first law is that a Tauri call is a message onto the GTK loop and
verdicts come from the X server through `display.rs`. The flag survives
i3 unmapping the window on a workspace switch, so on any workspace but
the one it was left on, `toggle` takes the hide path.

`hide()` (`hud.rs:225`) then calls `focus.restore()` unconditionally,
with no counterpart to the `holds_keyboard` guard `show()` has at
`hud.rs:199`.

Measured: window up on workspace 1, person on workspace 2. One press of
the binding answers `taken`, shows nothing, and activates the textbox,
which pulls i3 back to workspace 1. The first press of "show me the
conversation" moves the person off the workspace they were on. Without
workspaces the same unguarded restore is a plain focus steal: open from
the tray in editor E, click into browser B, put the window away, and the
keyboard jumps to E.

Read the display the way every other window in the crate does, and
restore only when the window actually held the keyboard.

**M4. The conversation page throws typed words away before the host
decides.**
`ui/hud.ts:132` sets `words.value = ""` and then invokes `hud_submit`.
`Hud::typed` (`hud.rs:237`) returns early with no `tell()` when
`Conversation::typed` refuses a second line while one is in flight
(`conversation.rs:126`). Nothing comes back: no transcript entry, no
notice, no trouble, no log line.

Three places state the opposite, and it is the whole justification for
refusing rather than queueing. `hud.rs:239`: "The words are still in the
field either way." `conversation.rs:125`: "the field still holds the
words either way." D-HUD-9 says the same.

The blank case is fine - the page guards `text.trim() === ""` before
clearing. Only the in-flight case loses a sentence, and that is the case
the design was written around.

Clear the field when the host confirms the line was taken, or refuse
Enter in the page while the notice says `sending`.

**M5. The `attention` state has no consumer, so a blocked job never
reaches the person.**
`orchestration.ts:211` still emits `ATTENTION_STATE_EVENT`. At
`185034a`, `desktop/index.ts:194` subscribed and reported it, and the
tray painted it wisteria with "Scufris needs you" (`tray.rs:55,104`).
That file was deleted in the inversion and nothing replaced the
subscription. The three surviving references are the constant, the
import and the emit.

`ScufrisState` has no `attention` variant. `assistant-state.ts:6` defers
it to "the textbox increment", which has since landed without it.
`CHANGELOG.md:202` advertises the state in a released section, and the
Unreleased "Removed" block says nothing about its loss.

The tray's `attention` path is still reachable through `Phase::Retained`
(`state.rs:948`), so the state is not dead - only the blocked-job route
to it.

Either route it, or record the removal.

**M6. A sixth wire refusal code lives outside the module that declares
them.**
`refusal` (`scufris-control/src/service.rs:294`) is documented "Stable
refusal codes. A caller branches on these" and holds five. `no_frontend`
is a private `const` in the service binary (`service.rs:76`), sent from
four places. Both TypeScript consumers branch on it
(`service/client.ts:34`, `conversation.ts:71`) and three tests pin it on
the wire. `docs/src/dev/service.md:63` enumerates the same five and
mentions the sixth only in prose further down.

An author of a new frontend or control client who enumerates the module
to cover every refusal will not handle it.

**M7. The two orientation chapters describe the pre-inversion tree.**
`overview.md:9` tells Linux users to select a separate voice-capable
package; `flake.nix` exposes no such attribute and `nix/scufris.nix`
builds one launcher on purpose. `overview.md:15` says five extensions
and names `voice`, where `package.json` lists six, `extensions/scufris/
voice/` is gone, and `conversation` is missing.
`dev/extensions.md:3` already says six, so the two chapters disagree.
`architecture.md:7` lists `voice/` and omits `shared/`, `response.ts`
and `conversation.ts`; `:41` names a speech module deleted in `8813aa4`.

These are the first two pages a reader lands on.

**M8. The conversation page shipped 186 lines of behaviour with no
test.**
`tests/desktop-ui.test.ts:25` compiles the whole `ui/tsconfig.json`
project and reads `dist/pill.js` and `dist/textbox.js`. `dist/hud.js` is
built by the same call and never read; the file header still says "The
two desktop webview pages".

The untested logic is not cosmetic: the follow-scroll threshold
(`hud.ts:63`), the notice precedence (`:99`), Enter against Shift+Enter
(`:143`), the focus rescue (`:154`), and the field clearing (`:126`) -
which is M4. The textbox has pinned tests for its equivalents. AGENTS.md
says to add files with their first tested behaviour.

M4 is the proof that this one is not hypothetical.

### MINOR

**m1. `bounded()` cuts on a UTF-16 index and can make a lone surrogate.**
`client.ts:498` shrinks by `slice(0, floor(length * 0.9))`, which can
land between the halves of an astral character.
`encodeClientMessage`'s `wellFormed` replacer then throws
`not_well_formed` (`protocol.ts:160`), and `tell()` swallows it at
`"info"` (`client.ts:326`), which `service/index.ts:69` does not
surface. The line the function exists to preserve is dropped instead.

Latent, and this is the one point the panel split on. Both emit sites
pass `entry.spoken` through `plainProseParagraph`, which returns
`undefined` above 1000 UTF-8 bytes (`response.ts:62`), or through a
`maxLength: 1000` schema. A thousand code points cannot exceed 4000
bytes, so the loop cannot run today. The craft lane ranked it MAJOR and
tied it to the silent-agent incident; that is an overreach. It becomes
live the moment anything else calls `said` or `speak`, which is one line
away. Cut on code points, and raise the catch for `not_well_formed` to
`warn`.

**m2. A message the service cannot write is logged below the default
level.** `server.rs:102` uses `debug!` where the filter is `info`
(`logging.rs:13`), so the service silently kills connections while still
reporting that it is listening. This is what turned B1 from diagnosable
into opaque. Distinguish `MessageError::TooLarge`, which is the service
having built a message it cannot send, from a peer that went away.

**m3. The companion's command socket is taken and released on the
opposite policy to the service's.** `command.rs:76` removes whatever is
at the path with no liveness probe, where `server.rs:51` refuses with
`AddrInUse` when the socket still answers. `unbind` (`command.rs:130`)
then removes the path unconditionally, so a second companion stopping
takes the first one's socket file with it and `scufris-ctl open` reports
nothing listening while a companion runs. Needs two companions, which
L1 forbids. The asymmetry is the finding, not the removal.

**m4. The pending record is fsynced but the directory entry is not.**
`pending.rs:215` does `sync_all` then `rename` and returns. The rename
that gives the file its name stays in the page cache, so a power loss
after `save()` returned can lose an accepted transcript, against a
module header that says nothing is submitted until a save is known to
have landed. Only a machine-level loss reaches it; process death is
already covered.

**m5. `begin_debug` checks the lease before the role.** `service.rs:791`
answers `debug_held` to a frontend that could never hold the lease;
with no lease held the same client correctly gets `wrong_role`. No
shipped caller reaches it - `scufris-ctl debug` always connects as
control. Swap the two blocks.

**m6. The conversation verb stalls the whole service stream.**
`link.rs:202` calls `observe` inline on the single reader thread, and
`LinkEvent::Conversation` runs `Hud::show()`, which blocks on GTK plus
up to 500 ms of `display::came_up` polling. For that window the frontend
reads no transcript, no state, no answer and no widget command. Nothing
is lost - the socket buffers - but a `Sent` pill and a `sending` window
both wait on answers that are sitting unread. Every other branch hands
off without blocking, and `MENU_VOICE` (`main.rs:395`) already dispatches
off-thread for this reason.

**m7. A configured key equal to the hotkey silently removes activation.**
`PillKeys::new` (`keys.rs:89`) checks neither key against `hotkey`, and
the handler matches `cancels` then `stops` before falling through to
`Event::Activate` (`main.rs:305`). `SCUFRIS_DESKTOP_CANCEL_KEY=Super+D`
beside the default hotkey makes `Super+D` mean Escape, with no warning.
`chosen()` already refuses and logs an unparseable accelerator; this is
the same class of answer.

**m8. `hud` is missing from the verb check.** `nix/checks/service.nix:47`
loops `send state watch abort debug open`. `Spoken::Hud` exists
(`scufris-ctl.rs:90`) and `scufris-ctl hud` is the only way a window
manager reaches the conversation window (D-HUD-4). Renaming or dropping
it passes `nix flake check`.

**m9. Three of four capability labels are unguarded.**
`every_widget_window_label_matches_the_capability_glob`
(`widgets/windows.rs:169`) asserts `widget-*` is in
`capabilities/default.json`. `pill`, `textbox` and `hud` each carry a
doc comment claiming the file names them, and nothing checks. A renamed
label loses its IPC silently. The existing test extends in three lines.

**m10. The environment table claims to be complete and is not.**
`installation.md:207` says "Every value comes from the environment" and
then omits `SCUFRIS_DESKTOP_COMMAND_SOCKET`, which `config.rs:93` reads,
`config.rs:171` prints, `nix/checks/desktop.nix:129` diffs, and
`dev/desktop.md:616` documents.

**m11. Two places still say `voice.enable` changes the agent.**
`README.md:58` says it "lets the agent decide what is worth saying
aloud"; `nix/checks/home.nix:39` says "Voice changes which resources the
agent is handed and nothing else", and the check below it proves the
launcher is unchanged. In `nix/home-manager.nix` the option does two
things: a platform assertion, and appending
`SCUFRIS_DESKTOP_SPEAK_COMMAND` to the desktop unit. `response.ts`
shapes the spoken paragraph with no gate at all.

**m12. Comments that point at things this range deleted.**
`shared/spoken.ts:23` names the speech mode, contradicting its own file
header eight lines above. `service/protocol.ts:8` sends the reader to
`desktop/protocol.ts`. `shared/assistant-state.ts:6` defers to an
increment that has landed. `scufris-desktop/src/config.rs:40` still
calls the service socket the "Daemon control socket", and eight of its
tests use `daemon.sock` paths.

**m13. `tests/*.py` are run by no gate.** `npm test` globs
`tests/*.test.ts` and no derivation under `nix/checks/` runs Python.
This range adds a fourth such file, `test_usage_backends.py`, covering
444 new lines of parsing in the two new backends. All 45 pass when run
by hand. The gap was opened by the three earlier files, not this one;
`dev/maintenance.md:154` documents the runner, and AGENTS.md does not.

**m14. The README doubled and took on documentation the mdBook carries.**
32 lines to 64. It now explains the debug lease and what each module
option means; `dev/service.md:126` carries the first almost verbatim.
AGENTS.md: keep the README to the description and Quickstart. Line 3
also still describes the product without the service.

**m15. Duplication the build forces, recorded only in a comment.**
`native/widgets/claude/widget.ts` and `codex/widget.ts` are 224 lines
identical apart from a six-line header. `deaf()` is the same nine lines
in four backends, and the whole driver is duplicated between the two new
Python ones. `build.rs` `include_str!`s each module whole, so a relative
import would not resolve and the copies are a consequence, not
carelessness. Nothing stops a shared prelude being concatenated at build
time. If the constraint is permanent, say so in `build.rs`.

**m16. Three implementations of cutting a string at a byte bound.**
`rpc.rs:215`, `speech.rs:231`, `client.ts:498`. The two Rust ones are the
same `is_char_boundary` walk in two crates that both already depend on
`scufris-control`, which is where the bound itself lives. The
TypeScript one is m1.

**m17. This skill's own briefs are stale.** They were edited in this
range and half-updated. `lanes/contracts.md:14` names `PROTOCOL_VERSION`,
which exists nowhere in the tree - both sides spell it `SERVICE_VERSION`.
`lanes/contracts.md:25` names `src/review.rs` and `ui/review.css`, both
renamed to `textbox` here. `lanes/desktop.md:26` describes `pill::open`
claiming the keyboard, where the pill is built `focusable(false)` and
`pill::holds_the_keyboard` is the inverse predicate. `lanes/desktop.md:34`
describes an invisible field in `ui/index.html` that has no input at all.
`lanes/red-team.md:30` says a transcript caps at 8 KiB; the shipped
`MAX_TRANSCRIPT_TEXT_BYTES` is 4 KiB.

A lane that greps for a file that does not exist reports a pass. This
round, two lanes did exactly that and caught it themselves.

## Verified

The panel confirmed these are sound, which is most of the range.

- **Protocol v3 on both sides.** Version constant, every `ClientBody`
  and `ServiceBody` tag, role names, and all four byte bounds
  (`MAX_MESSAGE_BYTES` 64 KiB, `MAX_IDENTIFIER_LENGTH` 64,
  `MAX_TRANSCRIPT_TEXT_BYTES` 4 KiB, `MAX_WIDGET_DATA_BYTES` 8 KiB).
  Exercised on the wire at the boundary and one byte over, in both
  directions: all four measure UTF-8 bytes, not characters.
- **Role gating**, full matrix over a real socket. An agent cannot take
  the debug lease, a frontend cannot, a second `hello` is ignored, and
  the role cannot be escalated.
- **Two frontends at once.** The first is displaced and closed promptly
  rather than left orphaned.
- **Socket lifecycle.** A second service against a live socket is
  refused; a stale socket after `kill -9` is removed and rebound; a dead
  service leaves no orphan agent.
- **Concurrency in the service.** No caller holds the `Inner` mutex
  across a blocking write. `try_send` means a client that stops reading
  is dropped rather than blocking the service. Agent supervision is
  generation-guarded throughout.
- **Identifier collisions.** The pill's `{prefix}-{n}` and the
  conversation window's `{prefix}-h{n}` cannot name the same submission,
  and the service's `c-N` lives in Pi's RPC id space.
- **The 15 s acknowledgement timeout** is the right shape: Pi emits the
  `prompt` response on acceptance, not at turn end, so a long turn does
  not trip it.
- **The pending store.** Corrupt, oversized, wrong-version, and
  tombstoned records are all reported rather than mistaken for an empty
  store.
- **Launcher argv**, in all six places it is written.
- **Capability labels** cover every window label created.
- **Versions** are 0.4.0 in all three files.
- **CHANGELOG** covers every breaking module option removed.
- **Frames.** Pill 190x230 and textbox 620x140 match the CSS arithmetic
  on both sides.
- **The raise ordering works.** The conversation window takes the
  keyboard on first and repeat shows, and the pill still refuses it.
- **The pill click reaches `hud_toggle`** through the blob's bounding
  shape.
- Suites: Rust 331 pass, `TMPDIR=/tmp npm test` 72 pass, typecheck
  clean, Python 45 pass by hand.

## Not checked

A skip is not a pass.

- `nix flake check`. The skill forbids it to lanes and it was not run
  in adjudication either.
- Keyboard delivery. The desktop lane confirmed where the X focus is,
  not what a keystroke does with it. No key was injected.
- The configurable keys as actual grabs. `Super+Escape` and
  `Super+Delete` parse and the arrangement is unit-tested, but no
  accelerator was pressed on a display.
- `scufris_conversation` on the wire. `Hud::show`/`hide` were exercised
  through `Verb::Hud`, which is the same call.
- Multi-monitor, HiDPI, scaled placement, and the tray left click.
- A real `pi --mode rpc`. Every service probe used a stand-in agent.
  Whether real Pi can accept a prompt and never answer it is unverified;
  that is the one case that would strand the conversation window's
  `sending` flag, which has no timeout where the pill has 15 s.
- Outbox overflow against a full 200-entry ring. Replay alone cannot
  overflow it (202 into 256); overflow needs a slow reader and about 54
  further pushes. Worth its own pass, because the recovery is the same
  non-widening 250 ms reconnect B1 exposes.
- `Service::submit`'s split lock (`service.rs:714`): the state is read
  under one lock and the command built under another, so a steer can be
  decided against a state that has since changed. No concrete failure
  was stated.
- Whether the orb descriptions in `dev/desktop.md:177` still match what
  the vendored engine draws. State names and speeds were checked; the
  shapes need rendering.
- Live acceptance on the real i3 desktop after a home-manager rebuild,
  which the task still has open.

## Proofs rerun

- `cd native && TMPDIR=/tmp nix develop --offline -c cargo test
  --workspace` - 331 pass, 0 fail.
- `TMPDIR=/tmp npm test` - 72 pass, 0 fail.
- `TMPDIR=/tmp npm run typecheck` - clean.
- `python3 -m unittest discover -s tests` - 45 pass (run by hand; no
  gate runs these).
- Xvfb 1920x1080 with i3 4.25.1, `focus_follows_mouse yes` and
  `focus_on_window_activation smart`, the real companion binary at
  `a13cb38`, instrumented with `XGetInputFocus`, `_NET_ACTIVE_WINDOW`,
  `XQueryTree`, `WM_HINTS`, and XTest.
- A scratch `scufris-service` on a private socket driven by stand-in RPC
  agents, for the bounds, the role matrix, the socket lifecycle, and B1.

All helper processes were stopped by recorded PID. The working tree is
unchanged.
