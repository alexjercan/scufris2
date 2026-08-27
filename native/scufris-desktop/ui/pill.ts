// Pill frontend. It renders the presentation the companion publishes and
// forwards the keys the accepted interaction defines. Every decision stays in
// the Rust state machine, including whether a key sends anything at all.
//
// The look is the Orb Study's bare orb (tasks/20260825-231826/orb-study.html,
// section 03): the window is a frame around the vendored thinking-orbs engine,
// repainted in gruber ink, and the orb's shape and accent are the whole state.
// The only other mark is the listening timer. One rAF loop drives the mic level
// and the orb; it stops under reduced motion and while the window is hidden.
// Four soft earcons mark the boundaries the eye can miss.
//
// The words live here even though they are read next door: the keys belong to
// this window, so the field the person types into does too, invisible, and
// every edit is mirrored to the review window.
//
// Arriving is shared with the host: the window rises into place from below,
// and the orb pops open and squashes once as it lands. The host cannot read
// prefers-reduced-motion, so this page reports it once.
//
// Compiled by tsc from build.rs into ui/dist; the window loads the output.

const { invoke } = window.__TAURI__.core;
const { listen, emitTo } = window.__TAURI__.event;

// Console output and uncaught failures are forwarded to the companion at
// DEBUG under the `webview` target, so pill behaviour is readable from
// journalctl and --foreground runs. Forwarding must never throw or reject,
// or an uncaught rejection would forward itself forever.
function forwardLog(level: string, message: string): void {
  try {
    invoke("pill_log", { level, message }).catch(() => {});
  } catch {
    // Nothing to do: the log stays in the webview console only.
  }
}

function logText(value: unknown): string {
  if (typeof value === "string") return value;
  try {
    return JSON.stringify(value) ?? String(value);
  } catch {
    return String(value);
  }
}

const LEVELS = ["debug", "log", "info", "warn", "error"] as const;
for (const level of LEVELS) {
  const original = console[level].bind(console);
  console[level] = (...args: unknown[]) => {
    original(...args);
    forwardLog(level, args.map(logText).join(" "));
  };
}

window.addEventListener("error", (event) => {
  forwardLog("error", `uncaught: ${event.message}`);
});

window.addEventListener("unhandledrejection", (event) => {
  forwardLog("error", `unhandled rejection: ${logText(event.reason)}`);
});

// The ids live in our own index.html, so a missing element is a build defect
// worth failing loudly on, not a condition to soldier through.
function element<T extends HTMLElement>(id: string): T {
  const found = document.getElementById(id);
  if (found === null) throw new Error(`the pill page is missing #${id}`);
  return found as T;
}

const pill = element<HTMLElement>("pill");
const transcript = element<HTMLInputElement>("transcript");
const timer = element<HTMLElement>("timer");
const orb = element<HTMLCanvasElement>("orb");

const reducedMotion = matchMedia("(prefers-reduced-motion: reduce)");

function currentState(): string {
  return pill.dataset["state"] ?? "idle";
}

// ---------- earcons ----------

interface TonePart {
  from: number;
  to?: number;
  dur: number;
  gain: number;
}

// Four boundary cues, ever: mic open, mic close, attention, error. Nothing
// for working or speaking. Retained and uncertain are attention-class: the
// tray already presents them as "needs you". Soft but audible by gain; the
// tray menu mutes them.
const CUES: Record<string, TonePart[]> = {
  open: [{ from: 520, to: 760, dur: 0.09, gain: 0.14 }],
  close: [{ from: 760, to: 520, dur: 0.09, gain: 0.14 }],
  chime: [
    { from: 880, dur: 0.35, gain: 0.12 },
    { from: 1318, dur: 0.28, gain: 0.05 },
  ],
  error: [
    { from: 165, dur: 0.18, gain: 0.16 },
    { from: 110, dur: 0.18, gain: 0.08 },
  ],
};

let cuesEnabled = true;
let audio: AudioContext | null = null;
let warnedSuspended = false;

function tone(name: string): void {
  if (!cuesEnabled) return;
  const parts = CUES[name];
  if (parts === undefined) return;
  try {
    const context = audio ?? new AudioContext();
    audio = context;
    if (context.state === "suspended") {
      void context
        .resume()
        .then(() => {
          // An autoplay policy that keeps the context suspended makes every
          // cue silent; say so once so journalctl explains the silence.
          if (context.state === "suspended" && !warnedSuspended) {
            warnedSuspended = true;
            console.warn(
              "cues are silenced: the audio context stays suspended",
            );
          }
        })
        .catch(() => {});
    }
    const now = context.currentTime;
    for (const part of parts) {
      const osc = context.createOscillator();
      const gain = context.createGain();
      osc.type = "sine";
      osc.frequency.setValueAtTime(part.from, now);
      if (part.to !== undefined) {
        osc.frequency.exponentialRampToValueAtTime(part.to, now + part.dur);
      }
      gain.gain.setValueAtTime(0.0001, now);
      gain.gain.exponentialRampToValueAtTime(part.gain, now + 0.015);
      gain.gain.exponentialRampToValueAtTime(0.0001, now + part.dur);
      osc.connect(gain).connect(context.destination);
      osc.start(now);
      osc.stop(now + part.dur + 0.05);
    }
    console.debug(`cue ${name} (audio ${context.state})`);
  } catch {
    // No audio output is a silent pill, not a broken one.
  }
}

function boundaryCue(previous: string, next: string): void {
  if (next === "listening") return tone("open");
  // An error crossing swallows the mic-close cue: one boundary, one sound.
  if (next === "error") return tone("error");
  if (next === "attention" || next === "retained" || next === "uncertain") {
    return tone("chime");
  }
  if (previous === "listening") return tone("close");
}

invoke("pill_cues")
  .then((enabled) => {
    cuesEnabled = enabled === true;
  })
  .catch(() => {});

void listen("scufris://cues", (event) => {
  cuesEnabled = event.payload === true;
});

// ---------- the mic level ----------

// The displayed level springs toward the target each frame:
// display += (target - display) * 0.25. The target is the 60 ms mic tick
// while listening, a synthesized pulse while speaking (no TTS level exists
// yet), and a per-state resting value otherwise. The whole orb breathes with
// it through --lv.
const BASELINE: Record<string, number> = {
  idle: 0.15,
  listening: 0.5,
  transcribing: 0.3,
  review: 0.16,
  sent: 0.3,
  retained: 0.3,
  uncertain: 0.3,
  working: 0.3,
  speaking: 0.5,
  attention: 0.3,
  detached: 0.12,
  error: 0.22,
  starting: 0.12,
  disconnected: 0.1,
};

let levelTarget = BASELINE["idle"] ?? 0.15;
let levelShown = levelTarget;
let phase = 0;
let frameId: number | null = null;

function applyLevel(level: number): void {
  pill.style.setProperty("--lv", level.toFixed(3));
}

// ---------- orb ----------

// The dotted thought orb, drawn by the vendored thinking-orbs engine
// (orb-engine.js). The engine returns a finished frame per instant; this
// painter turns its ink value into a panel-to-accent mix, so one accent
// carries the whole depth range instead of the engine's grayscale.
type Rgb = [number, number, number];

// The engine tunes two presets, 64 and 20, and 64 is the rich one: it is what
// the study drew and the only one worth a window of its own. Asking for any
// other size is not a size the preset table has.
const ORB_PRESET = 64;

// What the orb is drawn at, which is not the preset it comes from. The frame
// functions take the size as an argument and scale the dots by (size/300)^0.6,
// so the engine's own answer to a bigger orb is the same dots spread finer over
// a wider sphere - which is the direction its 64-to-20 tuning already points,
// in reverse. Drawing the 64 preset on a 64 canvas and scaling that up would
// keep the study's proportions and give 160 pixels of 64-pixel drawing instead.
const ORB_SIZE = 160;
const PANEL: Rgb = [16, 16, 16];
const QUARTZ: Rgb = [149, 169, 159];
// The frozen instant reduced motion renders, in engine seconds.
const STILL = 2.35;

interface OrbLook {
  state: OrbEngineState;
  /** Multiplier over the preset's own speed. */
  speed: number;
}

// Pill state to engine verb. No accents here: pill.css owns those.
const ORB_LOOKS: Record<string, OrbLook> = {
  listening: { state: "listening", speed: 1 },
  transcribing: { state: "composing", speed: 1 },
  review: { state: "breathing", speed: 1 },
  sent: { state: "solving", speed: 1 },
  working: { state: "working", speed: 1 },
  speaking: { state: "listening", speed: 1 },
  uncertain: { state: "shaping", speed: 1 },
  detached: { state: "breathing", speed: 0.35 },
  error: { state: "breathing", speed: 0.35 },
  starting: { state: "connecting", speed: 1 },
  disconnected: { state: "connecting", speed: 1 },
};

// idle, retained and attention: a resting ring in whatever accent they carry.
const RESTING: OrbLook = { state: "breathing", speed: 1 };

const engine = window.OrbEngine;
const orbContext = orb.getContext("2d");
if (orbContext !== null) {
  // Capped at 2: past that the extra arc fills buy nothing at this size.
  const scale = Math.min(window.devicePixelRatio || 1, 2);
  orb.width = ORB_SIZE * scale;
  orb.height = ORB_SIZE * scale;
  orbContext.scale(scale, scale);
}

let orbLook = RESTING;
let orbPreset = engine.resolvePreset(RESTING.state, ORB_PRESET);
let orbAccent: Rgb = QUARTZ;

function mix(from: Rgb, to: Rgb, f: number, alpha: number): string {
  const r = Math.round(from[0] + (to[0] - from[0]) * f);
  const g = Math.round(from[1] + (to[1] - from[1]) * f);
  const b = Math.round(from[2] + (to[2] - from[2]) * f);
  return `rgba(${r},${g},${b},${alpha})`;
}

function ink(white: number): number {
  return 1 - Math.min(1, Math.max(0, white));
}

// pill.css stays the single source of accent truth: --acc computes to the
// state's hex token, which the orb takes as a triple.
function parseAccent(value: string): Rgb | null {
  const digits = /^#([0-9a-f]{6})$/i.exec(value)?.[1];
  if (digits === undefined) return null;
  const packed = Number.parseInt(digits, 16);
  return [(packed >> 16) & 255, (packed >> 8) & 255, packed & 255];
}

function drawOrb(seconds: number): void {
  if (orbContext === null) return;
  const t = seconds * orbPreset.speed * orbLook.speed;
  const instant = engine.MODE_FRAMES[orbPreset.mode](
    ORB_SIZE,
    t,
    orbPreset.opts,
  );
  orbContext.clearRect(0, 0, ORB_SIZE, ORB_SIZE);
  // Lines first, so the nodes sit on top of their edges.
  for (const line of instant.lines) {
    orbContext.strokeStyle = mix(
      PANEL,
      orbAccent,
      ink(line.white),
      line.a ?? 1,
    );
    orbContext.lineWidth = line.w;
    orbContext.beginPath();
    orbContext.moveTo(line.x1, line.y1);
    orbContext.lineTo(line.x2, line.y2);
    orbContext.stroke();
  }
  for (const dot of instant.dots) {
    orbContext.fillStyle = mix(PANEL, orbAccent, ink(dot.white), dot.a ?? 1);
    orbContext.beginPath();
    orbContext.arc(dot.x, dot.y, dot.r, 0, Math.PI * 2);
    orbContext.fill();
  }
}

// ---------- the frame loop ----------

// One loop for the whole pill: the level spring and the orb. WebKit throttles
// a hidden page to a crawl, so a hidden pill stops the loop outright instead
// of trusting rAF to keep time.
function looping(): boolean {
  return !reducedMotion.matches && !document.hidden;
}

function frame(now: number): void {
  phase += 1 / 60;
  if (currentState() === "speaking") {
    levelTarget =
      0.4 + 0.2 * Math.sin(phase * 3.1) + 0.15 * Math.sin(phase * 7.3 + 1.2);
  }
  levelShown += (levelTarget - levelShown) * 0.25;
  applyLevel(levelShown);
  drawOrb(now / 1000);
  frameId = requestAnimationFrame(frame);
}

// Settles one state: its baseline level, its accent, its orb verb, and whether
// the loop runs at all. A still pill paints one frame per state change and
// costs nothing after it; the crossfades come from the CSS transitions.
function retune(next: string): void {
  levelTarget = BASELINE[next] ?? 0.15;
  const accent = getComputedStyle(pill).getPropertyValue("--acc").trim();
  orbAccent = parseAccent(accent) ?? QUARTZ;
  orbLook = ORB_LOOKS[next] ?? RESTING;
  orbPreset = engine.resolvePreset(orbLook.state, ORB_PRESET);
  if (looping()) {
    if (frameId === null) frameId = requestAnimationFrame(frame);
    return;
  }
  if (frameId !== null) {
    cancelAnimationFrame(frameId);
    frameId = null;
  }
  levelShown = levelTarget;
  applyLevel(levelShown);
  drawOrb(STILL);
}

reducedMotion.addEventListener?.("change", () => retune(currentState()));
document.addEventListener("visibilitychange", () => retune(currentState()));

// ---------- entrance ----------

// The host rises the window into its resting spot from below and tells the page
// the moment it starts; this half pops the orb open inside the frame and
// squashes it once as it lands. The window cannot be resized while it is up, so
// growing is something only the page can do. The host runs its half only on a
// hidden-to-visible transition, so this never replays for a re-render.
function arrive(): void {
  if (reducedMotion.matches) return;
  pill.classList.remove("arriving");
  // Reading a layout value between the two forces the restart; without it the
  // class change is coalesced and the animation carries on where it was.
  void pill.offsetWidth;
  pill.classList.add("arriving");
}

pill.addEventListener("animationend", (event) => {
  if (event.animationName === "arrive") pill.classList.remove("arriving");
});

void listen("scufris://entrance", () => arrive());

// ---------- the mirrored draft ----------

// The review window shows the words; this window holds them. Every edit and
// every caret move is sent next door, so what the person reads there is what
// Enter would send from here.
const REVIEW_WINDOW = "review";
let warnedMirror = false;

function editing(): boolean {
  return !transcript.hidden && !transcript.readOnly;
}

function mirror(): void {
  if (!editing()) return;
  const start = transcript.selectionStart ?? transcript.value.length;
  const end = transcript.selectionEnd ?? start;
  // Which end of a selection the person is dragging: the caret belongs there,
  // the same as it does in the field itself.
  const caret = transcript.selectionDirection === "backward" ? start : end;
  emitTo(REVIEW_WINDOW, "scufris://draft", {
    text: transcript.value,
    start,
    end,
    caret,
  }).catch((error: unknown) => {
    // Once only: a mirror that fails fails on every keystroke, and the words
    // being unreadable is already reported by the host.
    if (warnedMirror) return;
    warnedMirror = true;
    console.warn(`the transcript is not mirrored: ${logText(error)}`);
  });
}

transcript.addEventListener("input", mirror);
transcript.addEventListener("keyup", mirror);
transcript.addEventListener("select", mirror);
// Arrow keys, Home, and a click all move the caret without changing the text.
document.addEventListener("selectionchange", mirror);

// ---------- editing ----------

// The field is an ordinary text input, so the ordinary textbox keys are the
// port's: arrows, Ctrl and the arrows, Home, End, Backspace, Delete, shift to
// select, Ctrl+A, and the clipboard. Every one of them lands in the field and
// then in the mirror, because a keyup and a selectionchange follow it.
//
// The deletions below are done here rather than left to it. They are the ones
// a port either binds or does not - Ctrl+Backspace is a GTK binding, Ctrl+U and
// Ctrl+K are a terminal habit - and a transcript window that loses a word on
// one desktop and not on another is worse than one that decides for itself.
// Doing them here also means the mirror is told, whatever WebKit would have
// done.

/** Whitespace is the only word boundary: "don't" and "http://x" are one word. */
const BREAK = /\s/;

// A word deletion takes two runs: the one the caret is in and the one beyond
// it. Whichever way it goes, that is a word and the gap beside it, so deleting
// a word never leaves the two spaces that used to be either side of it.
function wordStart(text: string, at: number): number {
  let index = Math.max(0, Math.min(at, text.length));
  const gap = BREAK.test(text.charAt(index - 1));
  while (index > 0 && BREAK.test(text.charAt(index - 1)) === gap) index -= 1;
  while (index > 0 && BREAK.test(text.charAt(index - 1)) !== gap) index -= 1;
  return index;
}

function wordEnd(text: string, at: number): number {
  let index = Math.max(0, Math.min(at, text.length));
  const gap = BREAK.test(text.charAt(index));
  while (index < text.length && BREAK.test(text.charAt(index)) === gap) {
    index += 1;
  }
  while (index < text.length && BREAK.test(text.charAt(index)) !== gap) {
    index += 1;
  }
  return index;
}

/** What one key deletes, or null when the field keeps the key. */
function deletion(event: KeyboardEvent): [number, number] | null {
  if (!(event.ctrlKey || event.metaKey) || event.altKey) return null;
  const text = transcript.value;
  const start = transcript.selectionStart ?? text.length;
  const end = transcript.selectionEnd ?? start;
  // A selection is already a range, and deleting it is what the field's own
  // Backspace does. Only a caret needs a range worked out for it.
  if (start !== end) return null;
  if (event.key === "Backspace") return [wordStart(text, start), start];
  if (event.key === "Delete") return [start, wordEnd(text, start)];
  if (event.key === "u") return [0, start];
  if (event.key === "k") return [start, text.length];
  return null;
}

function erase(from: number, to: number): void {
  if (from >= to) return;
  const before = transcript.value;
  transcript.setSelectionRange(from, to);
  // execCommand keeps the field's own undo history, which setRangeText does
  // not, so it is tried first and checked rather than trusted: a command the
  // port does not carry can answer true and do nothing.
  if (!document.execCommand("delete") || transcript.value === before) {
    transcript.setRangeText("", from, to, "end");
  }
  // setRangeText raises no input event, and neither path may skip the mirror.
  mirror();
}

// ---------- rendering ----------

function formatDuration(seconds: number): string {
  const minutes = Math.floor(seconds / 60);
  const rest = seconds % 60;
  return `${minutes}:${String(rest).padStart(2, "0")}`;
}

function render(presentation: Presentation): void {
  const previous = currentState();
  pill.dataset["state"] = presentation.state;
  if (previous !== presentation.state) {
    boundaryCue(previous, presentation.state);
    retune(presentation.state);
  }
  transcript.hidden = presentation.text === "" && !presentation.editable;
  transcript.readOnly = !presentation.editable;
  const editing = document.activeElement === transcript;
  if (!editing || !presentation.editable) {
    transcript.value = presentation.text;
  }
  if (presentation.editable && !editing) {
    transcript.focus();
    transcript.setSelectionRange(
      transcript.value.length,
      transcript.value.length,
    );
  }
  if (!presentation.recording) {
    timer.textContent = "";
  }
  mirror();
}

void listen("scufris://presentation", (event) => {
  render(event.payload as Presentation);
});

// A click on this window is how a person brings it back to the front, and the
// click's own default is to move the page's focus to whatever lies under the
// pointer. On a window whose review is read in the box next door, that is
// anything but the field the keys are meant for. Enter and Escape live through
// it, because they are read from the window; the arrows, Backspace, and every
// letter the person types are read from the field, and a field the focus has
// left reads nothing. So the click that was meant to give the window back
// gives back half of it, and the missing half is the editing this window
// exists for.
document.addEventListener("mousedown", (event) => {
  if (editing()) event.preventDefault();
});

// The focus can still be moved off the field by a desktop that hands this
// window the keyboard on its own, so a window that comes back editable takes
// the field again and puts the caret where the person left it. The box next
// door is redrawn from the offsets, because it has been showing a caret in a
// window that could not move it.
window.addEventListener("focus", () => {
  if (!editing()) return;
  if (document.activeElement === transcript) return;
  const start = transcript.selectionStart ?? transcript.value.length;
  const end = transcript.selectionEnd ?? start;
  transcript.focus();
  transcript.setSelectionRange(start, end);
  mirror();
});

void listen("scufris://copy", (event) => {
  // Copying is the safe choice offered for a transcript whose outcome nobody
  // knows, so a clipboard that refuses must not look like anything happened.
  navigator.clipboard?.writeText(event.payload as string).catch(() => {});
});

void listen("scufris://tick", (event) => {
  const tick = event.payload as Tick;
  timer.textContent = formatDuration(tick.seconds);
  if (currentState() !== "listening") return;
  levelTarget = 0.12 + Math.min(tick.level, 1) * 0.88;
  if (frameId === null) {
    // No spring without the loop: the tick itself is the crossfade.
    levelShown = levelTarget;
    applyLevel(levelShown);
  }
});

// Accepting what the pill is showing, with whatever is in the field. The one
// place that decides which words are sent, so a key pressed here and a verb
// arriving from the window manager send the same ones.
const accept = (): void => {
  void invoke("pill_submit", {
    text: transcript.readOnly || transcript.hidden ? null : transcript.value,
  });
};

// Enter arriving from outside the window, because the window does not hold the
// keyboard. Escape has no twin here: it carries no words, so it goes straight
// to the runtime and works even when this page does not.
void listen("scufris://accept", () => {
  accept();
});

window.addEventListener("keydown", (event) => {
  if (event.key === "Enter") {
    event.preventDefault();
    accept();
    return;
  }
  if (event.key === "Escape") {
    event.preventDefault();
    void invoke("pill_cancel");
    return;
  }
  if (event.key === "c" && (event.ctrlKey || event.metaKey)) {
    // Only when nothing is selected: an ordinary copy inside the field stays
    // an ordinary copy.
    if (transcript.selectionStart !== transcript.selectionEnd) return;
    event.preventDefault();
    void invoke("pill_copy");
    return;
  }
  // Nothing is edited where nothing is editable: the words of a frozen
  // transcript are not the person's to change from here.
  if (!editing()) return;
  const range = deletion(event);
  if (range !== null) {
    event.preventDefault();
    erase(range[0], range[1]);
  }
});

retune(currentState());
// The preference goes with the first hello: the host owns the window's half of
// the entrance and has no media query of its own to read.
void invoke("pill_ready", { reducedMotion: reducedMotion.matches });
