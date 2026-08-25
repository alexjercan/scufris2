# Market research: pill redesign, voice UX, and what comes after v1

Date: 2026-08-25. Method: four parallel research sweeps (competitive UI scan,
design guidance and implementation techniques, Linux activation and
observability mechanics, sticky-feature evidence). Raw reports with all
sources live in `research/`. This file is the synthesis and the
recommendations.

## Baseline measured against

- The pill today is a translucent dark box. Motion is one 26 px circle scaled
  by mic level plus a blinking red ring. State is accent color only.
- Rust sends the webview one scalar (peak level, 60 ms tick) as `--level`.
- Logging is bare `eprintln!`. No levels, no journald awareness.
- `nix run .#scufris-desktop` exists; there is no `--foreground` or log
  control.
- The Linux field: no tool has an audio-reactive pill. The ecosystem ceiling
  is waybar icons and bare text overlays (whisper-overlay, hyprwhspr). A
  Siri-class reactive pill is differentiating on its own.

## 1. Pill redesign (the "gray box" fix)

Recommended architecture (converged on independently by two tracks):

- Stay CSS + small Canvas 2D. Avoid WebGL: WebKitGTK silently falls back to
  software rendering with a masked renderer string, and transparent windows
  have known NVIDIA bugs. Tauri's own docs say ship a non-WebGL fallback on
  Linux. Canvas 2D is GPU-accelerated since WebKitGTK 2.46 (Skia).
- Three layers in the existing pill:
  1. Orb core, always present: layered conic/radial gradients rotated via a
     registered `@property --angle`, soft glow. State swaps palette and
     motion pattern. Compositor-driven, near zero cost.
  2. Audio-reactive layer, listening and speaking only: siriwave-style
     Canvas 2D waveform, or a reactive border glow around the pill
     (VoiceOrbs "Edge Glow" recipe). Spring the displayed amplitude toward
     the 60 ms tick level inside requestAnimationFrame:
     `display += (target - display) * 0.25`. Run rAF only in these states.
  3. Transcript layer: live partial text is the strongest "it hears me"
     signal (whisper-overlay's lesson). The orb must never be the only
     progress signal for more than about 1 s.
- If richer motion is wanted later, extend the Rust tick with 3-4 band
  energies. Do not stream FFTs to JS and do not open the mic from the
  webview (getUserMedia is broken under wry on Linux).
- One continuous element that morphs between states. Discrete state swaps
  read as lag (Mycroft's documented failure). The pill must react within one
  frame of Super+D; latency is the first animation (Aqua Voice's lesson).

State grammar. Copy the two best documented systems: Alexa's attention-state
rules (input states are audio-reactive, processing states self-animate, red
is reserved for mic-off/error) and Home Assistant Voice PE's ring grammar
(speed encodes intensity, direction encodes who is talking, position encodes
persistent flags, arc length encodes scalars).

| State        | Motion                                                                      | Color             | Sound                             |
| ------------ | --------------------------------------------------------------------------- | ----------------- | --------------------------------- |
| listening    | glow/waveform tracks mic level                                              | cyan/blue         | short rising earcon on mic open   |
| transcribing | waveform collapses into orb; shimmer; partial text streams                  | blue to violet    | short falling earcon on mic close |
| review       | near-still, faint breathing; caret is the live element                      | neutral, low glow | none                              |
| working      | slow self-driven gradient travel, sharp head, diffuse tail (Gemini shimmer) | violet            | none                              |
| speaking     | pulse tracks TTS output level; motion direction reversed vs listening       | teal/green        | none                              |
| attention    | two soft bright pulses, then slow loop                                      | amber             | one gentle chime, once            |
| error        | quick flash, 2-3 fast shallow pulses, settle dim; plain-language line       | red               | one low tone                      |
| disconnected | desaturated gray, very slow breathing                                       | gray              | tone on transition only           |

Rules that came up in every source: red only for error/mute; sounds only at
boundaries the eye can miss (mic open, mic close, attention, error), never
for working or speaking; honor prefers-reduced-motion (crossfade instead of
motion; WebKitGTK maps it from GTK settings); every state must survive
grayscale (motion pattern, not hue, is the differentiator).

Interaction details worth copying: elapsed timer next to the waveform
(Raycast); recording surface inert to clicks so a stray click cannot kill a
take (Wispr Flow); a 6 px secondary status dot for substate (Superwhisper);
theme-matching the user's accent color (hyprwhspr); visualizer as a
selectable style over one state machine, later (VoiceInk).

References to copy from, all MIT: kopiro/siriwave (Canvas 2D waveform with
setAmplitude), SmoothUI Siri Orb (conic-gradient orb with this exact state
vocabulary), aaaa-zhen/siri-glsl (timing reference for the modern Apple
look), VoiceOrbs (Edge Glow, Halo, Waveform Ring recipes).

## 2. Diegetic feedback with real logs (the journalctl + CLI ask)

- Adopt `tracing` + `tracing-subscriber` (env-filter, fmt) +
  `tracing-journald`, with `tracing-log` bridging log-crate deps. Replace
  `eprintln!`.
- Init: try `tracing_journald::layer()`; on failure or `--foreground`, use a
  fmt layer with ANSI when stderr is a TTY. Same binary both ways, so
  `nix run .#scufris-desktop -- --foreground` gives pretty colored logs and
  the service gives structured journald fields.
- Level policy: INFO = lifecycle and state transitions only (quiet steady
  state); DEBUG = per-request detail (whisper timings); WARN = degraded;
  ERROR = user-visible failure. RUST_LOG overrides everything.
- Forward webview console into the Rust stream at DEBUG under a `webview`
  target (Tauri forwardConsole pattern) so pill JS is debuggable from
  journalctl too.

## 3. Focus-free keys (Escape without mouse focus)

Recommendation: i3 binding mode, not XGrabKey.

- `bindsym $mod+d exec scufris-ctl open; mode "scufris"`; inside the mode,
  Escape and Return exec `scufris-ctl cancel|accept; mode "default"`.
- The pill window gets a no_focus floating rule. The user keeps typing in
  their editor while recording; there is no focus to restore on close.
- i3bar shows the mode name: a free state indicator. No grab conflicts, no
  stuck-grab failure mode. Sway runs identical config, so this survives a
  Wayland move.
- The app must run `i3-msg mode default` whenever it closes the pill for any
  other reason, or mode and UI desync.
- Fallback for non-i3 X11: tauri-plugin-global-shortcut, registered only
  while the pill is visible. Push-to-talk (hold Super) is a later X11-only
  experiment; every mechanism for it is fragile.
- Honest cost: while the mode is active, bare Escape/Return do not reach the
  focused app. If that bites (vim), switch the mode bindings to
  $mod+Escape/$mod+Return.

## 4. Wake word (voice activation)

- Engine: openWakeWord as a separate systemd user service. Already in
  nixpkgs (`wyoming-openwakeword` + NixOS module). Porcupine is
  disqualified (key-gated blob; free-tier keys dead since 2026-06-30).
  Snowboy is dead. Vosk only if a non-English wake word is ever needed.
  Rustpotter (pure Rust, Apache-2.0) is the embedded fallback if shipping a
  Python service chafes.
- Shape, copied from wyoming-satellite: the wake service owns its own
  PipeWire capture stream (PipeWire shares one mic across streams natively)
  and on detection pokes the companion over a control channel, which runs
  the same start action Super+D runs - exactly what TASK.md already
  reserved.
- Posture: off by default, explicit toggle, persistent bar indicator while
  enabled (i3status-rust privacy block shows PipeWire capture), distinct
  pill state while streaming to whisper.
- Sequencing per the feature evidence: do not build this before the hotkey
  path saturates. Wake misses are the top documented cause of voice
  abandonment; push-to-talk sidesteps them.

## 5. What to add next (evidence-ranked)

Five durability laws from the feature sweep: reactive beats proactive; the
hotkey is the product; local tools are the differentiator; sub-1.5 s decides
voice adoption; ambient surfaces survive as dense live state, not decorative
canvases.

Ranked backlog (full evidence in research/product-features.md):

1. Dictation-everywhere (small): a pill mode that types the transcript into
   the focused window instead of the agent. Strongest daily-habit evidence
   in the study; makes Super+D hourly muscle memory.
2. Fast-verb tier with agent fallback, led by timers (medium): deterministic
   intents (timers, open/focus, mute, brief me) answered under 1.5 s with an
   earcon; everything else falls through to Pi. Timers become the first
   dashboardd widget content.
3. "Look at this" context capture (small): snapshot the focused window
   (maim), or Kitty scrollback + cwd as text, into the session. Kills the
   copy-paste loop, which is the actual current friction.
4. Summonable HUD - the dashboardd embed (large): hotkey-summoned overlay of
   agent-fed widgets (briefing, timers, den references, stateful action
   tiles). Never an always-on canvas; the agent maintains the feeds, so no
   per-widget config rot. This intersection is unclaimed on Linux.
5. Morning briefing, pull-first (medium): user-authored timer, actionable
   items only, every line linked to its source, one spoken sentence at most,
   per-topic mute from day one; plus "catch me up" on demand.
6. Explicit memory verbs (medium): "remember this" filing into the den and a
   recall verb. Explicit curated memory retains; continuous capture is the
   thing people uninstall (Recall/Rewind verdict).
7. Turn-taking and the speak/chime split (small): reopen the mic without
   re-activation when the reply is a question; chime acks, speak only
   answers. Decides whether voice survives week two.
8. Presence layer (small): reactive state animation on tray/pill bound to
   real assistant state, coherent earcon set, hidden easter eggs, one
   permanent persona; zero snark in confirmations; never initiates.

Explicitly not worth building: a chat webview, launcher/clipboard/window
management (rofi and i3 own them), avatar bodies, continuous screen
recording, sarcastic personality content, assistant-initiated speech outside
the briefing budget, wake word before the hotkey path saturates.

## Design review decisions (2026-08-25, on the design page)

Review of the published design page (Scufris HUD artifact) settled:

- The pill gets square corners, not a rounded capsule, to match the rest of
  the desktop. Shadow, layered gradient plus faint scanline background, and
  state-colored corner ticks in the HUD panel language.
- Listening shows no text. The wave, glow, and timer carry the state; words
  exist only once transcription streams. Partials appear at transcribing.
  Note: the current flow posts the full take to whisper-server after stop,
  so partials mean chunked decode progress; true mid-speech streaming needs
  a streaming STT endpoint.
- Mockups use a top i3 bar (the real layout). Voice feedback bottom edge,
  status top edge.
- The session HUD (v2) was first parked, then promoted on second review:
  the HUD widget becomes the primary way to read the session - super+s or
  a pill click spawns it - and kitty + pi demotes to the debug view, the
  raw terminal for seeing exactly what the agent did. Transition
  gradually: the Kitty popup stays authoritative until the mirror earns
  trust.
- The v3 widget surface is not one overlay. Two kinds of floating windows
  from the same dashboardd runtime: exhibits, spawned by Scufris as visual
  aids while he speaks ("cpu is at 90%" plus the graph beside the pill) and
  fading with the topic; and instruments (calendar, tasks), summoned by the
  user, interactive, alive until closed. Timers sit in both camps: born
  from a voice verb, persistent while running.
- Escalation and widgets unify: the popup/session surface is itself a
  widget, so escalation is not a special mechanism - it is spawning
  surfaces with increasing prominence through one spawn interface: a line
  in the pill, then exhibits beside it, then the session surface raised.
  Open question to settle before the dashboardd embed spec: are exhibits
  ephemeral only, or can the user pin one into an instrument?
- Yellow = listening is confirmed. The Alexa yellow-means-notification
  convention carries no weight for the owner; gruber yellow keeps the
  identity on the mic moment, attention stays wisteria, red stays reserved.
- Earcons ship enabled from the start: the four boundary cues (mic open,
  mic close, attention, error), near-subliminal, with a mute switch.
- The i3 mode binds bare Escape and Enter. While the pill is open nothing
  else is being touched, so bare keys win; $mod variants stay the
  documented fallback if they ever bite.
- Pill personality is the dynamic glow. The state-colored glow rides the
  mic level while listening and breathes slowly at idle. No blink or
  oneko-style flourishes; prefers-reduced-motion disables the breathing.
- Exhibit lifecycle: exhibits age on topic relevance, not on the pill;
  closing the pill changes nothing on screen. A topic change dims the
  exhibit to ~40%, a ~60s grace window follows, then a quick exit is
  fine - dimming already gave the feedback. The invariant is that
  nothing disappears straight from LIVE, not that exits are slow.
  Being cited again or hovered revives it. Every clock
  freezes while the mic is hot, Scufris is speaking, or the pointer is
  over the exhibit, so its numbers can be read aloud mid-dictation. The
  only instant exits are explicit: the close tick or a "clear" verb.
  Pinning promotes an exhibit into an instrument (stops aging,
  user-owned). This settles the ephemeral-vs-pinnable question:
  ephemeral by default, pinnable by promotion.
- Pill and popup are two depths of one session, not two modes. Invariant:
  the pill is ephemeral and never holds state you would miss. Proposed rule:
  the pill escalates to the popup - opening it with context carried over -
  on long output, follow-ups, or attention. The refused/uncertain
  self-reopen edge already shipped in v1. This escalation rule is the
  concrete answer to "why keep both".

## Suggested implementation order

1. Logging (section 2): small, unblocks live playtesting of everything else
   with real observability.
2. Pill visual redesign (section 1): the stated pain, all technique
   de-risked, and the state grammar is a contract the later features reuse.
3. i3 mode focus-free keys (section 3): changes daily feel immediately.
4. Backlog items 1-3 (dictation mode, fast verbs + timers, context
   capture): each small-to-medium, each independently shippable.
5. Wake word (section 4) and the HUD embed (backlog 4) as their own tasks.
