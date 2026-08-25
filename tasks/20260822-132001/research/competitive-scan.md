# Research track: voice UI market survey

Agent report, 2026-08-25. Raw findings; synthesis lives in ../RESEARCH.md.

Scope: recording/listening indicators, state signaling, placement, and steal-worthy ideas for a small bottom-center pill on Linux. Sources at the end of each section.

## 1. Apple Siri (modern iOS/macOS)

a. Indicator. Two generations. Classic: a swirling multicolor orb at the bottom center whose internal waves ripple with your voice. Apple Intelligence era (iOS 18+): the orb is gone; a rainbow gradient glow wraps the entire screen edge. The glow pulses with the rhythm of your speech, acting as a pseudo-waveform. On invoke there is a "ripple" wash over the screen. On macOS Sequoia, Siri is a small movable floating window (top-right by default) with the orb icon; Type to Siri opens a compact text field.

b. States. Invoke: edge ripple in. Listening: edge glow pulses with voice. Thinking: the glow animates on its own, slower. The transcription appears live in a bubble at the top. On devices without Apple Intelligence the old orb persists, so Apple itself uses the visual as a brand tier signal. Rumor track for iOS 27: Siri renders as a pill that covers the Dynamic Island, i.e. Apple is converging on exactly the "small pill" form factor.

c. Placement. Full-screen edge treatment on iOS; small floating window on macOS; bottom-center orb historically.

d. Steal. Two things. First, voice-reactive glow on the border of a shape rather than a meter inside it; it reads as alive at very small sizes. Second, the non-modal principle: the indicator never blocks the screen, you keep working while it listens.

e. Sources: https://www.slashgear.com/1865686/iphone-glowing-around-edges-reason/ , https://www.pocket-lint.com/how-to-get-new-siri-look-glowing-border/ , https://mcmw.abilitynet.org.uk/how-to-use-siri-the-digital-assistant-in-macos-15-sequoia , https://www.macrumors.com/2026/06/16/iphone-18-could-make-siri-a-circle/

## 2. ChatGPT voice mode

a. Indicator. Advanced voice: a soft blue-white orb (blurred gradient sphere, cloud-like interior) centered on a plain background. It swells and ripples while you speak, pulses gently while listening idle, and undulates in sync with its own speech. Standard/legacy voice was a plain black circle. The orb's motion is slow and fluid; nothing is literal (no bars, no mic icon).

b. States. Distinguished almost entirely by motion quality, not color: small breathing = idle listening, reactive swelling = hearing you, self-driven undulation = speaking, gentle pulsing/shimmer = thinking. In Nov 2025 OpenAI removed the separate full-screen orb view; voice now runs inside the chat with a smaller indicator, with live transcript and visuals in the same screen.

c. Placement. Was full-screen centered; now integrated in the chat UI.

d. Steal. The motion-only state language: one shape, different animation regimes per state. It proves you do not need labels if the movement quality is distinct. Also the lesson from the redesign: people want to see the text while speaking, so the pill should coexist with a transcript, not replace it.

e. Sources: https://learnprompting.org/blog/how-to-use-openai-chatgpt-advanced-voice-mode , https://techcrunch.com/2025/11/25/chatgpts-voice-mode-is-no-longer-a-separate-interface/ , https://www.ghacks.net/2025/11/26/chatgpt-now-lets-you-use-voice-mode-directly-in-chat/

## 3. Windows Copilot (Mico) and Cortana

a. Indicator. Copilot voice mode now has "Mico": a small animated blob/orb with a minimal face. It changes color with conversational tone, changes shape and size to show listening vs responding, and syncs simple mouth/eye expressions to speech. "Hey Copilot" activation shows a mic indicator plus a chime. Cortana (Windows 10) was a flat blue pulsating circle/halo near the Start menu with 18 scripted emotion animations (happy, embarrassed, thinking, speaking), designed with 343 Industries so users would forgive errors more readily.

b. States. Mico: color + shape + micro-expression. Cortana: variations of the circle animation; thinking vs speaking had inverted inner/outer halo treatments.

c. Placement. Mico lives in the Copilot app pane; Cortana sat bottom-left by the taskbar, small.

d. Steal. The idea that error/attention states get an "emotional" animation (a wobble, a dim, an apologetic shrink) rather than just red. Cortana's inverted-halo trick (swap which ring is bright) is a cheap, legible way to separate speaking from listening on a tiny element. Full face/avatar is out of scope and risks kitsch.

e. Sources: https://windowsforum.com/threads/mico-microsoft-copilots-animated-avatar-for-voice-mode.386367/ , https://blogs.windows.com/windowsexperience/2015/02/10/how-cortana-comes-to-life-in-windows-10/ , https://www.pcworld.com/article/431767/cortanas-ui-now-expresses-18-different-emotions-siri-remains-detached-and-aloof.html , https://learn.microsoft.com/en-us/windows/apps/design/input/cortana-design-guidelines

## 4. Google Gemini / Assistant

a. Indicator. Gemini's design system uses soft gradient shapes in the four brand colors. Listening is shown with rippling radial gradients ("voice waves"). Voice input in Gboard/Gemini replaces the text field with a full-width waveform. Classic Assistant used four bouncing colored dots that morph into a waveform when you speak.

b. States. Thinking: concentrated-then-diffused gradients, a sharp opaque leading edge blooming into a blurred tail. Working: pulsing, expanding gradient bars. Google's stated principle: "movement is not decorative, it is an essential guiding element"; inner motion conveys thinking and makes processing feel transparent.

c. Placement. Bottom sheet / keyboard area on Android; the waveform is bottom-anchored and wide, which matches a bottom-center pill well.

d. Steal. The gradient shimmer for "thinking": animate a gradient's focus point traveling through the pill (sharp head, diffuse tail). It is cheap in CSS (animated background-position or mask) and reads clearly as "working, not stuck." Also the idea that transcription state = diffuse gradient over the text area.

e. Sources: https://design.google/library/gemini-ai-visual-design , https://9to5google.com/2026/03/19/gemini-voice-input-redesign/ , https://60fps.design/shots/google-gemini-response-gradient-animation

## 5. Amazon Alexa (Echo light ring)

a. Indicator. An LED ring. Listening: solid blue ring with a cyan spotlight segment that points toward the speaker (direction-of-voice cue). Thinking: blue/cyan spinning. Speaking: cyan pulsing.

b. States. Pure color + pattern grammar, no text: red solid = mic muted; yellow slow burst = notification/message waiting; green pulse = incoming call, green spin = active call; orange spin = setup/connectivity; purple flash = do-not-disturb; white arc = volume level. This grammar is documented and users learn it.

c. Placement. Hardware ring, but the grammar transfers to any ring/border.

d. Steal. The complete state-color contract, especially: red is reserved exclusively for "mic off/error", yellow pulse = "I have something for you" (maps to the attention state), and an arc-length display for scalar values (volume, progress). Also the cyan "spotlight" segment: a brighter sector on a dim ring is a strong minimal listening cue.

e. Sources: https://www.amazon.com/gp/help/customer/display.html?nodeId=GKLDRFT7FP4FZE56 , https://www.pcworld.com/article/578763/what-flashing-lights-on-amazon-echo-speaker-mean.html , https://www.howtogeek.com/what-do-alexas-light-colors-mean/

## 6. Dictation tools

### Wispr Flow

a/b. The "Flow Bar": a slim, minimal dark floating pill with a live waveform and a pulsing recording dot while listening, then a near-instant transition to finished text. The waveform area is deliberately not clickable during dictation so you cannot abort by accident.
c. Bottom of screen by default; can dock to left/right edges, where the bar reorients vertically and the waveform reflows.
d. Steal: restraint (dark, quiet, no chrome), the non-interactive recording zone, and the edge-docking reflow.
e. https://docs.wisprflow.ai/articles/6409258247-starting-your-first-dictation , https://docs.wisprflow.ai/articles/5002934560-why-is-the-wispr-bar-is-not-appearing-or-disappearing , https://abduzeedo.com/wispr-flow-voice-dictation-app

### Superwhisper

a/b. Floating recording window with a live waveform plus a color-coded status dot: yellow = model loading, blue = processing, green = done. Shows the active mode and its shortcut, plus a "context captured" light when it grabbed clipboard/selection. A compact mini window shows a small idle indicator and reveals controls on hover.
c. Small floating window; mini mode can stay resident when idle.
d. Steal: the tiny status dot as a secondary channel (pill shape carries the main state, a 6px dot carries substate), and the "context captured" confirmation light, which is a diegetic trust cue. Hover-to-reveal controls keeps idle chrome at zero.
e. https://superwhisper.com/docs/get-started/interface-rec-window

### VoiceInk

a/b. Open-source macOS dictation with a mini recorder and a notch-style recorder, and a menu of selectable recorder animation styles: Gold Pulse, Waveform, Ripple, Neon Ring, Morph, Vortex, Quantum, Time Travel, Ink Pen. States: recording, transcribing, AI enhancement.
d. Steal: shipping the visualizer as a user-selectable theme. The pill renderer can be a pluggable "style" (ring, waveform, blob) over one shared state machine.
e. https://tryvoiceink.com/ , https://mac.softpedia.com/get/Generative-AI-Tools/VoiceInk.shtml

### Aqua Voice

a/b. Hold-to-talk pill focused on latency: starts in under 50ms, inserts text in about a second. UI is minimal; the marketing emphasis is speed over spectacle. No detailed public docs on the indicator visuals.
d. Steal: the latency bar. An indicator that appears within one frame of the hotkey is itself the strongest feedback; animation must never delay perceived start.
e. https://aquavoice.com/ , https://www.producthunt.com/products/aqua

### MacWhisper and the mac-whisper-clone ecosystem

a/b. MacWhisper dictation is hotkey plus a small overlay. The surrounding open-source ecosystem converged on the same pattern: menu bar waveform icon that turns red when recording (local-whisper); a floating overlay cycling recording -> transcribing -> fixing -> done (WhisperApp); a floating indicator with an animated "Siri-style border" while recording (look-ma-no-hands).
d. Steal: the explicit four-step state ladder (recording, transcribing, fixing, done) shown in one small overlay; it matches the listening/transcribing/review/working ladder almost exactly.
e. https://docs.macwhisper.com/article/14-how-to-use-the-dictation-feature , https://github.com/luisalima/local-whisper , https://github.com/Gamezxz/WhisperApp , https://github.com/qaid/look-ma-no-hands

### Apple Dictation

a/b/c. A small feedback popover near the cursor or bottom of screen: blue mic icon with fluctuating loudness bars, plus a ready tone. With multiple languages the popover also shows the current language as a clickable chip.
d. Steal: the ready tone paired with the visual (audio + visual redundancy for eyes-elsewhere use), and surfacing one bit of session metadata (language, or here the active mode) on the indicator itself.
e. https://support.apple.com/guide/mac-help/use-dictation-mh40584/11.0/mac/11.0 , https://macmost.com/how-to-use-dictation-on-your-mac-2.html

## 7. Launcher-style tools

### Raycast

a/b. The "Dictation Pill": floats above the active app with a live waveform and a session timer. Toggle or hold-to-talk hotkey; release ends the session and processing begins; text pastes into the focused app.
c. A pill, floating, small, above whatever you are doing.
d. Steal: the timer next to the waveform (cheap, reassuring, tells you the take is still rolling), and the accept-on-release interaction. Raycast is the closest commercial analog to scufris-desktop's form factor.
e. https://manual.raycast.com/ai/dictation , https://www.raycast.com/changelog/macos-beta/0-57

### Alfred and Ulauncher

Neither has a native voice UI; Alfred users fall back to macOS dictation inside the search field, Ulauncher has no voice story at all. The takeaway is negative but useful: keyboard-launcher chrome (input field + list) is the wrong metaphor for voice; the pill/orb overlay is the right one. Sources: https://www.raycast.com/finjo/whisper-dictation , https://startupik.com/ulauncher-vs-alfred-which-tool-is-better/

## 8. Open-source / Linux

### Home Assistant Voice PE (and Assist satellites)

a/b. The LED ring grammar, from the shipped ESPHome config: wake word acknowledged = slow clockwise spin with trailing decay; actively listening = the same spin but fast; thinking = two opposing LEDs pulsing in place; replying = fast counter-clockwise spin (direction reversal marks who is talking); error = rapid red pulse; muted = two fixed red LEDs at ring positions 3 and 9 over the base color; timer = arc whose length is remaining time; volume = arc whose length is level. Accent color is user-configurable; red is hardcoded for error/mute.
d. Steal: this is the best documented open state grammar in the survey. Speed encodes intensity (waiting vs listening), direction encodes speaker (you vs it), position encodes persistent flags (mute pips), arc length encodes scalars. All of it maps directly onto a conic-gradient ring or the pill's border.
e. https://github.com/esphome/home-assistant-voice-pe/blob/dev/home-assistant-voice.yaml , https://esphome.io/components/voice_assistant/ , https://www.home-assistant.io/blog/2024/12/19/voice-chapter-8-assist-in-the-home/

### Mycroft / OVOS

a/b. Mark II GUI cycles full screens per utterance: listening (vertical bar animation) -> thinking -> speaking -> idle. Known pain point: latency between wake word and the animation starting undermined trust, and the team explicitly worked to simplify the screen states.
d. Steal: the warning. Long state ladders shown as discrete screen swaps feel laggy; continuous morphing of one element between states feels alive.
e. https://github.com/MycroftAI/skill-mark-2/issues/49 , https://mycroft-ai.gitbook.io/docs/skill-development/displaying-information/mycroft-gui

### whisper-overlay (oddlama)

a/b. Wayland layer-shell overlay, push-to-talk. Shows realtime partial transcription on screen as you speak, then replaces it with the high-fidelity pass on release, then types it into the focused window. GTK, themeable via user style.css. Companion waybar module: gray = disconnected, blue = connected idle, red = recording/transcribing.
d. Steal: live partial text as the primary feedback (the words themselves are the best "it hears me" signal), plus a three-color connection state for the disconnected state. Also validates layer-shell overlay + hotkey as the native Linux pattern.
e. https://github.com/oddlama/whisper-overlay

### hyprwhspr / Voxtype (Omarchy, Hyprland ecosystem)

a/b. hyprwhspr: animated microphone OSD overlay on layer-shell compositors (Hyprland, Sway, niri, KDE) that auto-matches the live shell theme on Omarchy/Noctalia; beep on start, boop on stop; waybar status module. Voxtype: Rust, hold-hotkey, waybar text states for recording/processing.
d. Steal: theme-matching (read the user's accent color) and the start/stop earcons. Waybar-only feedback is the ceiling of the current Linux ecosystem, which is exactly the opening scufris-desktop can exploit.
e. https://github.com/goodroot/hyprwhspr , https://github.com/basecamp/omarchy/discussions/1258 , https://paolino.me/dictation-is-the-new-prompt/

### Speech Note (dsnote)

Qt Flatpak app, note-window-centric with global shortcuts; no ambient overlay indicator. Confirms the gap. https://github.com/mkiol/dsnote

### Ready-made web implementations (directly reusable technique)

VoiceOrbs: 14 orb styles, all with the same state set (idle, connecting, listening, thinking, speaking, error, disabled); notable ones for a webview pill: Halo (pure CSS conic halo with pulsing core), Gooey (SVG turbulence/displacement liquid blob), Waveform Ring (canvas polar waveform), Edge Glow (Siri-style masked conic-gradient frame that wraps your own content), Plasma (canvas shader, no Three.js). Also: voiceorb (Three.js + GLSL, Perlin displacement, Fresnel rim, 4 states) and a CSS-only morphing border-radius blob. https://voiceorbs.vercel.app/ , https://github.com/aguscruiz/voiceorb , https://codeshack.io/morphing-voice-assistant-orb-css/ , https://medium.com/@therealmilesjackson/building-a-voice-reactive-orb-in-react-audio-visualization-for-voice-assistants-2bee12797b93

## Ranked shortlist: 5 ideas to copy

1. Siri-style edge glow on the pill border, driven by mic level. Replace the scaling circle with a conic/linear gradient glow that runs around the pill's rounded border and pulses with voice energy (VoiceOrbs "Edge Glow" is a working CSS recipe: masked conic gradient + blur). It reads as alive at 300x48px, leaves the pill interior free for text, and costs one animated CSS custom property fed from the existing mic-level signal.

2. The Voice PE motion grammar for states. One accent-colored glow, where animation encodes state: slow drift = idle/wake, fast clockwise sweep = listening, two stationary pulsing nodes = transcribing/working, reverse-direction sweep = speaking, rapid red pulse = error, fixed red pips = mic muted, gray/desaturated = disconnected, yellow slow burst = attention (Alexa's notification cue). This is a proven, documented grammar and maps one-to-one onto animating the conic gradient's angle, speed, direction, and hue on the pill border.

3. Gemini-style gradient shimmer for thinking/working/review. While transcribing or while Pi is working, run a sharp-headed, diffuse-tailed gradient highlight traveling through the pill (animated background-position or mask on the pill fill). It visually separates "machine is busy" from "machine is hearing you" without changing the pill's silhouette, and it is pure CSS.

4. Raycast/Wispr pill internals: live waveform (or partial transcript) + timer, non-clickable while recording. Inside the pill, show a small live canvas waveform and an elapsed timer during listening, swapping to live partial text like whisper-overlay when transcription streams. Make the recording surface inert to clicks so a stray click cannot kill a take; hover reveals cancel/accept like Superwhisper's mini window.

5. ChatGPT-style single morphing blob for the "speaking" and idle personality, with instant appearance. Keep one continuous element that morphs between states (CSS border-radius morph or SVG gooey filter for the accent blob inside the pill) instead of Mycroft-style discrete screen swaps, and make the pill react within one frame of the hotkey (Aqua's lesson: latency is the first animation). Self-driven undulation while speaking vs mic-driven swelling while listening gives you a who-is-talking cue with zero extra pixels.

Cross-cutting notes: reserve red exclusively for muted/error (Alexa, Voice PE both do); pair every state change with a short earcon (hyprwhspr, Apple Dictation) since a bottom pill sits in peripheral vision; read the user's shell accent color like hyprwhspr does on Omarchy; and treat the current Linux field (waybar icons and bare text overlays) as the bar to clear -- no Linux tool today has an audio-reactive pill, so idea 1 alone is differentiating.
