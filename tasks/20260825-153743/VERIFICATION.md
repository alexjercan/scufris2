# Verification

Date: 2026-08-25. Reference: `tasks/20260822-132001/scufris-hud.html`.
Round 2 folds in the live playtest findings: opaque exact-fit window,
audible earcons with logging, the TypeScript port, and the passive pill
that watches the turn it started.

## Every state renders on the design grammar

The real render path was driven headlessly: a harness page stubs
`window.__TAURI__`, loads the shipped `pill.css` and the `pill.js` that tsc
compiled from `ui/pill.ts`, and pushes one `scufris://presentation` payload
per state (plus repeating `scufris://tick` for listening, as production
sends). Chromium `--headless=new` screenshotted all twelve states at the
pill window size (560x64).

- `listening.png`: yellow accent, red privacy ring, canvas wave at the tick
  level, timer, no text.
- `review.png`: quartz accent, editable transcript with underline and accent
  caret, near-still orb.
- Checked the same way: transcribing (brown, dimmed wave, no timer), working
  (niagara, label shimmer), speaking (green, wave), attention (wisteria,
  detail line), error (red, still orb, plain words), disconnected
  (desaturated gray), retained and uncertain (wisteria, attention-class),
  sent, idle.

## The window is the panel

`pill.rs` builds the window opaque at exactly 560x64 and the page fills it
edge to edge; the glow became an inset shadow. Round 1 drew transparent
margins around the panel, and the live playtest showed them black: the bare
i3/X11 session runs no compositor, so per-pixel alpha is discarded. The
opaque exact-fit window needs nothing from the environment. A compositor
would only ever be needed to move the glow back outside the panel.

The size is pinned with min == max inner-size hints on a window left
resizable. Round 2's playtest showed the panel as a big block: `xwininfo`
measured the window at 560x200, and `WM_NORMAL_HINTS` showed the app itself
publishing a 200 minimum. A plain-GTK reproduction (`resize(560, 64)`,
`set_resizable(false)`, a WebKit view in a box) confirmed the mechanism: the
WebKitGTK view's natural height is 200, a non-resizable GTK window ignores
the requested size and takes the natural size, and GTK clamps size hints up
to it. The same reproduction with the window resizable and min == max
geometry hints comes up at 560x64, so that is what `ensure` builds; the
equal hints keep the person from resizing and make i3 float the pill.
Verified live on the i3 session: the pill maps at `560x64+680+944`,
bottom-center with the 72 px margin on the 1920x1080 monitor, and a screen
capture of the mapped pill matches the harness renders.

Known pre-existing flake, out of scope here: the start()-recovery path
(pending transcript found at startup) can log "the pill did not come up"
because `show` asks `is_visible` before the event loop has mapped the
window. It predates this task (the unmodified packaged binary does it too),
only affects recovery-at-startup, and the abandon path keeps the transcript
on disk as designed.

## Grayscale and reduced motion

- `grayscale.png`: the eight main states desaturated. Each stays legible by
  elements, not hue: ring + wave + timer = listening; dimmed wave =
  transcribing; text + caret = review; label shimmer = working; wave without
  ring or timer = speaking; the rest differ by label and detail.
- `listening-reduced.png`: captured under
  `--force-prefers-reduced-motion`. Animations are off (solid ring, no
  breathing); the glow and wave still show the tick level, drawn directly on
  each 60 ms tick without the spring.

## The pill is a resident HUD

Round 3, from the second playtest: the pill still left the screen when the
assistant went idle, and Alex wants visibility to be theirs alone - "it
should be a user action Esc to close it". So the watch became residency.
`Phase::Hidden` is now `Phase::Resting` (the journal line that read
`phase sent -> hidden` was describing a visible pill), and a `dismissed`
flag owns visibility: the first activation clears it, `Escape` sets it, and
nothing else touches it. The resting pill sits passive showing the resting
presentation - idle, working, speaking, attention, disconnected - which the
tray already computed; the acknowledgment path now settles to resting
instead of closing, and `Enter` on an error acknowledges it (rests) while
`Escape` dismisses. A disconnect is shown, not hidden. Dismissing a resting
passive pill takes `Super+D` then `Escape`, since a passive pill holds no
keys.

Tests: `the_pill_rests_on_screen_through_and_after_a_turn`,
`assistant_activity_never_raises_a_dismissed_pill`,
`only_the_persons_escape_dismisses_the_resting_pill`; the acknowledgment
tests now expect no hide and an `idle` presentation on a pill still up.

The postures came out of round 2 and stand unchanged underneath: Focused
holds the keyboard for the interactive phases, Passive is up without it, Off
is down. The handoff restores focus without hiding
(`an_accepted_transcript_is_persisted_before_it_is_submitted` expects
`["show", "restore"]` after submit), `lower` restores focus only from a pill
that holds it, and `falls_short` treats a passive pill holding the keyboard
as wrong, so the repair chain gives the keys back. Mic safety is unchanged:
Passive requires the window to be seen, and recording still requires the
pill up. Round 2's engagement scoping (watch the turn, leave at idle) is
gone; residency replaced it.

## Earcons

Four cues in `ui/pill.ts CUES`, played on `data-state` transitions only: mic
open (rising 520-760 Hz), mic close (falling), attention chime (also for
retained and uncertain), error low tone. An error crossing swallows the
mic-close cue so one boundary makes one sound. Round 1 gains (0.02-0.06)
were confirmed real but inaudible in the playtest - whisper transcribed the
mic-open cue as "_Ding_" - so they rose to 0.05-0.16, soft but audible.
Every cue logs at DEBUG under the `webview` target, and an audio context the
autoplay policy keeps suspended is reported once at WARN, so silence is
diagnosable from journalctl.

The mute switch is the tray item "Mute sound cues": `CueSwitch(AtomicBool)`
in `main.rs`, read once at webview start through the `pill_cues` command,
pushed on toggle over `scufris://cues`, logged at INFO. It is session-scoped;
every start ships cues enabled.

## TypeScript

`ui/pill.js` became `ui/pill.ts` under the ui-local strict `tsconfig.json`
(DOM lib, `noUncheckedIndexedAccess`). `build.rs` compiles it with `tsc`
(falling back to `npx --no-install tsc`) and assembles `ui/dist` - compiled
script plus copied `index.html` and `pill.css` - which `tauri.conf.json`
embeds as `frontendDist`. `pkgs.typescript` joined the package
`nativeBuildInputs` and the dev shell. `ui/dist` is git- and
prettier-ignored.

## Tray follows the same grammar

`tray::state_color` on the gruber palette, ring at grammar red `#f43841`,
tests `red_is_reserved_for_error_and_the_mic_ring` and
`the_cue_switch_offers_the_opposite_of_the_current_enablement`.

## Checks run

- `cargo test` (desktop workspace): 101 scufris-desktop tests pass, plus
  scufris-control.
- `cargo clippy --all-targets`: no warnings. `cargo fmt --check`: clean.
- `npx prettier --check .`: clean.
- `nix build .#scufris-desktop` builds with the frontend compiled inside the
  sandbox; the packaged binary answers `--version`.

## Left for live playtesting

- The raised earcons audibly firing, and the journal `cue ...` lines
  appearing with them.
- The pill lingering through working and speaking after `Enter`, closing on
  idle, and never appearing for a turn typed in the chat.
- The window sitting flush with no black margins.
