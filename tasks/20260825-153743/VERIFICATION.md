# Verification

Date: 2026-08-25. Reference: `tasks/20260822-132001/scufris-hud.html`.

## Every state renders on the design grammar

The real `ui/pill.js` render path was driven headlessly: a harness page stubs
`window.__TAURI__`, loads the shipped `pill.css` and `pill.js` unchanged, and
pushes one `scufris://presentation` payload per state (plus one
`scufris://tick` for listening). Chromium `--headless=new` screenshotted all
twelve states at the pill window size (560x96).

- `listening.png`: yellow accent, red privacy ring, canvas wave at the tick
  level, timer, no text.
- `review.png`: quartz accent, editable transcript with underline and accent
  caret, near-still orb.
- Checked the same way: transcribing (brown, dimmed wave, no timer), working
  (niagara, label shimmer), speaking (green, wave), attention (wisteria,
  detail line), error (red, still orb, plain words), disconnected
  (desaturated gray), retained and uncertain (wisteria, attention-class),
  sent, idle.

## Grayscale and reduced motion

- `grayscale.png`: the eight main states desaturated. Each stays legible by
  elements, not hue: ring + wave + timer = listening; dimmed wave =
  transcribing; text + caret = review; label shimmer = working; wave without
  ring or timer = speaking; the rest differ by label and detail.
- `listening-reduced.png`: captured under
  `--force-prefers-reduced-motion`. Animations are off (solid ring, no
  breathing), the glow and wave still show the tick level, drawn directly on
  each tick without the spring.

## Motion and cost policy in the code

- `display += (target - display) * 0.25` in `pill.js frame()`, target from
  the 60 ms `scufris://tick` while listening, synthesized while speaking (no
  TTS level exists yet), per-state baseline otherwise.
- `requestAnimationFrame` runs only in listening and speaking
  (`REACTIVE` set); every other state settles once in `retune()`. Reduced
  motion never starts the loop.
- Canvas 2D only; no WebGL anywhere in the webview.

## Earcons

Four cues in `pill.js CUES`, played on `data-state` transitions only: mic
open (rising 520-760 Hz), mic close (falling), attention chime (also for
retained and uncertain, which the tray already presents as attention-class),
error low tone. An error crossing swallows the mic-close cue so one boundary
makes one sound. Gains are 0.02-0.06. Nothing plays for working, speaking,
sent, or disconnected.

The mute switch is the tray item "Mute sound cues": `CueSwitch(AtomicBool)`
in `main.rs`, read once at webview start through the `pill_cues` command,
pushed on toggle over `scufris://cues`, logged at INFO. It is session-scoped;
every start ships cues enabled.

## Tray follows the same grammar

`tray::state_color` moved to the gruber palette (yellow listening, brown
transcribing, niagara working, green speaking, wisteria attention, red only
for error) and the privacy ring became grammar red `#f43841`. New tests:
`red_is_reserved_for_error_and_the_mic_ring`,
`the_cue_switch_offers_the_opposite_of_the_current_enablement`.

## Checks run

- `cargo test` (desktop workspace): 98 scufris-desktop tests pass, plus
  scufris-control.
- `cargo clippy --all-targets`: no warnings. `cargo fmt --check`: clean.
- `npx prettier --check .`: clean.
- `nix build .#scufris-desktop`:
  `/nix/store/wkqax65km0ch6xj9wgh0g784rv80fj38-scufris-desktop-0.4.0`; the
  packaged binary answers `--version` and takes the error path on an invalid
  endpoint probe (exit 1 through tracing).

## Left for live playtesting

- The four earcons audibly firing at their boundaries in WebKitGTK, and the
  tray mute silencing them. Web Audio autoplay policy in the webview is the
  one thing the headless harness cannot prove.
- Glow and wave riding the real microphone level, and the one-frame reaction
  to Super+D.
- The side-by-side comparison with the design page on the real desktop.
