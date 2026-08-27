// The transcript box that rises above the orb, from the Orb Study section 03
// (tasks/20260825-231826/orb-study.html). It renders the same presentation the
// orb window renders - the host broadcasts it to both - and it is the one
// window here that holds the keyboard.
//
// So this is where the words are answered. Enter sends what is in the field,
// Escape discards it, Ctrl+C copies it, and every ordinary editing key works
// because it arrives in a focused window rather than being rescued from
// outside one. Every decision still belongs to the Rust state machine: this
// page reports which key was pressed and what the field holds, and nothing
// more.
//
// Wrapped in a block: pill.ts and this file are separate classic scripts in one
// tsc project, so a name at the top level of either is a name in the other's
// global scope.
//
// Compiled by tsc from build.rs into ui/dist; the window loads the output.

{
  const { invoke } = window.__TAURI__.core;
  const { listen } = window.__TAURI__.event;

  // Same forwarding as the pill page: a second window that fails silently is a
  // window nobody can debug from journalctl.
  const forward = (level: string, message: string): void => {
    try {
      invoke("pill_log", { level: `textbox.${level}`, message }).catch(
        () => {},
      );
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
    if (found === null) throw new Error(`the textbox page is missing #${id}`);
    return found as T;
  };

  const box = element<HTMLElement>("box");
  const words = element<HTMLTextAreaElement>("words");
  const hint = element<HTMLElement>("hint");

  const reducedMotion = matchMedia("(prefers-reduced-motion: reduce)");

  // The states this box exists for, and the one line each of them says. They
  // are the phases `state.rs` gives Posture::Editing; a state with no hint here
  // is a state the host keeps this window down in.
  const HINTS: Record<string, string> = {
    editing: "enter sends - esc discards",
    retained: "not sent - enter tries again - esc discards",
    uncertain: "the service is unsure - enter again sends anyway",
  };

  let showing = false;

  /** True while the person may change what is in the field. */
  const editable = (): boolean => !words.readOnly;

  /** True while the field is the one the keys are landing in. */
  const holding = (): boolean => document.activeElement === words;

  // A take longer than the box scrolls under a soft edge. Measured rather than
  // assumed: the type is monospace by preference and not by guarantee, and the
  // box wraps.
  const fade = (): void => {
    words.classList.toggle(
      "overflowing",
      words.scrollHeight > words.clientHeight,
    );
  };

  const pop = (): void => {
    if (reducedMotion.matches) return;
    box.classList.remove("popping");
    // Reading a layout value between the two forces the restart; without it the
    // class change is coalesced and the animation carries on where it was.
    void box.offsetWidth;
    box.classList.add("popping");
  };

  box.addEventListener("animationend", (event) => {
    if (event.animationName === "boxpop") box.classList.remove("popping");
  });

  words.addEventListener("input", fade);

  // ---------- the keys ----------

  // The field is an ordinary textarea, so the ordinary textbox keys are the
  // port's: arrows, Ctrl and the arrows, Home, End, Backspace, Delete, shift to
  // select, Ctrl+A, and the clipboard.
  //
  // The deletions below are done here rather than left to it. They are the ones
  // a port either binds or does not - Ctrl+Backspace is a GTK binding, Ctrl+U
  // and Ctrl+K are a terminal habit - and a transcript window that loses a word
  // on one desktop and not on another is worse than one that decides for
  // itself.

  /** Whitespace is the only word boundary: "don't" and "http://x" are one word. */
  const BREAK = /\s/;

  // A word deletion takes two runs: the one the caret is in and the one beyond
  // it. Whichever way it goes, that is a word and the gap beside it, so deleting
  // a word never leaves the two spaces that used to be either side of it.
  const wordStart = (text: string, at: number): number => {
    let index = Math.max(0, Math.min(at, text.length));
    const gap = BREAK.test(text.charAt(index - 1));
    while (index > 0 && BREAK.test(text.charAt(index - 1)) === gap) index -= 1;
    while (index > 0 && BREAK.test(text.charAt(index - 1)) !== gap) index -= 1;
    return index;
  };

  const wordEnd = (text: string, at: number): number => {
    let index = Math.max(0, Math.min(at, text.length));
    const gap = BREAK.test(text.charAt(index));
    while (index < text.length && BREAK.test(text.charAt(index)) === gap) {
      index += 1;
    }
    while (index < text.length && BREAK.test(text.charAt(index)) !== gap) {
      index += 1;
    }
    return index;
  };

  /** What one key deletes, or null when the field keeps the key. */
  const deletion = (event: KeyboardEvent): [number, number] | null => {
    if (!(event.ctrlKey || event.metaKey) || event.altKey) return null;
    const text = words.value;
    const start = words.selectionStart;
    const end = words.selectionEnd;
    // A selection is already a range, and deleting it is what the field's own
    // Backspace does. Only a caret needs a range worked out for it.
    if (start !== end) return null;
    if (event.key === "Backspace") return [wordStart(text, start), start];
    if (event.key === "Delete") return [start, wordEnd(text, start)];
    if (event.key === "u") return [0, start];
    if (event.key === "k") return [start, text.length];
    return null;
  };

  const erase = (from: number, to: number): void => {
    if (from >= to) return;
    const before = words.value;
    words.setSelectionRange(from, to);
    // execCommand keeps the field's own undo history, which setRangeText does
    // not, so it is tried first and checked rather than trusted: a command the
    // port does not carry can answer true and do nothing.
    if (!document.execCommand("delete") || words.value === before) {
      words.setRangeText("", from, to, "end");
    }
    // setRangeText raises no input event, so neither path may skip this.
    fade();
  };

  window.addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      // Never a newline. One take is one message, and the state machine is
      // what decides whether these words go anywhere.
      event.preventDefault();
      void invoke("textbox_submit", {
        text: editable() ? words.value : null,
      });
      return;
    }
    if (event.key === "Escape") {
      event.preventDefault();
      void invoke("textbox_cancel");
      return;
    }
    if (event.key === "c" && (event.ctrlKey || event.metaKey)) {
      // Only when nothing is selected: an ordinary copy inside the field stays
      // an ordinary copy.
      if (words.selectionStart !== words.selectionEnd) return;
      event.preventDefault();
      void invoke("textbox_copy");
      return;
    }
    // Nothing is edited where nothing is editable: the words of a frozen
    // transcript are not the person's to change from here.
    if (!editable()) return;
    const range = deletion(event);
    if (range !== null) {
      event.preventDefault();
      erase(range[0], range[1]);
    }
  });

  // A window manager can hand this window the keyboard without the field
  // taking it - a click on the border, a focus-follows-mouse enter. The keys
  // are the field's, so it takes them back and puts the caret where the person
  // left it.
  window.addEventListener("focus", () => {
    if (holding()) return;
    const start = words.selectionStart;
    const end = words.selectionEnd;
    words.focus();
    words.setSelectionRange(start, end);
  });

  // ---------- rendering ----------

  void listen("scufris://presentation", (event) => {
    const presentation = event.payload as Presentation;
    const line = HINTS[presentation.state];
    if (line === undefined) {
      // The host is taking this window down. Emptying it now means the next
      // transcript never appears behind a flash of the last one.
      showing = false;
      words.readOnly = true;
      words.value = "";
      fade();
      return;
    }
    // The host raises this window before it says what goes in it, and the
    // rescue below puts the caret in the field the moment it arrives. So the
    // field holding the keyboard is not evidence of a person typing: only a
    // window that was already up can have anything of theirs in it.
    const arriving = !showing;
    box.dataset["state"] = presentation.state;
    hint.textContent = line;
    words.readOnly = !presentation.editable;
    // What is in the field belongs to whoever may edit it. Once the box is up
    // and the words are theirs, the host does not write over them.
    if (arriving || !presentation.editable) {
      words.value = presentation.text;
    }
    if (arriving || !holding()) {
      words.focus();
      words.setSelectionRange(words.value.length, words.value.length);
    }
    fade();
    // The window is raised for this presentation and only this one, so the
    // entrance runs once per arrival rather than on every re-render.
    if (arriving) pop();
    showing = true;
  });

  void listen("scufris://copy", (event) => {
    // Copying is the safe choice offered for a transcript whose outcome nobody
    // knows, so a clipboard that refuses must not look like anything happened.
    navigator.clipboard?.writeText(event.payload as string).catch(() => {});
  });

  // A transcript recovered from a previous process is published while this page
  // is still loading, so the current presentation is asked for rather than
  // waited for: an empty box over words nobody can read is worse than no box.
  void invoke("textbox_ready");
}
