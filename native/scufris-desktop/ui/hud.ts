// The conversation window: every line the service has pushed, and a field to
// add one from.
//
// This page renders and reports, the way the pill and the textbox pages do.
// Every decision is the Rust side's: src/conversation.rs decides what a typed
// line means and src/hud.rs decides what the window does. What arrives here is
// lines to draw and a notice to show; what leaves is Enter and Escape.
//
// The lines it draws are text. Not markdown, not tool calls, not thinking - the
// service's transcript is what was said, and the session file and
// `scufris-ctl debug` are where the rest of a run lives.
//
// Wrapped in a block: the pages are separate classic scripts in one tsc
// project, so a name at the top level of one is a name in the others' global
// scope.
//
// Compiled by tsc from build.rs into ui/dist; the window loads the output.

{
  const { invoke } = window.__TAURI__.core;
  const { listen } = window.__TAURI__.event;

  const forward = (level: string, message: string): void => {
    try {
      invoke("pill_log", { level: `hud.${level}`, message }).catch(() => {});
    } catch {
      // Nothing to do: the log stays in the webview console only.
    }
  };

  window.addEventListener("error", (event) => {
    forward("error", `uncaught: ${event.message}`);
  });

  window.addEventListener("unhandledrejection", () => {
    forward("error", "unhandled rejection");
  });

  const element = <T extends HTMLElement>(id: string): T => {
    const found = document.getElementById(id);
    if (found === null) throw new Error(`the HUD page is missing #${id}`);
    return found as T;
  };

  const lines = element<HTMLOListElement>("lines");
  const notice = element<HTMLElement>("notice");
  const words = element<HTMLTextAreaElement>("words");

  // What the gutter says for each speaker. A speaker with no word here is one
  // this build does not know about, and it is drawn rather than dropped: a line
  // that was said belongs on screen whoever the service says said it.
  const WHO: Record<string, string> = {
    user: "you",
    assistant: "scufris",
  };

  /** What one line does to the notice line, when nothing is in flight. */
  const KEYS = "enter sends - esc closes";

  // ---------- drawing ----------

  /** True while the person is reading the bottom of the conversation. */
  const atBottom = (): boolean =>
    lines.scrollHeight - lines.scrollTop - lines.clientHeight < 24;

  const draw = (entry: TranscriptEntry): HTMLLIElement => {
    const line = document.createElement("li");
    line.className = "line";
    line.dataset["speaker"] = entry.speaker;

    const who = document.createElement("span");
    who.className = "who";
    who.textContent = WHO[entry.speaker] ?? entry.speaker;

    const what = document.createElement("span");
    what.className = "what";
    // textContent, never innerHTML. What the service pushes is whatever was
    // said, and what was said is not markup.
    what.textContent = entry.text;

    line.append(who, what);
    return line;
  };

  const append = (entry: TranscriptEntry): void => {
    // Whether to follow is decided before the line goes in, because adding it
    // is what changes the answer. A person who has scrolled up to read
    // something is not dragged back down by the next line arriving.
    const follow = atBottom();
    lines.append(draw(entry));
    if (follow) lines.scrollTop = lines.scrollHeight;
  };

  const replace = (entries: TranscriptEntry[]): void => {
    lines.replaceChildren(...entries.map(draw));
    lines.scrollTop = lines.scrollHeight;
  };

  const say = (state: Notice): void => {
    if (state.trouble !== "") {
      notice.dataset["tone"] = "trouble";
      notice.textContent = state.trouble;
      return;
    }
    if (state.sending) {
      notice.dataset["tone"] = "sending";
      notice.textContent = "sending";
      return;
    }
    notice.dataset["tone"] = "keys";
    notice.textContent = KEYS;
  };

  // ---------- the field ----------

  // The window cannot be resized once it is up, so the field grows into the
  // room the conversation is using and stops. Measured rather than counted:
  // the type is monospace by preference and not by guarantee, and it wraps.
  const fit = (): void => {
    words.style.height = "auto";
    words.style.height = `${words.scrollHeight}px`;
  };

  words.addEventListener("input", fit);

  const send = (): void => {
    const text = words.value;
    if (text.trim() === "") return;
    // Cleared here rather than when the service answers. The person has moved
    // on to the next thing they want to say, and a field that stayed full for
    // the length of a round trip is one they would type over.
    words.value = "";
    fit();
    void invoke("hud_submit", { text });
  };

  window.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      event.preventDefault();
      void invoke("hud_close");
      return;
    }
    if (event.key === "Enter" && !event.shiftKey) {
      // Shift+Enter is the newline. One Enter is one message, which is the
      // same bargain the textbox makes.
      event.preventDefault();
      send();
    }
  });

  // A window manager can hand this window the keyboard without the field
  // taking it - a click on the border, a focus-follows-mouse enter. The keys
  // are the field's.
  window.addEventListener("focus", () => {
    if (document.activeElement === words) return;
    words.focus();
  });

  // ---------- what the host says ----------

  void listen("scufris://said", (event) => {
    append(event.payload as TranscriptEntry);
  });

  // The service replays its whole transcript ring to a frontend that connects,
  // so a reconnection is a whole conversation arriving rather than a line.
  void listen("scufris://conversation", (event) => {
    const backlog = event.payload as Backlog;
    replace(backlog.lines);
    say(backlog.notice);
  });

  void listen("scufris://notice", (event) => {
    say(event.payload as Notice);
  });

  // The window is built at startup and filled whether it is on screen or not,
  // so there is usually a backlog by the time anybody opens it.
  void (async () => {
    const backlog = (await invoke("hud_ready")) as Backlog;
    replace(backlog.lines);
    say(backlog.notice);
    fit();
    words.focus();
  })();
}
