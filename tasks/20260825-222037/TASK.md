# Pill orb on the thinking-orbs engine

- STATUS: IN_PROGRESS
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

Design document: `tasks/20260825-222037/orb-study.html` - the working
demo with the real engine inline, the gruber painter, and the state
mapping. Published copy:
https://claude.ai/code/artifact/0c723f03-fc1e-48ad-8d10-59f4ba06c855

## Verification evidence (2026-08-25)

Implemented in `desktop/scufris-desktop/`: vendored
`ui/orb-engine.js`, hand-written `ui/orb-engine.d.ts`, a 30px canvas
in `ui/index.html`, the painter and state mapping in `ui/pill.ts`,
the dead disc rules removed from `ui/pill.css`, the new files added
to `build.rs` and `ui/tsconfig.json`, and the engine added to
`.prettierignore`.

- Vendored file is byte-identical. `diff` of `ui/orb-engine.js`
  below its 24-line license header against lines 1-545 of
  `thinking-orbs@0.3.1` `dist/engine.es.js`: no difference. Only the
  final `export { ... }` is replaced by the `window.OrbEngine`
  wrapper, exactly as in the study. `node --check` passes.
- Prettier never rewrites it. `npx prettier --stdin-filepath x.js`
  on its contents differs from the file, so the ignore entry is what
  keeps it stable; `npm run format:check` passes repo-wide.
- `tsc -p ui/tsconfig.json` (the invocation `build.rs` makes) passes
  under `strict` and `noUncheckedIndexedAccess`.
- `cargo build -p scufris-desktop` inside `nix develop` succeeds
  (8.6s). `ui/dist/orb-engine.js` is copied byte-identical.
- Headless smoke run of the compiled `ui/dist/pill.js` against a stub
  DOM, driving every `data-state` through the real presentation
  listener: each state paints its mapped mode (idle/review/error and
  the wisteria states 120 arcs from `ring`, listening/speaking 42 from
  `wave`, transcribing 208 from `ribbon`, sent 30 from `rubik`,
  working 39 from `orbits`, uncertain 18 from `morph`, disconnected 9
  from `web` plus a stroked packet edge), each in its own `--acc` mix.
  Backing store is 60x60 at devicePixelRatio 2.
- RAF discipline in the same run: one pending callback while visible,
  zero after `visibilitychange` to hidden, one again on visible. With
  `prefers-reduced-motion`, zero callbacks are ever scheduled and one
  static frame is painted per state change.
- `npm run check` reports 48 test failures; the same 48 fail on clean
  `master` with the change stashed, so they are pre-existing and
  unrelated.

Still open: the visual per-state check on a live desktop is Alex's.
Nothing above proves the orb reads well at 30px, that the accent mix
carries in the panel, or that the `--lv` scale still feels right.

## Bare orb vision (2026-08-25)

Alex sketched a further direction after the orb and the pill polish
landed: the orb alone is the pill. No frame, no label; the state reads
from the orb's shape and accent alone, and a review textbox
materializes above the orb only when a transcript needs a decision.
The listening timer is useful (whisper caps a take at two minutes) but
not structural - it could be one small line under the orb, surfacing
only near the cap. The study page gained section 03 ("The pill is the
orb") demonstrating it live with the 64px preset, the pop-in review
box, and the small timer. Exploration only; no implementation decision
yet.
