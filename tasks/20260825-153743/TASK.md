# Redesign the pill: square gruber HUD, state grammar, dynamic glow, earcons

- STATUS: IN_PROGRESS
- PRIORITY: 85
- TAGS: voice, desktop, design

## Goal

Replace the gray box with the reviewed HUD pill: square gruber-darker
panel, diegetic state grammar, audio-reactive motion, and the four
boundary earcons. The approved look is the design page recorded in
`tasks/20260822-132001/RESEARCH.md` (Design review decisions).

## Scope

- Square corners, layered gradient plus faint scanline background,
  shadow, and state-colored corner ticks. No rounded capsule.
- Technique: CSS plus small Canvas 2D only. No WebGL (WebKitGTK software
  fallback, NVIDIA transparency bugs). Spring the displayed amplitude
  toward the 60 ms tick level: `display += (target - display) * 0.25`.
  Run rAF only in audio-reactive states.
- State grammar (gruber palette): yellow listening, brown transcribing,
  quartz review, niagara working (shimmer), green speaking, wisteria
  attention, red reserved for error and mic. Speed = intensity,
  direction = who talks. Every state must survive grayscale.
- Listening shows no text. Words appear only once transcription streams;
  partials appear at transcribing.
- Personality is the dynamic glow: it rides the mic level while
  listening and breathes slowly at idle. No blink or oneko flourishes.
- Earcons ship enabled: mic open (rising), mic close (falling),
  attention (one chime), error (one low tone). Near-subliminal, with a
  mute switch. Nothing for working or speaking.
- Honor prefers-reduced-motion: crossfade instead of motion, breathing
  off.
- Keep the elapsed recording timer. Recording surface stays inert to
  clicks.

## Verification

- Each state is distinguishable in grayscale and with reduced motion.
- The pill reacts within one frame of Super+D.
- Earcons fire only at the four boundaries and the mute switch works.
- Live playtest against the design page side by side.

References (MIT): kopiro/siriwave, SmoothUI Siri Orb, VoiceOrbs Edge
Glow. Full synthesis: `tasks/20260822-132001/RESEARCH.md` section 1.
