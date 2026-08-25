# Pill orb on the thinking-orbs engine

- STATUS: OPEN
- PRIORITY: 75
- TAGS: desktop, ux

## Goal

Replace the pill's conic-gradient orb disc with the dotted thought-orb
language of `thinking-orbs` (Jakub Antalik, MIT), repainted in gruber
ink. Feasibility proven live in the Orb Study page (2026-08-25):
the real engine at the pill's 30px is ~40 arc fills per frame on a
plain 2D canvas - no WebGL, no CSS filters, exactly what WebKitGTK
without a compositor wants.

## Decisions (Alex, 2026-08-25)

- Use the library, do not recreate it. MIT; the engine runs unmodified.
- No npm in `ui/`. Vendor `dist/engine.es.js` from
  `thinking-orbs@0.3.1` (19KB, 557 lines) with its MIT header and a
  one-line provenance comment. `build.rs` keeps building with plain
  tsc; revisit only when the ui genuinely wants a second or third
  library.
- The same orb later serves as the widget shell's busy indicator
  (recorded in `tasks/20260825-215520`).

## Scope

- Vendor the engine into `ui/` plus a hand-written `.d.ts` for the
  pieces `pill.ts` uses (`MODE_FRAMES`, `resolvePreset`, frame types).
- A small gruber painter: ink 0..1 maps to a panel-to-accent mix,
  alpha preserved, lines before dots. Accents come from the existing
  per-state `--acc` tokens.
- State mapping from the study: listening -> listening/yellow,
  transcribing -> composing/brown, review -> breathing/quartz,
  sent -> solving/green, working -> working/niagara,
  speaking -> listening/green, uncertain -> shaping/wisteria,
  error -> breathing slowed/red, disconnected -> connecting/bg4.
- A 30px canvas replaces the `.orb i` disc; the mic-level `--lv` scale
  transform stays on the container.
- Reduced motion renders one static frame. The RAF loop stops while
  the pill window is hidden (WebKit throttles hidden pages; do not
  rely on rAF ticking).

## Verification

- Each pill state shows its mapped animation in its accent.
- The vendored engine file is byte-identical to the package except the
  export wrapper, license header intact.
- Reduced motion shows a still frame; a hidden pill runs no RAF.
- The desktop build passes (`cargo build` runs tsc via build.rs).

Reference: Orb Study artifact,
https://claude.ai/code/artifact/0c723f03-fc1e-48ad-8d10-59f4ba06c855
(the gruber painter and the state mapping are on the page).
