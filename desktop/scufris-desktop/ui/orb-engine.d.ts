// Hand-written declarations for the vendored orb-engine.js, covering only the
// surface pill.ts uses. The engine ships React types the pill has no use for,
// so nothing here is generated from the package.

/** One projected dot. `white` is the engine's ink value, 0 = darkest ink. */
interface OrbDot {
  x: number;
  y: number;
  r: number;
  white: number;
  a?: number;
}

/** One stroked edge; `white` follows the dot convention. */
interface OrbLine {
  x1: number;
  y1: number;
  x2: number;
  y2: number;
  white: number;
  a?: number;
  w: number;
}

/** A finished instant: lines are drawn first, dots are already z-sorted. */
interface OrbFrame {
  dots: OrbDot[];
  lines: OrbLine[];
}

type OrbMode =
  | "orbits"
  | "globe"
  | "rubik"
  | "wave"
  | "web"
  | "braid"
  | "ribbon"
  | "ring"
  | "morph";

type OrbEngineState =
  | "working"
  | "searching"
  | "solving"
  | "listening"
  | "connecting"
  | "weaving"
  | "composing"
  | "breathing"
  | "shaping";

/** Draw options. Opaque to the pill: resolved once, passed straight back. */
type OrbOpts = Record<string, number | undefined>;

interface OrbResolved {
  mode: OrbMode;
  speed: number;
  opts: OrbOpts;
}

interface OrbEngine {
  MODE_FRAMES: Record<
    OrbMode,
    (size: number, t: number, opts: OrbOpts) => OrbFrame
  >;
  /** Only 64 and 20 are tuned; the pill draws the 20 preset at 36 px. */
  resolvePreset(state: OrbEngineState, size: 64 | 20): OrbResolved;
}

interface Window {
  OrbEngine: OrbEngine;
}
