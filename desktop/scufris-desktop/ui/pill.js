// Pill frontend. It renders the presentation the companion publishes and
// forwards the keys the accepted interaction defines. Every decision stays in
// the Rust state machine, including whether a key sends anything at all.

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const pill = document.getElementById("pill");
const label = document.getElementById("label");
const transcript = document.getElementById("transcript");
const detail = document.getElementById("detail");
const timer = document.getElementById("timer");

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

function formatDuration(seconds) {
  const minutes = Math.floor(seconds / 60);
  const rest = seconds % 60;
  return `${minutes}:${String(rest).padStart(2, "0")}`;
}

function render(presentation) {
  pill.dataset.state = presentation.state;
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
    pill.style.setProperty("--level", "0.55");
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
  const level = 0.55 + Math.min(event.payload.level, 1) * 0.75;
  pill.style.setProperty("--level", level.toFixed(3));
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

invoke("pill_ready");
