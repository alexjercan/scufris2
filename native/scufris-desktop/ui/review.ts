// The transcript box that rises above the orb, from the Orb Study section 03
// (tasks/20260825-231826/orb-study.html). It renders the same presentation the
// orb window renders - the host broadcasts it to both - plus the live draft the
// orb window mirrors while the person types.
//
// It decides nothing and it takes no keys. Enter, Escape and Ctrl+C all belong
// to the orb window, which is the one holding the field these words come from.
// The host owns whether this window is up at all.
//
// The words are one run, and the caret and the selection are blocks laid under
// them: the browser is asked where a letter is drawn, and a block is put there.
// Nothing about the caret is part of the line, so nothing about the line moves
// when the caret does.
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
  const words = element<HTMLElement>("words");
  const marks = element<HTMLElement>("marks");
  const probe = element<HTMLElement>("probe");
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

  /** Where one mark goes, in the text area's own coordinates. */
  interface Mark {
    left: number;
    top: number;
    width: number;
    height: number;
  }

  const within = (at: number, length: number): number =>
    Math.max(0, Math.min(at, length));

  // A mark is placed against the text area, not the window, and against its
  // scrolled contents, not what is on screen: a long take scrolls, and a caret
  // measured on screen would then be drawn a line away from its letter.
  //
  // Measured rectangles are also visual: while the box pops in it is scaled by
  // a transform, and the orb window mirrors a draft as soon as it renders, so
  // the first drafts of a review are measured mid-pop. A mark laid from those
  // unscaled distances sticks where the mid-pop frame put it - the offset
  // caret that snapped right on the first key. Dividing by the scale lays the
  // mark in the box's own coordinates, which ride the transform instead.
  const place = (rect: DOMRect, frame: DOMRect, scale: number): Mark => ({
    left: (rect.left - frame.left) / scale + text.scrollLeft,
    top: (rect.top - frame.top) / scale + text.scrollTop,
    width: rect.width / scale,
    height: rect.height / scale,
  });

  // Where the browser actually drew a stretch of the words: one rectangle per
  // line it fell across. Measuring beats arithmetic here - the type is
  // monospace by preference and not by guarantee, and the box wraps.
  const drawn = (from: number, to: number): DOMRect[] => {
    const node = words.firstChild;
    if (node === null || from >= to) return [];
    const range = document.createRange();
    range.setStart(node, from);
    range.setEnd(node, to);
    return Array.from(range.getClientRects());
  };

  // The block the caret is: the box of the letter it sits on, so it lies under
  // that letter rather than between two of them. Past the last letter there is
  // no box to take, so the last one's right edge carries a letter's width; on
  // an empty line there is neither, and the probe is what a letter would be.
  const caretMark = (
    value: string,
    at: number,
    frame: DOMRect,
    scale: number,
  ): Mark => {
    const index = within(at, value.length);
    const advance = probe.getBoundingClientRect();
    if (index < value.length) {
      const rect = drawn(index, index + 1)[0];
      if (rect !== undefined) return place(rect, frame, scale);
    }
    if (value.length > 0) {
      const rects = drawn(value.length - 1, value.length);
      const rect = rects[rects.length - 1];
      if (rect !== undefined) {
        const mark = place(rect, frame, scale);
        return {
          left: mark.left + mark.width,
          top: mark.top,
          width: advance.width / scale || mark.width,
          height: mark.height,
        };
      }
    }
    return {
      left: 0,
      top: 0,
      width: advance.width / scale,
      height: advance.height / scale,
    };
  };

  const block = (kind: string, mark: Mark): HTMLElement => {
    const node = document.createElement("span");
    node.className = kind;
    node.style.left = `${mark.left}px`;
    node.style.top = `${mark.top}px`;
    node.style.width = `${mark.width}px`;
    node.style.height = `${mark.height}px`;
    return node;
  };

  // The words, then everything that is not the words. The run is replaced whole
  // and never split, so a caret that moves changes nothing about the line it is
  // in; the marks are thrown away and laid again, which is the cheap half.
  const draw = (
    value: string,
    start: number,
    end: number,
    focus: number,
  ): void => {
    words.textContent = value;
    marks.textContent = "";
    // Measured with no marks in the way: they are out of flow, but they are
    // still inside the box being asked how tall its contents are.
    text.classList.toggle("overflowing", text.scrollHeight > text.clientHeight);
    if (!editable) return;
    const frame = text.getBoundingClientRect();
    // The pop scales uniformly, so one ratio of visual width to layout width
    // undoes it; a page that measures zero is hidden, and nothing is scaled.
    const scale =
      frame.width > 0 && text.offsetWidth > 0
        ? frame.width / text.offsetWidth
        : 1;
    const from = within(Math.min(start, end), value.length);
    const to = within(Math.max(start, end), value.length);
    for (const rect of drawn(from, to)) {
      marks.appendChild(block("pick", place(rect, frame, scale)));
    }
    const caret = marks.appendChild(
      block("caret", caretMark(value, focus, frame, scale)),
    );
    caret.scrollIntoView({ block: "nearest" });
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
      draw("", 0, 0, 0);
      return;
    }
    box.dataset["state"] = presentation.state;
    hint.textContent = line;
    editable = presentation.editable;
    const end = presentation.text.length;
    draw(presentation.text, end, end, end);
    // The window is raised for this presentation and only this one, so the
    // entrance runs once per arrival rather than on every re-render.
    if (!showing) pop();
    showing = true;
  });

  void listen("scufris://draft", (event) => {
    if (!showing) return;
    const draft = event.payload as Draft;
    draw(draft.text, draft.start, draft.end, draft.caret);
  });

  // A transcript recovered from a previous process is published while this page
  // is still loading, so the current presentation is asked for rather than
  // waited for: an empty box over words nobody can read is worse than no box.
  void invoke("review_ready");
}
