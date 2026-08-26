// The two desktop webview pages, run headlessly against a stub DOM.
//
// The pages are compiled by build.rs and loaded by windows that need an X
// display, a compositor-less desktop and a microphone, so nothing about them is
// exercised by the Rust tests. What can be exercised is what they compute: the
// caret and the selection are blocks laid under one unbroken run of words, the
// editing keys reach the mirror, and every state paints an orb at the size the
// frame is built for.
//
// The stub is a fake, and it says so: it lays characters out on a fixed grid
// rather than shaping type. That is enough for the invariants here, all of
// which are about which element holds what and where a block is put, and none
// of which are about the shape of a letter.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import test from "node:test";
import { createContext, runInContext } from "node:vm";

const root = resolve(new URL("..", import.meta.url).pathname);
const ui = join(root, "desktop", "scufris-desktop", "ui");

/** The pages, compiled the way build.rs compiles them. */
function pages(): { pill: string; review: string; engine: string } {
  if (compiled === null) {
    const built = spawnSync(
      join(root, "node_modules", ".bin", "tsc"),
      ["-p", join(ui, "tsconfig.json")],
      { encoding: "utf8" },
    );
    assert.equal(
      built.status,
      0,
      `the pill frontend does not compile: ${built.stdout}${built.stderr}`,
    );
    compiled = {
      pill: readFileSync(join(ui, "dist", "pill.js"), "utf8"),
      review: readFileSync(join(ui, "dist", "review.js"), "utf8"),
      engine: readFileSync(join(ui, "orb-engine.js"), "utf8"),
    };
  }
  return compiled;
}

let compiled: { pill: string; review: string; engine: string } | null = null;

// ---------- the stub ----------

interface Rect {
  left: number;
  top: number;
  right: number;
  bottom: number;
  width: number;
  height: number;
}

/** One character of the fake type, and one line of it. */
const ADVANCE = 10.8;
const LINE = 26.1;
/** How many characters the fake box fits on a line before it wraps. */
const COLUMNS = 52;
/** Where the text area sits in the fake window. */
const FRAME = { left: 24, top: 17 };

function rect(left: number, top: number, width: number, height: number): Rect {
  return {
    left,
    top,
    right: left + width,
    bottom: top + height,
    width,
    height,
  };
}

type Listener = (event: Record<string, unknown>) => void;

class Stub {
  id = "";
  className = "";
  hidden = false;
  readOnly = false;
  value = "";
  width = 0;
  height = 0;
  offsetWidth = 0;
  scrollTop = 0;
  scrollLeft = 0;
  scrollHeight = 0;
  clientHeight = 100;
  selectionStart = 0;
  selectionEnd = 0;
  selectionDirection = "none";
  content = "";
  rect: Rect = rect(0, 0, 0, 0);
  readonly style: Record<string, unknown> = {
    setProperty: (name: string, value: string): void => {
      this.style[name] = value;
    },
  };
  readonly dataset: Record<string, string> = {};
  readonly children: Stub[] = [];
  readonly classes = new Set<string>();
  readonly listeners = new Map<string, Listener[]>();
  page: Page | null = null;

  get textContent(): string {
    return this.content;
  }

  set textContent(value: string) {
    this.content = value;
    this.children.length = 0;
  }

  get firstChild(): object | null {
    return this.content.length > 0 ? { text: this.content } : null;
  }

  readonly classList = {
    add: (name: string): void => {
      this.classes.add(name);
    },
    remove: (name: string): void => {
      this.classes.delete(name);
    },
    contains: (name: string): boolean => this.classes.has(name),
    toggle: (name: string, force: boolean): void => {
      if (force) this.classes.add(name);
      else this.classes.delete(name);
    },
  };

  appendChild(node: Stub): Stub {
    this.children.push(node);
    return node;
  }

  addEventListener(type: string, handler: Listener): void {
    const existing = this.listeners.get(type) ?? [];
    existing.push(handler);
    this.listeners.set(type, existing);
  }

  dispatch(type: string, event: Record<string, unknown>): void {
    for (const handler of this.listeners.get(type) ?? []) handler(event);
  }

  getBoundingClientRect(): Rect {
    return this.rect;
  }

  scrollIntoView(): void {}

  focus(): void {
    if (this.page !== null) this.page.activeElement = this;
  }

  blur(): void {
    if (this.page !== null && this.page.activeElement === this) {
      this.page.activeElement = null;
    }
  }

  setSelectionRange(start: number, end: number): void {
    this.selectionStart = start;
    this.selectionEnd = end;
  }

  setRangeText(
    replacement: string,
    start: number,
    end: number,
    mode: string,
  ): void {
    this.value =
      this.value.slice(0, start) + replacement + this.value.slice(end);
    const caret = mode === "end" ? start + replacement.length : start;
    this.selectionStart = caret;
    this.selectionEnd = caret;
  }

  getContext(): Painter {
    if (this.page === null) throw new Error("the canvas has no page");
    return this.page.painter;
  }
}

/** Every arc the orb painter drew, in the canvas's own pixels. */
interface Painter {
  fillStyle: string;
  strokeStyle: string;
  lineWidth: number;
  arcs: { x: number; y: number; r: number }[];
  scale(x: number, y: number): void;
  clearRect(): void;
  beginPath(): void;
  moveTo(): void;
  lineTo(): void;
  stroke(): void;
  fill(): void;
  arc(x: number, y: number, r: number): void;
}

function painter(): Painter {
  const arcs: { x: number; y: number; r: number }[] = [];
  return {
    fillStyle: "",
    strokeStyle: "",
    lineWidth: 0,
    arcs,
    scale: () => {},
    clearRect: () => {},
    beginPath: () => {},
    moveTo: () => {},
    lineTo: () => {},
    stroke: () => {},
    fill: () => {},
    arc: (x: number, y: number, r: number) => {
      arcs.push({ x, y, r });
    },
  };
}

interface Emission {
  target: string;
  event: string;
  payload: Record<string, unknown>;
}

class Page {
  readonly elements = new Map<string, Stub>();
  readonly emissions: Emission[] = [];
  readonly invocations: { command: string; args: unknown }[] = [];
  readonly handlers = new Map<string, Listener[]>();
  readonly frames: ((now: number) => void)[] = [];
  readonly painter = painter();
  readonly window = new Stub();
  readonly document = new Stub();
  activeElement: Stub | null = null;
  reducedMotion = false;
  accent = "#95a99f";
  /** What `document.execCommand` does, which is a port's choice. */
  execCommand: ((command: string) => boolean) | null = null;

  element(id: string): Stub {
    const found = this.elements.get(id);
    if (found === undefined) throw new Error(`the stub page has no #${id}`);
    return found;
  }

  add(id: string): Stub {
    const node = new Stub();
    node.id = id;
    node.page = this;
    this.elements.set(id, node);
    return node;
  }

  /** Hands one event to the page, the way the host emits it. */
  publish(event: string, payload: unknown): void {
    for (const handler of this.handlers.get(event) ?? []) handler({ payload });
  }

  drafts(): Record<string, unknown>[] {
    return this.emissions
      .filter((emission) => emission.event === "scufris://draft")
      .map((emission) => emission.payload);
  }

  lastDraft(): Record<string, unknown> {
    const drafts = this.drafts();
    const last = drafts[drafts.length - 1];
    assert.ok(last !== undefined, "nothing was mirrored");
    return last;
  }
}

/** The characters `from` to `to` as the fake type would have drawn them. */
// The visual scale the measured rectangles report, as during the box's pop-in
// transform: 1 outside the one test that sets it.
let zoom = 1;

function drawn(from: number, to: number): Rect[] {
  const rects: Rect[] = [];
  let index = from;
  while (index < to) {
    const line = Math.floor(index / COLUMNS);
    const stop = Math.min(to, (line + 1) * COLUMNS);
    rects.push(
      rect(
        FRAME.left + zoom * (index % COLUMNS) * ADVANCE,
        FRAME.top + zoom * line * LINE,
        zoom * (stop - index) * ADVANCE,
        zoom * LINE,
      ),
    );
    index = stop;
  }
  return rects;
}

function run(page: Page, ids: string[], scripts: string[]): void {
  for (const id of ids) page.add(id);
  const global: Record<string, unknown> = {
    console: {
      debug: () => {},
      log: () => {},
      info: () => {},
      warn: () => {},
      error: () => {},
    },
    navigator: {},
    document: Object.assign(page.document, {
      getElementById: (id: string): Stub | null =>
        page.elements.get(id) ?? null,
      createElement: (): Stub => {
        const node = new Stub();
        node.page = page;
        return node;
      },
      createRange: () => {
        let from = 0;
        let to = 0;
        return {
          setStart: (_node: unknown, offset: number): void => {
            from = offset;
          },
          setEnd: (_node: unknown, offset: number): void => {
            to = offset;
          },
          getClientRects: (): Rect[] => drawn(from, to),
        };
      },
      execCommand: (command: string): boolean =>
        page.execCommand === null ? false : page.execCommand(command),
      hidden: false,
    }),
    matchMedia: () => ({
      get matches(): boolean {
        return page.reducedMotion;
      },
      addEventListener: () => {},
    }),
    getComputedStyle: () => ({
      getPropertyValue: (): string => page.accent,
    }),
    requestAnimationFrame: (callback: (now: number) => void): number => {
      page.frames.push(callback);
      return page.frames.length;
    },
    cancelAnimationFrame: () => {},
    AudioContext: class {
      state = "running";
      currentTime = 0;
      destination = {};
      createOscillator = () => ({
        type: "",
        frequency: {
          setValueAtTime: () => {},
          exponentialRampToValueAtTime: () => {},
        },
        connect: () => ({ connect: () => {} }),
        start: () => {},
        stop: () => {},
      });
      createGain = () => ({
        gain: {
          setValueAtTime: () => {},
          exponentialRampToValueAtTime: () => {},
        },
        connect: () => ({ connect: () => {} }),
      });
      resume = (): Promise<void> => Promise.resolve();
    },
  };
  // Which element holds the focus is read while the page runs, so the document
  // has to answer from the page and not from a copy of it: `Object.assign`
  // reads a getter once and keeps the one answer, which for a page that has
  // not started is that nothing is focused, forever.
  Object.defineProperty(page.document, "activeElement", {
    get: (): Stub | null => page.activeElement,
  });
  const context = createContext(global);
  const win = Object.assign(page.window, {
    __TAURI__: {
      core: {
        invoke: (command: string, args: unknown): Promise<unknown> => {
          page.invocations.push({ command, args });
          return Promise.resolve(command === "pill_cues" ? true : undefined);
        },
      },
      event: {
        listen: (event: string, handler: Listener): Promise<unknown> => {
          const existing = page.handlers.get(event) ?? [];
          existing.push(handler);
          page.handlers.set(event, existing);
          return Promise.resolve(undefined);
        },
        emitTo: (
          target: string,
          event: string,
          payload: Record<string, unknown>,
        ): Promise<void> => {
          page.emissions.push({ target, event, payload });
          return Promise.resolve();
        },
      },
    },
    devicePixelRatio: 1,
  });
  global["window"] = win;
  for (const script of scripts) runInContext(script, context);
}

/** The pill page, loaded with its engine and its four elements. */
function pill(still = false): Page {
  const page = new Page();
  page.reducedMotion = still;
  const { pill: script, engine } = pages();
  run(page, ["pill", "transcript", "timer", "orb"], [engine, script]);
  return page;
}

/** The review page, loaded with the text area sized like the real one. */
function review(): Page {
  const page = new Page();
  run(
    page,
    ["box", "text", "words", "marks", "probe", "hint"],
    [pages().review],
  );
  const text = page.element("text");
  text.rect = rect(FRAME.left, FRAME.top, COLUMNS * ADVANCE, 3 * LINE);
  text.offsetWidth = COLUMNS * ADVANCE;
  text.clientHeight = 3 * LINE;
  page.element("probe").rect = rect(FRAME.left, FRAME.top, ADVANCE, LINE);
  return page;
}

function present(
  page: Page,
  state: string,
  text: string,
  editable: boolean,
): void {
  page.publish("scufris://presentation", {
    state,
    detail: "",
    text,
    editable,
    recording: false,
  });
}

function marks(page: Page, kind: string): Stub[] {
  return page
    .element("marks")
    .children.filter((child) => child.className === kind);
}

function column(node: Stub): number {
  return Number.parseFloat(String(node.style["left"]));
}

// ---------- the caret ----------

test("the caret is a block under the letter, and the words are one run", () => {
  const page = review();
  const words = "hello brave world";
  present(page, "review", words, true);
  assert.equal(page.element("words").textContent, words);

  for (let at = 0; at <= words.length; at += 1) {
    page.publish("scufris://draft", {
      text: words,
      start: at,
      end: at,
      caret: at,
    });
    // The words never move, and never split: the run holds all of them, in
    // order, whatever the caret is doing.
    assert.equal(page.element("words").textContent, words);
    assert.equal(page.element("words").children.length, 0);
    const carets = marks(page, "caret");
    assert.equal(carets.length, 1, `no caret at ${at}`);
    const caret = carets[0];
    assert.ok(caret !== undefined);
    assert.ok(
      Math.abs(column(caret) - at * ADVANCE) < 0.001,
      `the caret at ${at} is drawn at ${column(caret)}`,
    );
    // A block the width of a letter, not a rule between two of them.
    assert.equal(caret.style["width"], `${ADVANCE}px`);
    assert.equal(caret.style["top"], "0px");
  }
});

test("a draft measured mid-pop still lands the marks on the letters", () => {
  // While the box pops in, every measured rectangle is scaled by the
  // transform; the marks are laid in the box's own coordinates, so an
  // unscaled reading left the caret short of its letter until the next key
  // redrew it. The regression: present during the pop and the caret must
  // already sit at the end of the words.
  const page = review();
  const s = 0.94;
  const text = page.element("text");
  text.rect = rect(FRAME.left, FRAME.top, s * COLUMNS * ADVANCE, s * 3 * LINE);
  page.element("probe").rect = rect(
    FRAME.left,
    FRAME.top,
    s * ADVANCE,
    s * LINE,
  );
  zoom = s;
  try {
    const words = "hello brave world";
    present(page, "review", words, true);
    const caret = marks(page, "caret")[0];
    assert.ok(caret !== undefined);
    assert.ok(
      Math.abs(column(caret) - words.length * ADVANCE) < 0.001,
      `the caret is drawn at ${column(caret)}, not ${words.length * ADVANCE}`,
    );
    const width = Number.parseFloat(String(caret.style["width"]));
    assert.ok(Math.abs(width - ADVANCE) < 0.001);
    assert.equal(caret.style["top"], "0px");
  } finally {
    zoom = 1;
  }
});

test("a selection is drawn as bands under the words, one for each line", () => {
  const page = review();
  const words = "a".repeat(COLUMNS + 20);
  present(page, "review", words, true);
  page.publish("scufris://draft", {
    text: words,
    start: 4,
    end: COLUMNS + 8,
    caret: COLUMNS + 8,
  });
  assert.equal(page.element("words").textContent, words);
  const bands = marks(page, "pick");
  assert.equal(bands.length, 2, "a selection across two lines drew one band");
  const first = bands[0];
  const second = bands[1];
  assert.ok(first !== undefined && second !== undefined);
  assert.ok(Math.abs(column(first) - 4 * ADVANCE) < 0.001);
  assert.equal(first.style["top"], "0px");
  assert.equal(second.style["top"], `${LINE}px`);
  assert.equal(second.style["width"], `${8 * ADVANCE}px`);
  // The caret rides the end the person is dragging, on top of the band.
  const caret = marks(page, "caret")[0];
  assert.ok(caret !== undefined);
  assert.ok(Math.abs(column(caret) - 8 * ADVANCE) < 0.001);
});

test("a frozen transcript is drawn with no caret and no selection", () => {
  const page = review();
  present(page, "uncertain", "the words nobody may edit", false);
  assert.equal(page.element("words").textContent, "the words nobody may edit");
  assert.equal(page.element("marks").children.length, 0);
  // The draft never arrives for a frozen field, but a stray one changes
  // nothing either.
  page.publish("scufris://draft", {
    text: "the words nobody may edit",
    start: 0,
    end: 4,
    caret: 4,
  });
  assert.equal(page.element("marks").children.length, 0);
});

test("an empty transcript still carries a caret, one letter wide", () => {
  const page = review();
  present(page, "review", "", true);
  const caret = marks(page, "caret")[0];
  assert.ok(caret !== undefined);
  assert.equal(caret.style["left"], "0px");
  assert.equal(caret.style["width"], `${ADVANCE}px`);
  assert.equal(caret.style["height"], `${LINE}px`);
});

// ---------- the keys ----------

function editing(page: Page, words: string): Stub {
  present(page, "review", words, true);
  const transcript = page.element("transcript");
  transcript.selectionStart = words.length;
  transcript.selectionEnd = words.length;
  return transcript;
}

function press(page: Page, key: string, control: boolean): void {
  page.window.dispatch("keydown", {
    key,
    ctrlKey: control,
    metaKey: false,
    altKey: false,
    preventDefault: () => {},
  });
}

test("a click on the pill leaves the field the focus the keys need", () => {
  // A click is how a person brings this window back, and a click's default is
  // to move the focus to whatever lies under it. Enter and Escape are read
  // from the window and would live through that; the arrows, Backspace, and
  // every letter are read from the field, so a review recovered by a click
  // could only be sent or thrown away, never edited.
  const page = pill();
  editing(page, "hello world");
  let refused = 0;
  const click = {
    preventDefault: (): void => {
      refused += 1;
    },
  };

  page.document.dispatch("mousedown", click);
  assert.equal(refused, 1);

  // Nothing is being edited, so the click is the person's to spend as they like.
  present(page, "listening", "", false);
  page.document.dispatch("mousedown", click);
  assert.equal(refused, 1);
});

test("a window that gets the keyboard back takes the field again", () => {
  // The desktop can move the focus off the field on its own, and the window
  // comes back with the keyboard and nothing to type into.
  const page = pill();
  const transcript = editing(page, "hello world");
  transcript.selectionStart = 5;
  transcript.selectionEnd = 5;
  transcript.blur();

  page.window.dispatch("focus", {});

  assert.equal(page.activeElement, transcript);
  assert.equal(transcript.selectionStart, 5);
  press(page, "Backspace", true);
  assert.equal(transcript.value, " world");
});

test("ctrl-backspace deletes a word and mirrors what is left", () => {
  const page = pill();
  const transcript = editing(page, "hello brave world");
  press(page, "Backspace", true);
  // The word and the space that joined it to the one before: two spaces where
  // a word used to be is not what deleting a word means.
  assert.equal(transcript.value, "hello brave");
  const draft = page.lastDraft();
  assert.equal(draft["text"], "hello brave");
  assert.equal(draft["caret"], 11);
  assert.equal(draft["start"], 11);
  assert.equal(draft["end"], 11);

  press(page, "Backspace", true);
  assert.equal(transcript.value, "hello");
  assert.equal(page.lastDraft()["text"], "hello");

  // The last word leaves nothing behind it, and the field is then empty.
  press(page, "Backspace", true);
  assert.equal(transcript.value, "");
  assert.equal(page.lastDraft()["caret"], 0);
});

test("the port's own delete is used when it has one, with the same result", () => {
  const page = pill();
  const transcript = editing(page, "hello brave world");
  page.execCommand = (command: string): boolean => {
    if (command !== "delete") return false;
    transcript.value =
      transcript.value.slice(0, transcript.selectionStart) +
      transcript.value.slice(transcript.selectionEnd);
    transcript.selectionEnd = transcript.selectionStart;
    return true;
  };
  press(page, "Backspace", true);
  assert.equal(transcript.value, "hello brave");
  assert.equal(page.lastDraft()["text"], "hello brave");
});

test("ctrl-delete, ctrl-u and ctrl-k cut forward, to the start and to the end", () => {
  const forward = pill();
  const words = forward.element("transcript");
  editing(forward, "hello brave world");
  words.selectionStart = 6;
  words.selectionEnd = 6;
  press(forward, "Delete", true);
  assert.equal(words.value, "hello world");
  assert.equal(forward.lastDraft()["text"], "hello world");

  const start = pill();
  const line = editing(start, "hello brave world");
  line.selectionStart = 12;
  line.selectionEnd = 12;
  press(start, "u", true);
  assert.equal(line.value, "world");
  assert.equal(start.lastDraft()["caret"], 0);

  const end = pill();
  const rest = editing(end, "hello brave world");
  rest.selectionStart = 6;
  rest.selectionEnd = 6;
  press(end, "k", true);
  assert.equal(rest.value, "hello ");
  assert.equal(end.lastDraft()["text"], "hello ");
});

test("the field keeps the keys it already carries, selections included", () => {
  const page = pill();
  const transcript = editing(page, "hello brave world");
  // A selection is a range the field deletes itself: nothing here may turn
  // one backspace into a word.
  transcript.selectionStart = 6;
  transcript.selectionEnd = 11;
  press(page, "Backspace", true);
  assert.equal(transcript.value, "hello brave world");
  // A plain backspace is the field's too.
  transcript.selectionStart = 17;
  transcript.selectionEnd = 17;
  press(page, "Backspace", false);
  assert.equal(transcript.value, "hello brave world");
});

test("a word jump changes no words and still reaches the mirror", () => {
  const page = pill();
  const transcript = editing(page, "hello brave world");
  const before = page.drafts().length;
  // What Ctrl+Left does in the field itself: the caret moves, nothing else.
  transcript.selectionStart = 6;
  transcript.selectionEnd = 6;
  transcript.dispatch("keyup", {});
  assert.equal(page.drafts().length, before + 1);
  const draft = page.lastDraft();
  assert.equal(draft["text"], "hello brave world");
  assert.equal(draft["caret"], 6);
});

test("nothing is edited where nothing is editable", () => {
  const page = pill();
  present(page, "uncertain", "the words nobody may edit", false);
  const transcript = page.element("transcript");
  transcript.selectionStart = transcript.value.length;
  transcript.selectionEnd = transcript.value.length;
  const before = page.drafts().length;
  press(page, "Backspace", true);
  assert.equal(transcript.value, "the words nobody may edit");
  assert.equal(page.drafts().length, before, "a frozen field was mirrored");
});

// ---------- the orb ----------

/** Paints the newest frame, which is where a moving pill draws its orb. */
function paint(page: Page, still: boolean): void {
  if (still) return;
  const frame = page.frames[page.frames.length - 1];
  assert.ok(frame !== undefined, "the pill asked for no frame");
  frame(1000);
}

const STATES = [
  "idle",
  "listening",
  "transcribing",
  "review",
  "sent",
  "retained",
  "uncertain",
  "working",
  "speaking",
  "attention",
  "error",
  "disconnected",
];

test("every state paints an orb at the size the frame is built for", () => {
  for (const still of [true, false]) {
    for (const state of STATES) {
      const page = pill(still);
      // A state is settled when it is arrived at, so every state is arrived at
      // from another one.
      present(page, state === "listening" ? "idle" : "listening", "", false);
      paint(page, still);
      page.painter.arcs.length = 0;
      present(page, state, "", state === "review");
      paint(page, still);
      const arcs = page.painter.arcs;
      assert.ok(arcs.length > 20, `${state} painted ${arcs.length} dots`);
      let spread = 0;
      for (const arc of arcs) {
        assert.ok(
          arc.x >= 0 && arc.x <= 160 && arc.y >= 0 && arc.y <= 160,
          `${state} painted a dot at ${arc.x},${arc.y}`,
        );
        spread = Math.max(spread, Math.abs(arc.x - 80), Math.abs(arc.y - 80));
      }
      // Drawn at 160, not at the 64 the preset is tuned at: an orb inside the
      // old square would never reach past 32 from the middle.
      assert.ok(spread > 40, `${state} painted an orb only ${spread} across`);
    }
  }
});
