# Research track: design guidance and orb implementation techniques

Agent report, 2026-08-25. Raw findings; synthesis lives in ../RESEARCH.md.

Context checked locally first: the pill window is `transparent(true)`, `always_on_top(true)`, undecorated (`desktop/scufris-desktop/src/pill.rs`). Rust already captures the mic with cpal and emits `scufris://tick` every 60 ms with a peak-normalized `level` (`src/app.rs`: `TICK_INTERVAL = 60ms`, `TickPayload.level`; `src/audio.rs`: `take_level()`). `ui/pill.js` already maps it to a `--level` CSS custom property. This shapes everything below: the audio plumbing exists; only the visual layer and state language need work.

## Track A: design guidance

### Apple HIG

- Siri HIG: minimize friction, respond fast, design voice-first ("people don't always look at the screen"). https://developers.apple.com/design/human-interface-guidelines/technologies/siri/introduction
- Motion HIG: add motion purposefully; motion communicates feedback and state change; never make motion the only carrier of important information; when Reduce Motion is on, minimize or eliminate animation (crossfade instead of movement). https://developers.apple.com/design/human-interface-guidelines/foundations/motion
- Accessibility HIG covers the same reduce-motion and color-independence points. https://developers.apple.com/design/human-interface-guidelines/foundations/accessibility
- Modern Siri visual language (iOS 18 / Apple Intelligence): the orb became a glowing multicolor edge light that pulses while listening. The relevant idea for a pill: state is carried by a glow that breathes with input, not by text. https://www.slashgear.com/1865686/iphone-glowing-around-edges-reason/

### Amazon Alexa attention states

The most concrete published spec of assistant states anywhere:

- Voice Interoperability baseline: all agents must convey Listening, Thinking, Speaking; states must be visually and sonically distinguishable; conveying mic on/off is "very important". https://developer.amazon.com/en-US/alexa/voice-interoperability/design-guide/baseline-guidance
- Light ring identity guidance: Listening = directional blue pointing at the speaker, pulse width tracks voice loudness; Thinking = alternating/cycling blue immediately after speech ends; Speaking = blue pulse tracking Alexa's output loudness; Notification = pulsing yellow; Error = quickly pulsing purple; DND = slowly pulsing purple; Mic off = solid red. Key patterns: input states are audio-reactive, processing states are self-animating loops, and the same color family (blue) spans the whole primary interaction, differentiated by motion pattern. https://developer.amazon.com/en-US/alexa/branding/echo-guidelines/identity-guidelines/light-ring
- Earcons: Alexa's optional "Start of Request" (wake) and required-when-enabled "End of Request" (endpointing) sounds exist so users know the mic state without looking. https://developer.amazon.com/en-US/docs/alexa/alexa-auto/invoking-alexa.html

### Google conversation design

- Errors: three types (No Match, No Input, System Error); assume the user is cooperative; be transparent about why something failed; escalate detail across retries (rapid reprompt first, examples second, graceful exit after two failures). https://developers.google.com/assistant/conversation-design/errors
- Earcons: use very few, use them consistently, and "if you feel like you have to teach users what an earcon means, don't use an earcon". Earcons add cognitive load. https://developers.google.com/assistant/conversation-design/earcons

### Voice UX literature

- Calm technology (Amber Case, from Weiser/PARC): technology should demand the smallest possible amount of attention and move between periphery and center; communicate through ambient light, tone, and motion rather than words. This is the theoretical basis for a diegetic pill. https://calmtech.com/ and https://www.caseorganic.com/post/principles-of-calm-technology
- Earcons generally: short, distinct, consistent; a subtle ping for start listening, distinct sound for processing, chime for completion, discordant tone for failure; keep the vocabulary tiny. https://medium.com/vui-magazine/earcons-the-audio-version-of-an-icon-59b7f0921235
- Latency masking: 300 ms is the conversational gap humans expect; delays are consciously noticed past ~500 ms; UX collapses around 4 s. Streaming partial results (live transcript text appearing as you speak) and immediate acknowledgment motion are the standard masks; backchanneling cuts perceived latency by 300-500 ms. Practical takeaway: the orb must react within one frame of state change, and transcription should render partials, not wait for finals. https://www.assemblyai.com/blog/low-latency-voice-ai and https://telnyx.com/resources/low-latency-voice-ai
- NN/g: without visual signifiers, activation tones and clear state feedback are critical; error states must never leave the user stuck. https://www.nngroup.com/reports/

### Accessibility

- `prefers-reduced-motion`: honor it; replace movement with opacity/color crossfades. WebKitGTK maps it from the GTK `gtk-enable-animations` setting, so it works in the pill webview. https://developer.mozilla.org/docs/Web/CSS/Reference/At-rules/@media/prefers-reduced-motion
- WCAG 2.3.3 (Animation from Interactions): reduce or replace non-essential motion; opacity fades and color transitions are safe alternatives. https://web.dev/learn/accessibility/motion
- Color-independence (WCAG 1.4.1 Use of Color): every state must differ by motion pattern or shape, not hue alone, for color-blind users. Alexa's ring already models this: listening is directional, thinking is cycling, speaking is bottom-pulsing, all in blue. https://www.w3.org/WAI/WCAG21/Understanding/use-of-color.html

## Track B: implementation techniques

### Libraries and recreations

- kopiro/siriwave: the canonical JS Siri waveform. Canvas 2D, MIT, zero dependencies, npm `siriwave`. Styles: `ios` (classic overlapping sines) and `ios9` (fluorescent multi-color wave); `setAmplitude()`/`setSpeed()`/`start()`/`stop()` are exactly the API needed to drive it from the Rust tick. Math writeup by the author: https://www.freecodecamp.org/news/how-i-built-siriwavejs-library-maths-and-code-behind-6971497ae5c1/ . Repo: https://github.com/kopiro/siriwave
- aaaa-zhen/siri-glsl: MIT, raw WebGL fragment shaders (single fullscreen triangle, zero deps, self-contained HTML files) recreating the modern Siri wave and its wave-to-fluid-dots morph, plus liquid glass. Best visual reference for the current Apple look. https://github.com/aaaa-zhen/siri-glsl (demo https://aaaa-zhen.github.io/siri-glsl/siri-wave.html)
- SmoothUI Siri Orb: six layered conic gradients animated via a registered `--angle` property; has exactly the state set needed (idle, listening, thinking, streaming, done, error), accepts an amplitude input, and honors reduced motion. React + Motion wrapper, but the technique is plain CSS and trivially portable to vanilla JS. SmoothUI is MIT. https://smoothui.dev/docs/components/siri-orb and https://github.com/educlopez/smoothui
- CodePen "Siri 2.0 iOS 18" pens: conic-gradient + `@property --angle` rotating glow, pure CSS. https://codepen.io/firepanther/pen/WNBZaEd . More CSS blob patterns (border-radius keyframe morphing + `filter: blur()`): https://freefrontend.com/css-blob-effects/
- Jxl-s/webgl-blob: audio-reactive sphere blob (Vite, mic input) but no license file; do not copy code from it. https://github.com/Jxl-s/webgl-blob
- Shadertoy "Siri-Inspired Audio Visualiser": good math reference, but Shadertoy's default license is CC BY-NC-SA; treat as look-reference only, do not port code. https://www.shadertoy.com/view/llySRm

### CSS-only technique summary

Layer 2-6 conic/radial gradients in one element, rotate via a registered `@property` angle (supported in WebKitGTK since the Safari 16.4-era feature sync; current distro WebKitGTK 2.46+ is fine), morph `border-radius` between keyframes for blob wobble, and apply a modest `filter: blur()`. Animate only `transform`, `opacity`, and the registered angle; do not animate blur radius continuously (repaints every frame).

### Audio input: AnalyserNode vs Rust-side levels

Rust-side wins decisively here:

- `getUserMedia`/WebRTC in WebKitGTK under wry/Tauri is unreliable on Linux: permission requests are auto-denied without custom handling, and WebRTC often requires a custom-built WebKitGTK. https://github.com/tauri-apps/wry/issues/85 , https://github.com/tauri-apps/tauri/issues/8851 , https://github.com/tauri-apps/tauri/discussions/8426
- Two processes opening the same mic (cpal for STT plus the webview) is a needless PipeWire complication.
- The 60 ms peak-level tick already exists. 16 Hz is too coarse to drive a 60 fps wave directly, so lerp/spring toward the latest level inside `requestAnimationFrame`; that is exactly how Echo's "pulse matches your voice" reads despite coarse sampling. If richer motion is wanted later, extend `TickPayload` with 3-4 band energies (bass/mid/treble RMS) computed in Rust; a full FFT-to-JS stream is unnecessary.

### WebKitGTK performance constraints

- Tauri's own Linux graphics page: WebKitGTK + NVIDIA commonly needs `WEBKIT_DISABLE_DMABUF_RENDERER=1` or worse `WEBKIT_DISABLE_COMPOSITING_MODE=1`; critically, WebGL context creation can silently succeed on a software rasterizer, and the renderer string is masked, so you cannot detect it from JS. Official advice: ship a non-WebGL fallback on Linux. https://v2.tauri.app/develop/debug/linux-graphics/
- Transparent windows have known NVIDIA crash/artifact issues (GBM Error 71, ghosting). https://github.com/tauri-apps/tauri/issues/14924
- WebKitGTK 2.46 moved 2D rendering to Skia with GPU-accelerated canvas by default (`enable-2d-canvas-acceleration` setting exists); 2.48 improved this further and pauses rendering for suspended windows. Canvas 2D is therefore in decent shape on modern distros. https://blogs.igalia.com/carlosgc/2024/09/27/graphics-improvements-in-webkitgtk-and-wpewebkit-2-46/ and https://webkitgtk.org/2025/04/08/webkitgtk-2.48.html
- The window is ~480x76 px (~36k pixels), so per-frame compositing cost is tiny even on the CPU path. The real battery rule is duty cycle: run rAF only in audio-reactive states, use pure CSS animation (compositor-driven) for ambient states, and render nothing when idle.

Conclusion for this environment: CSS + small Canvas 2D is the safe path; WebGL is a liability (silent software fallback, NVIDIA transparency bugs) for a product that must run on arbitrary Linux boxes.

## Recommended visual architecture

Hybrid, three layers inside the existing pill, all state driven by CSS classes on the pill root plus the existing `--level` variable:

1. Orb core (always present): one ~56 px DOM element, SmoothUI-style layered conic/radial gradients rotated via a registered `--angle` property, plus a soft radial `box-shadow`/glow. State classes swap the palette (CSS custom properties) and the animation-duration/pattern. Cost: near zero; compositor handles it.
2. Audio-reactive layer (listening and speaking only): a small Canvas 2D strip next to or behind the orb running siriwave `ios9` style (or a radial ring variant clipped to the orb). `start()` on state entry, `stop()` and clear on exit. Each rAF frame, spring the displayed amplitude toward the latest Rust tick level: `display += (target - display) * 0.25`. In speaking state, feed TTS output level through the same tick channel instead of mic level.
3. Transcript/review layer: the editable transcript text. During transcribing, render streaming partials immediately (this is the latency mask; the orb alone must never be the only progress signal for more than ~1 s).

No WebGL. Honor `prefers-reduced-motion`: disable rotation and waveform, keep a static orb whose glow opacity tracks `--level` and whose color crossfades on state change. Every state remains distinguishable with animations off (color + glow intensity + transcript area) and in grayscale (motion pattern + duty cycle).

## State-to-motion/color/sound mapping

Follows the Alexa grammar: input states react to sound, processing states self-animate, alerts pulse, faults are red/dim. One color family per phase; motion pattern is the color-independent differentiator. Sounds only at boundaries the user must not miss (mic open, mic close, attention, error); never a sound for working or speaking.

| State        | Motion                                                                                                                | Color                             | Sound                                                       |
| ------------ | --------------------------------------------------------------------------------------------------------------------- | --------------------------------- | ----------------------------------------------------------- |
| listening    | Orb breathes; waveform amplitude tracks mic `--level` (your voice visibly moves it)                                   | Cool cyan/blue                    | Short rising two-note earcon on mic open                    |
| transcribing | Waveform collapses into orb; gradient shimmer/slow spin; partial text streams in                                      | Blue shifting toward violet       | Short falling earcon on mic close (endpointing)             |
| review       | Orb settles almost still, faint slow breathing; text caret is the live element                                        | Neutral white/soft blue, low glow | None                                                        |
| working      | Continuous slow gradient rotation with gentle hue drift (clearly self-animating, not audio-reactive)                  | Violet/magenta                    | None                                                        |
| speaking     | Orb pulse tracks TTS output level (same reactive grammar as listening, different color)                               | Teal/green                        | None (the voice is the sound)                               |
| attention    | Two soft scale-and-brighten pulses, then loop a slow pulse until acknowledged                                         | Amber/yellow                      | One gentle chime, not repeated                              |
| error        | Single quick desaturate-flash, then 2-3 fast shallow pulses, settle dim; short plain-language line in transcript area | Red/orange                        | One low discordant tone                                     |
| disconnected | Orb dims to gray, very slow shallow breathing (alive but inert)                                                       | Desaturated gray                  | One low tone on transition only, silence while disconnected |

Reduced motion variant: replace all of the above motion with opacity/color crossfades; keep glow intensity tracking `--level` in listening/speaking (brightness change, not movement).

## Top 3 references to copy from

1. kopiro/siriwave (MIT) - https://github.com/kopiro/siriwave - proven Canvas 2D iOS/iOS9 waveform with `setAmplitude()`; drop-in for the listening/speaking reactive layer; math explained in the author's freeCodeCamp article.
2. SmoothUI Siri Orb (MIT) - https://smoothui.dev/docs/components/siri-orb / https://github.com/educlopez/smoothui - layered conic-gradient orb with the exact state vocabulary needed (idle/listening/thinking/streaming/done/error), amplitude input, and reduced-motion handling; port the CSS out of React into `pill.css`.
3. aaaa-zhen/siri-glsl (MIT) - https://github.com/aaaa-zhen/siri-glsl - dependency-free fragment-shader recreations of the modern Siri wave and wave-to-dots morph; use as the visual/timing reference for how listening should collapse into transcribing, even though the production build should stay CSS/canvas rather than WebGL.
