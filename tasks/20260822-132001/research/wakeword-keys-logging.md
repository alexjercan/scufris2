# Research track: wake word, focus-free keys, observability

Agent report, 2026-08-25. Raw findings; synthesis lives in ../RESEARCH.md.

## Area 1: Wake word / voice activation on Linux

### Engine survey

**openWakeWord** (https://github.com/dscripka/openWakeWord)

- Code Apache-2.0; pre-trained models CC BY-NC-SA 4.0 (non-commercial). Fine for a personal assistant; self-trained models carry no such restriction.
- Accuracy target: <5% false rejects, <0.5 false accepts/hour with threshold tuning. This is the engine Home Assistant standardized on for real-world accuracy (https://www.home-assistant.io/voice_control/about_wake_word/).
- CPU: one Raspberry Pi 3 core runs 15-20 models in real time. On a desktop x86 this is negligible (well under 1% of a core per model). Audio: 16 kHz 16-bit mono, 80 ms frames, `model.predict(frame)` streaming API.
- Custom words: fully synthetic training (Piper TTS), Colab notebook, model in under an hour. English only today.
- Runtime: Python with ONNX Runtime or tflite-runtime. Not embeddable in Rust without carrying ONNX; natural shape is a separate process.
- Nix: packaged as `wyoming-openwakeword` in nixpkgs with a NixOS module `services.wyoming.openwakeword` (threshold, triggerLevel, preloadModels, customModelsDirectories, uri) (https://mynixos.com/nixpkgs/options/services.wyoming.openwakeword). One known pitfall: SIGILL on CPUs without expected SIMD (https://github.com/NixOS/nixpkgs/issues/358973).

**microWakeWord** (https://esphome.io/components/micro_wake_word/, https://github.com/kahrendt/microWakeWord)

- Inception-based streaming tflite models built for ESP32-S3; lower latency and better false-accept rates than openWakeWord in HA's own tests (about 0.187 false accepts/hour on the Dinner Party Corpus) (https://www.home-assistant.io/blog/2024/06/26/voice-chapter-7/).
- Ecosystem and tooling are ESPHome-shaped. Running it on a desktop means driving tflite yourself with community models. Viable but off the paved path. Best regarded as the "satellite firmware" engine, not a desktop library.

**Picovoice Porcupine** (https://github.com/Picovoice/porcupine)

- Best-in-class accuracy reputation and tiny CPU cost, 9+ languages, custom words via their console.
- Licensing disqualifies it: repo wrapper is Apache-2.0 but the engine needs an AccessKey validated at init, with periodic online activation since v2.0. The free tier AccessKeys were shut off on June 30, 2026 (https://community.home-assistant.io/t/fyi-picovoice-confirmed-free-tier-accesskeys-will-stop-working-after-june-30-2026/1012744). A key-gated, phone-home proprietary blob is a poor fit for a NixOS local-first assistant.

**Vosk keyword spotting** (https://github.com/alphacep/vosk-api)

- Apache-2.0, packaged widely, 20+ languages: the best multilingual option. Grammar mode restricts the WFST search to a word list, cutting CPU (https://zenn.dev/diced/articles/vosk-silero-vad-wakeword-android?locale=en). OVOS ships a Vosk wake word plugin (https://github.com/OpenVoiceOS/ovos-ww-plugin-vosk).
- But it is full ASR running always-on: markedly more CPU and RAM (small model ~50 MB resident) than a dedicated spotter, and more prone to false accepts on short words. Sensible only for a non-English wake word.

**Snowboy**

- Dead. Online model training and CDN went offline January 1, 2021; only legacy pre-trained models still work (https://github.com/Kitt-AI/snowboy). Do not build on it.

**Home Assistant satellite pipeline** (https://github.com/rhasspy/wyoming-satellite)

- Architecture worth copying: the satellite process owns the mic (`arecord -r 16000 -c 1 -f S16_LE -t raw` piped in) and talks to a separate wake word service (wyoming-openwakeword) over the Wyoming protocol on TCP (`--wake-uri tcp://127.0.0.1:10400`). On detection it fires `--detection-command` (wake word name on stdin) and optionally plays `--awake-wav`. Both processes run as systemd services. This is exactly the "separate service signals the app" shape.

**Rustpotter** (https://github.com/GiviMAD/rustpotter)

- Apache-2.0, pure Rust, on crates.io: the only engine that embeds directly into a Tauri backend with zero extra runtime. Two modes: DTW references from 3-8 personal WAV samples (quick, weaker), or a trained small NN model (better). Accepts any sample rate, resamples to 16 kHz f32 internally, 30 ms chunks. `Rustpotter::new(config)`, `add_wakeword_from_file()`, `process(buffer) -> Option<RustpotterDetection>`.
- Reputation: fine for a personal, single-speaker wake word (it is speaker-adapted by construction); less robust than openWakeWord across voices, noise, and distance. Used by openHAB as a KS option (https://www.openhab.org/addons/voice/rustpotterks/). Maintenance is slow but the crate is stable.

### Mic sharing under PipeWire

Non-issue. PipeWire is a graph: multiple client streams can consume the same source node concurrently, unlike raw ALSA which needs dsnoop (https://docs.pipewire.org/page_overview.html, https://wiki.archlinux.org/title/PipeWire). A wake word listener at 16 kHz mono and the whisper recorder can hold independent capture streams on the same mic; PipeWire resamples per stream. So "wake service always listening + app records on demand" needs no routing tricks. The only care point is device selection: both should target the same default source so a headset swap moves both.

### Privacy expectations

Always-listening should be visible. i3 has no built-in mic indicator (GNOME/KDE draw one), but i3status-rust ships a `privacy` block with a PipeWire driver that shows when any app captures the mic (https://github.com/greshake/i3status-rust/blob/master/NEWS.md). Recommended posture: wake listening off by default, an explicit toggle, a persistent bar indicator while enabled, and a distinct pill state when actively streaming to whisper. PipeWire's stream visibility (`wpctl status`) means the user can always audit who holds the mic.

## Area 2: Focus-free keys under i3/X11

### Mechanisms

**Tauri global-shortcut plugin** (https://v2.tauri.app/plugin/global-shortcut/)

- Wraps the `global-hotkey` crate, which uses XGrabKey: a passive grab on keysym+modifiers on the root window. X11 only; the shortcut thread is explicitly disabled on Wayland to avoid libX11 segfaults (https://github.com/tauri-apps/tao/pull/543), and Wayland has no protocol for it yet (https://github.com/tauri-apps/tauri/issues/3578).
- XGrabKey semantics: a grabbed key is delivered only to the grabber, system-wide, and a second client grabbing the same combo gets BadAccess. Registering plain Escape/Enter permanently would swallow them for every app and destroy normal typing. Registering them only while the pill is visible and unregistering on hide is workable, but you are racing i3 (which also grabs keys) and any registration failure leaves dead keys with no indicator.

**i3 binding modes** (https://i3wm.org/docs/userguide.html)

- `mode "scufris" { bindsym Escape ...; bindsym Return ... }` swaps the active binding set. Bound keys are captured regardless of window focus; all unbound keys pass through to the focused window, so the user keeps typing in their editor while the pill listens. i3bar shows the mode name, giving a free visual indicator. i3 owns all grabs, so there are no BadAccess conflicts and no stuck-grab failure mode. Sway supports the identical syntax, so this ports to Wayland-on-sway unchanged.
- Wiring: `bindsym $mod+d exec scufris-ctl open; mode "scufris"` and inside the mode Escape/Return exec `scufris-ctl cancel|accept; mode "default"`. The app needs a tiny control channel (unix socket or `i3-msg`-driven) and must run `i3-msg mode default` itself whenever the pill closes for any other reason (timeout, click, tray) so mode and UI never desync.

**Push-to-talk (hold Super)**

- Needs key release detection. XGrabKey-based PTT historically breaks focus during the hold (https://bugs.launchpad.net/bugs/714696). The grab-free options are XInput2 raw key events, selectable only on the root window (https://lists.freedesktop.org/archives/xorg/2020-May/060269.html), or polling the keymap the way Discord does. i3 has `bindsym --release` but modes cannot express "while held" cleanly. Under Wayland this whole class needs compositor help or evdev access (https://gitlab.gnome.org/GNOME/gnome-shell/-/issues/2838). Treat PTT as a later, X11-specific experiment.

### Focus behavior of launchers

rofi does not rely on WM focus at all: it calls XGrabKeyboard and takes every key while open, dismissing on Escape (https://davatorium.github.io/rofi/1.7.3/rofi.1/). That is why i3 configs bind it `--release` (the grab fails while the hotkey is still held) (https://github.com/davatorium/rofi/issues/709). This "modal grab" model is right for a launcher you type into, and wrong for a voice pill: while recording, the user has nothing to type at the pill, and stealing the keyboard blocks their editor. The pill should be the opposite: a no-focus overlay (i3 `no_focus` / floating rule) plus the two keys captured by the i3 mode. Bonus: since the pill never takes focus, there is nothing to restore on close.

### What breaks, honestly

- While the mode is active, Escape and Enter go to the pill, not the focused app. Vim users will feel a stray Escape. Mitigations: keep the pill's open window short (auto-close after response), use `$mod+Escape`/`$mod+Return` instead of bare keys if it bites, and the i3bar mode label makes state obvious.
- i3 modes are config, not API: scufris must ship an i3 config snippet, and users of other X11 WMs need the global-shortcut fallback.

## Area 3: Observability for a Rust/Tauri systemd user service

### journald from Rust

- `tracing` + `tracing-journald` is the current standard: a `tracing_subscriber::Layer` that speaks the native journal protocol, preserves structured fields (sanitized to journald naming), and emits CODE_FILE, CODE_LINE, and TARGET automatically (https://docs.rs/tracing-journald/latest/tracing_journald/). Structured fields mean `journalctl --user -u scufris-desktop -o json | jq .SESSION_ID` works, which plain stderr logging cannot give.
- The lazy alternative: under a unit, stderr is already piped to the journal (https://systemd.io/JOURNAL_NATIVE_PROTOCOL/), and small daemons like wluma just use env_logger to stderr with RUST_LOG (https://github.com/maximbaz/wluma). You lose per-field metadata and correct PRIORITY levels (everything lands at the default priority unless you emit `<N>` sd-daemon prefixes).
- Level policy for a useful `journalctl --user -u scufris-desktop`: ERROR = user-visible failure; WARN = degraded (wake service unreachable, mic lost); INFO = lifecycle and state transitions only (started, pill open, recording start/stop, transcript length, wake detection) - a quiet steady state; DEBUG = per-request detail (whisper timings, PipeWire node ids); TRACE = audio frame plumbing. Default INFO in service mode, honoring RUST_LOG/EnvFilter overrides.

### Dual-mode pattern

The canonical shape, straight from tracing-journald's docs: try the journald layer, fall back to fmt.

```rust
let registry = tracing_subscriber::registry().with(EnvFilter::from_default_env());
match tracing_journald::layer() {          // Err if no journald socket
    Ok(j) if !force_foreground => registry.with(j).init(),
    _ => registry.with(fmt::layer().with_ansi(stderr().is_terminal())).init(),
}
```

Robust auto-detection: `tracing_journald::layer()` fails off-systemd, and `$JOURNAL_STREAM` (dev:inode of stderr, set by systemd >= 231) confirms stderr is journal-connected (https://systemd.io/JOURNAL_NATIVE_PROTOCOL/). A `--foreground` flag (or `--log pretty|journald|json`) should force the fmt layer so `nix run .#scufris-desktop -- --foreground` always gives colored human logs; auto-detect covers the rest.

### Tauri specifics

tauri-plugin-log (https://v2.tauri.app/plugin/logging/) builds on the `log` crate, not `tracing`, with targets Stdout/Stderr/LogDir/Folder/Webview. Two useful directions: `attachConsole()` shows Rust logs in the webview devtools; the documented `forwardConsole` pattern wraps `console.log/warn/error` in the frontend and routes them through the plugin into the Rust log stream - which then flows to journald. Since the plugin is `log`-based, bridge it into tracing with `tracing-log` (or skip the plugin's Rust side and just use its JS-to-Rust forwarding, letting `tracing` own the backend). Webview console lines should log at DEBUG under a `webview` target so they are filterable.

## Recommendations

**1. Wake word: openWakeWord as a separate systemd user service, Rustpotter as the embedded fallback.**
Porcupine is out (free tier dead since June 2026, key-gated blob). Snowboy is dead. Vosk is the multilingual escape hatch only. openWakeWord is the accuracy/ecosystem winner and is already in nixpkgs (`wyoming-openwakeword` plus a NixOS module), matching the AGENTS.md preference for small deterministic helpers outside the app. Integration shape, copied from wyoming-satellite: a `scufris-wakeword.service` user unit owns its own PipeWire capture stream (PipeWire shares the mic natively, so this coexists with the recorder), runs openWakeWord on 80 ms frames, and on detection pokes scufris-desktop over its unix control socket - the same socket the i3 mode uses - which opens the pill and starts recording. Keep it opt-in, and pair it with the i3status-rust `privacy` PipeWire block so always-listening is visible. If shipping a Python service ever chafes, Rustpotter (Apache-2.0 crate) embeds directly in the Tauri backend with personal-sample models; accept somewhat lower robustness.

**2. Focus-free keys: i3 binding mode, with the Tauri global-shortcut plugin as the non-i3 X11 fallback.**
`$mod+d` opens the pill and enters mode "scufris"; Escape/Return in the mode exec `scufris-ctl cancel|accept` and return to default. The pill window gets a `no_focus` floating rule so the user keeps typing in their app; i3bar shows the mode as a free indicator; no XGrabKey conflicts, no stuck grabs. The app must exit the mode (`i3-msg mode default`) whenever it closes the pill itself. Fallback for non-i3 X11: register Escape/Enter via tauri-plugin-global-shortcut only while the pill is visible. Wayland path: sway runs the identical mode config; on other compositors use compositor bindings or the org.freedesktop.portal GlobalShortcuts portal once relevant, since XGrabKey is a dead end there.

**3. Logging: tracing + tracing-journald + EnvFilter, journald auto-detect with a --foreground override.**
Crates: `tracing`, `tracing-subscriber` (env-filter, fmt), `tracing-journald`, `tracing-log` (bridge for `log`-crate deps including tauri-plugin-log). Init: try `tracing_journald::layer()`; on failure or `--foreground`, use a fmt layer with ANSI when stderr is a TTY. Levels: INFO default in service mode (lifecycle and state transitions only), DEBUG per-request detail, WARN degraded, ERROR user-visible failure; RUST_LOG overrides everything. Frontend: forwardConsole pattern from the Tauri logging docs routes webview console into the Rust stream at DEBUG under a `webview` target. Result: `journalctl --user -u scufris-desktop` is quiet and structured, `journalctl -o json` exposes fields, and `nix run .#scufris-desktop -- --foreground` gives pretty colored logs from the same binary.
