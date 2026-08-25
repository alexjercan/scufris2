// The transcript box that rises above the orb, from the Orb Study section 03
// (tasks/20260825-231826/orb-study.html). It renders the same presentation the
// orb window renders - the host broadcasts it to both - plus the live draft the
// orb window mirrors while the person types.
//
// It decides nothing and it takes no keys. Enter, Escape and Ctrl+C all belong
// to the orb window, which is the one holding the field these words come from.
// The host owns whether this window is up at all.
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
      invoke("pill_log", { level: `review.${level}`, message }).catch(() => {});
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
    if (found === null) throw new Error(`the review page is missing #${id}`);
    return found as T;
  };

  const box = element<HTMLElement>("box");
  const text = element<HTMLElement>("text");
  const before = element<HTMLElement>("before");
  const after = element<HTMLElement>("after");
  const caret = element<HTMLElement>("caret");
  const hint = element<HTMLElement>("hint");

  const reducedMotion = matchMedia("(prefers-reduced-motion: reduce)");

  // The states this box exists for, and the one line each of them says. The
  // host holds the same two in `review.rs`, as the states it raises the window
  // for; a state with no hint here is a state the window is down in.
  const HINTS: Record<string, string> = {
    review: "enter sends - esc discards",
    uncertain: "the daemon is unsure - enter again forces it",
  };

  let showing = false;
  let editable = false;

  // The caret is drawn where the person's own caret is, so the words either
  // side of it are two runs rather than one.
  const draw = (words: string, at: number): void => {
    const split = Math.max(0, Math.min(at, words.length));
    before.textContent = words.slice(0, split);
    after.textContent = words.slice(split);
    caret.hidden = !editable;
    // A take longer than the box gets a soft edge instead of a hard cut.
    text.classList.toggle("overflowing", text.scrollHeight > text.clientHeight);
    if (editable) caret.scrollIntoView({ block: "nearest" });
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

  void listen("scufris://presentation", (event) => {
    const presentation = event.payload as Presentation;
    const line = HINTS[presentation.state];
    if (line === undefined) {
      // The host is taking this window down. Emptying it now means the next
      // transcript never appears behind a flash of the last one.
      showing = false;
      editable = false;
      draw("", 0);
      return;
    }
    box.dataset["state"] = presentation.state;
    hint.textContent = line;
    editable = presentation.editable;
    draw(presentation.text, presentation.text.length);
    // The window is raised for this presentation and only this one, so the
    // entrance runs once per arrival rather than on every re-render.
    if (!showing) pop();
    showing = true;
  });

  void listen("scufris://draft", (event) => {
    if (!showing) return;
    const draft = event.payload as Draft;
    draw(draft.text, draft.caret);
  });

  // A transcript recovered from a previous process is published while this page
  // is still loading, so the current presentation is asked for rather than
  // waited for: an empty box over words nobody can read is worse than no box.
  void invoke("review_ready");
}
