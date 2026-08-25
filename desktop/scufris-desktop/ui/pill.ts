// Pill frontend. It renders the presentation the companion publishes and
// forwards the keys the accepted interaction defines. Every decision stays in
// the Rust state machine, including whether a key sends anything at all.
//
// The look is the reviewed HUD design (tasks/20260822-132001/scufris-hud.html):
// a sprung glow and a Canvas 2D wave ride the 60 ms mic level while listening,
// breathe at idle, and four soft earcons mark the boundaries the eye can miss.
// rAF runs only in audio-reactive states.
//
// Compiled by tsc from build.rs into ui/dist; the window loads the output.

interface TauriCore {
  invoke(command: string, args?: Record<string, unknown>): Promise<unknown>;
}

interface TauriEventModule {
  listen(
    event: string,
    handler: (event: { payload: unknown }) => void,
  ): Promise<unknown>;
}

interface Window {
  __TAURI__: { core: TauriCore; event: TauriEventModule };
}

// The payload shapes are owned by the Rust side (app.rs); the casts at the
// listen boundaries are the one place the frontend takes them on trust.
interface Presentation {
  state: string;
  detail: string;
  text: string;
  editable: boolean;
  recording: boolean;
}

interface Tick {
  seconds: number;
  level: number;
}

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

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
const label = element<HTMLElement>("label");
const transcript = element<HTMLInputElement>("transcript");
const detail = element<HTMLElement>("detail");
const timer = element<HTMLElement>("timer");
const wave = element<HTMLCanvasElement>("wave");

const reducedMotion = matchMedia("(prefers-reduced-motion: reduce)");

function currentState(): string {
  return pill.dataset["state"] ?? "idle";
}

const LABELS: Record<string, string> = {
  idle: "Scufris",
  listening: "Listening",
  transcribing: "Transcribing",
  review: "Review and send",
  sent: "Sent",
  retained: "Not delivered",
  uncertain: "Sent, outcome unknown",
  error: "Nothing was sent",
  working: "Working",
  speaking: "Speaking",
  attention: "Needs you",
  disconnected: "Backend unavailable",
};

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

// ---------- glow and wave ----------

// The displayed level springs toward the target each frame:
// display += (target - display) * 0.25. The target is the 60 ms mic tick
// while listening, a synthesized pulse while speaking (no TTS level exists
// yet), and a per-state resting value otherwise.
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
  error: 0.22,
  disconnected: 0.1,
};

const REACTIVE = new Set(["listening", "speaking"]);
const WAVE_STATES = new Set(["listening", "transcribing", "speaking"]);
const WAVE_WIDTH = 64;
const WAVE_HEIGHT = 26;
const BARS = 14;

const waveContext = wave.getContext("2d");
if (waveContext !== null) {
  const scale = window.devicePixelRatio || 1;
  wave.width = WAVE_WIDTH * scale;
  wave.height = WAVE_HEIGHT * scale;
  waveContext.setTransform(scale, 0, 0, scale, 0, 0);
}

let levelTarget = BASELINE["idle"] ?? 0.15;
let levelShown = levelTarget;
let phase = 0;
let frameId: number | null = null;
let waveColor = "#95a99f";

function drawWave(level: number): void {
  if (waveContext === null) return;
  waveContext.clearRect(0, 0, WAVE_WIDTH, WAVE_HEIGHT);
  waveContext.fillStyle = waveColor;
  const barWidth = 2;
  const gap = (WAVE_WIDTH - BARS * barWidth) / (BARS - 1);
  for (let i = 0; i < BARS; i += 1) {
    const mid = 1 - Math.abs(i - (BARS - 1) / 2) / ((BARS - 1) / 2);
    const wobble = 0.55 + 0.45 * Math.sin(i * 2.7 + phase * (6 + (i % 5)));
    const height = Math.max(
      2,
      WAVE_HEIGHT * (0.1 + 0.9 * level) * (0.35 + 0.65 * mid) * wobble,
    );
    waveContext.fillRect(
      i * (barWidth + gap),
      (WAVE_HEIGHT - height) / 2,
      barWidth,
      height,
    );
  }
}

function applyLevel(level: number): void {
  pill.style.setProperty("--lv", level.toFixed(3));
  if (WAVE_STATES.has(currentState())) drawWave(level);
}

function frame(): void {
  phase += 1 / 60;
  if (currentState() === "speaking") {
    levelTarget =
      0.4 + 0.2 * Math.sin(phase * 3.1) + 0.15 * Math.sin(phase * 7.3 + 1.2);
  }
  levelShown += (levelTarget - levelShown) * 0.25;
  applyLevel(levelShown);
  frameId = requestAnimationFrame(frame);
}

// Starts or stops the frame loop for one state. Non-reactive states settle to
// their baseline once and cost nothing per frame; reduced motion never loops
// and gets its crossfade from the CSS transitions instead.
function retune(next: string): void {
  levelTarget = BASELINE[next] ?? 0.15;
  waveColor =
    getComputedStyle(pill).getPropertyValue("--acc").trim() || "#95a99f";
  if (reducedMotion.matches || !REACTIVE.has(next)) {
    if (frameId !== null) {
      cancelAnimationFrame(frameId);
      frameId = null;
    }
    levelShown = levelTarget;
    applyLevel(levelShown);
    return;
  }
  if (frameId === null) frameId = requestAnimationFrame(frame);
}

reducedMotion.addEventListener?.("change", () => retune(currentState()));

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
  label.textContent = LABELS[presentation.state] ?? "Scufris";
  detail.textContent = presentation.detail;
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
}

void listen("scufris://presentation", (event) => {
  render(event.payload as Presentation);
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
  if (reducedMotion.matches) {
    // No spring without the loop: the tick itself is the crossfade.
    levelShown = levelTarget;
    applyLevel(levelShown);
  }
});

window.addEventListener("keydown", (event) => {
  if (event.key === "Enter") {
    event.preventDefault();
    void invoke("pill_submit", {
      text: transcript.readOnly || transcript.hidden ? null : transcript.value,
    });
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
  }
});

retune(currentState());
void invoke("pill_ready");
