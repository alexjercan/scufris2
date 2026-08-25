// Pill frontend. It renders the presentation the companion publishes and
// forwards the keys the accepted interaction defines. Every decision stays in
// the Rust state machine, including whether a key sends anything at all.
//
// The look is the reviewed HUD design (tasks/20260822-132001/scufris-hud.html):
// a sprung glow and a Canvas 2D wave ride the 60 ms mic level while listening,
// breathe at idle, and four near-subliminal earcons mark the boundaries the
// eye can miss. rAF runs only in audio-reactive states.

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

// Console output and uncaught failures are forwarded to the companion at
// DEBUG under the `webview` target, so pill behaviour is readable from
// journalctl and --foreground runs. Forwarding must never throw or reject,
// or an uncaught rejection would forward itself forever.
function forwardLog(level, message) {
  try {
    invoke("pill_log", { level, message }).catch(() => {});
  } catch {
    // Nothing to do: the log stays in the webview console only.
  }
}

function logText(value) {
  if (typeof value === "string") return value;
  try {
    return JSON.stringify(value) ?? String(value);
  } catch {
    return String(value);
  }
}

for (const level of ["debug", "log", "info", "warn", "error"]) {
  const original = console[level].bind(console);
  console[level] = (...args) => {
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

const pill = document.getElementById("pill");
const label = document.getElementById("label");
const transcript = document.getElementById("transcript");
const detail = document.getElementById("detail");
const timer = document.getElementById("timer");
const wave = document.getElementById("wave");

const reducedMotion = matchMedia("(prefers-reduced-motion: reduce)");

const LABELS = {
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

// Four boundary cues, ever: mic open, mic close, attention, error. Nothing
// for working or speaking. Retained and uncertain are attention-class: the
// tray already presents them as "needs you". Near-subliminal by gain; the
// tray menu mutes them.
const CUES = {
  open: [{ from: 520, to: 760, dur: 0.09, gain: 0.05 }],
  close: [{ from: 760, to: 520, dur: 0.09, gain: 0.05 }],
  chime: [
    { from: 880, dur: 0.35, gain: 0.045 },
    { from: 1318, dur: 0.28, gain: 0.02 },
  ],
  error: [
    { from: 165, dur: 0.18, gain: 0.06 },
    { from: 110, dur: 0.18, gain: 0.03 },
  ],
};

let cuesEnabled = true;
let audio = null;

function tone(parts) {
  if (!cuesEnabled) return;
  try {
    audio = audio ?? new AudioContext();
    if (audio.state === "suspended") audio.resume().catch(() => {});
    const now = audio.currentTime;
    for (const part of parts) {
      const osc = audio.createOscillator();
      const gain = audio.createGain();
      osc.type = "sine";
      osc.frequency.setValueAtTime(part.from, now);
      if (part.to) {
        osc.frequency.exponentialRampToValueAtTime(part.to, now + part.dur);
      }
      gain.gain.setValueAtTime(0.0001, now);
      gain.gain.exponentialRampToValueAtTime(part.gain, now + 0.015);
      gain.gain.exponentialRampToValueAtTime(0.0001, now + part.dur);
      osc.connect(gain).connect(audio.destination);
      osc.start(now);
      osc.stop(now + part.dur + 0.05);
    }
  } catch {
    // No audio output is a silent pill, not a broken one.
  }
}

function boundaryCue(previous, next) {
  if (next === "listening") return tone(CUES.open);
  // An error crossing swallows the mic-close cue: one boundary, one sound.
  if (next === "error") return tone(CUES.error);
  if (next === "attention" || next === "retained" || next === "uncertain") {
    return tone(CUES.chime);
  }
  if (previous === "listening") return tone(CUES.close);
}

invoke("pill_cues")
  .then((enabled) => {
    cuesEnabled = enabled;
  })
  .catch(() => {});

listen("scufris://cues", (event) => {
  cuesEnabled = event.payload;
});

// ---------- glow and wave ----------

// The displayed level springs toward the target each frame:
// display += (target - display) * 0.25. The target is the 60 ms mic tick
// while listening, a synthesized pulse while speaking (no TTS level exists
// yet), and a per-state resting value otherwise.
const BASELINE = {
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
{
  const scale = window.devicePixelRatio || 1;
  wave.width = WAVE_WIDTH * scale;
  wave.height = WAVE_HEIGHT * scale;
  waveContext.setTransform(scale, 0, 0, scale, 0, 0);
}

let levelTarget = BASELINE.idle;
let levelShown = BASELINE.idle;
let phase = 0;
let frameId = null;
let waveColor = "#95a99f";

function drawWave(level) {
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

function applyLevel(level) {
  pill.style.setProperty("--lv", level.toFixed(3));
  if (WAVE_STATES.has(pill.dataset.state)) drawWave(level);
}

function frame() {
  phase += 1 / 60;
  if (pill.dataset.state === "speaking") {
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
function retune(state) {
  levelTarget = BASELINE[state] ?? 0.15;
  waveColor =
    getComputedStyle(pill).getPropertyValue("--acc").trim() || "#95a99f";
  if (reducedMotion.matches || !REACTIVE.has(state)) {
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

reducedMotion.addEventListener?.("change", () => retune(pill.dataset.state));

// ---------- rendering ----------

function formatDuration(seconds) {
  const minutes = Math.floor(seconds / 60);
  const rest = seconds % 60;
  return `${minutes}:${String(rest).padStart(2, "0")}`;
}

function render(presentation) {
  const previous = pill.dataset.state;
  pill.dataset.state = presentation.state;
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

listen("scufris://presentation", (event) => render(event.payload));

listen("scufris://copy", (event) => {
  // Copying is the safe choice offered for a transcript whose outcome nobody
  // knows, so a clipboard that refuses must not look like anything happened.
  navigator.clipboard?.writeText(event.payload).catch(() => {});
});

listen("scufris://tick", (event) => {
  timer.textContent = formatDuration(event.payload.seconds);
  if (pill.dataset.state !== "listening") return;
  levelTarget = 0.12 + Math.min(event.payload.level, 1) * 0.88;
  if (reducedMotion.matches) {
    // No spring without the loop: the tick itself is the crossfade.
    levelShown = levelTarget;
    applyLevel(levelShown);
  }
});

window.addEventListener("keydown", (event) => {
  if (event.key === "Enter") {
    event.preventDefault();
    invoke("pill_submit", {
      text: transcript.readOnly || transcript.hidden ? null : transcript.value,
    });
    return;
  }
  if (event.key === "Escape") {
    event.preventDefault();
    invoke("pill_cancel");
    return;
  }
  if (event.key === "c" && (event.ctrlKey || event.metaKey)) {
    // Only when nothing is selected: an ordinary copy inside the field stays
    // an ordinary copy.
    if (transcript.selectionStart !== transcript.selectionEnd) return;
    event.preventDefault();
    invoke("pill_copy");
  }
});

retune(pill.dataset.state);
invoke("pill_ready");
