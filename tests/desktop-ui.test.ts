// The two desktop webview pages, run headlessly against a stub DOM.
//
// The pages are compiled by build.rs and loaded by windows that need an X
// display, a compositor-less desktop and a microphone, so nothing about them is
// exercised by the Rust tests. What can be exercised is what they compute: the
// textbox reports which key was pressed and what its field holds, the deletions
// it binds itself cut the right words, and every state paints an orb at the
// size the frame is built for.
//
// The stub is a fake, and it says so. That is enough for the invariants here,
// all of which are about which element holds what and what the page asks the
// host to do, and none of which are about the shape of a letter.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import test from "node:test";
import { createContext, runInContext } from "node:vm";

const root = resolve(new URL("..", import.meta.url).pathname);
const ui = join(root, "native", "scufris-desktop", "ui");

/** The pages, compiled the way build.rs compiles them. */
function pages(): { pill: string; textbox: string; engine: string } {
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
      textbox: readFileSync(join(ui, "dist", "textbox.js"), "utf8"),
      engine: readFileSync(join(ui, "orb-engine.js"), "utf8"),
    };
  }
  return compiled;
}

let compiled: { pill: string; textbox: string; engine: string } | null = null;

// ---------- the stub ----------

interface Rect {
  left: number;
  top: number;
  right: number;
  bottom: number;
  width: number;
  height: number;
}

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

class Page {
  readonly elements = new Map<string, Stub>();
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

  /** Every command the page asked the host to run, in order. */
  commands(): string[] {
    return this.invocations.map((invocation) => invocation.command);
  }

  /** The arguments of the last call to one command. */
  lastCall(command: string): Record<string, unknown> {
    const calls = this.invocations.filter(
      (invocation) => invocation.command === command,
    );
    const last = calls[calls.length - 1];
    assert.ok(last !== undefined, `the page never invoked ${command}`);
    return (last.args ?? {}) as Record<string, unknown>;
  }
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
      },
    },
    devicePixelRatio: 1,
  });
  global["window"] = win;
  for (const script of scripts) runInContext(script, context);
}

/** The pill page, loaded with its engine and its three elements. */
function pill(still = false): Page {
  const page = new Page();
  page.reducedMotion = still;
  const { pill: script, engine } = pages();
  run(page, ["pill", "timer", "orb"], [engine, script]);
  return page;
}

/** The textbox page, loaded with its field and its hint. */
function textbox(): Page {
  const page = new Page();
  run(page, ["box", "words", "hint"], [pages().textbox]);
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

// ---------- the field ----------

test("the words arrive in the field, with the line that says what the keys do", () => {
  const page = textbox();
  present(page, "editing", "hello brave world", true);
  const words = page.element("words");
  assert.equal(words.value, "hello brave world");
  assert.equal(words.readOnly, false);
  assert.equal(page.element("box").dataset["state"], "editing");
  assert.equal(page.element("hint").textContent, "enter sends - esc discards");
  // The caret is left where a person would carry on typing.
  assert.equal(words.selectionStart, "hello brave world".length);
  assert.equal(page.activeElement, words);
});

test("a transcript nobody may edit is shown and frozen", () => {
  const page = textbox();
  present(page, "uncertain", "the words nobody may edit", false);
  const words = page.element("words");
  assert.equal(words.value, "the words nobody may edit");
  assert.equal(words.readOnly, true);
  assert.match(String(page.element("hint").textContent), /unsure/);
});

test("a state this window is not for empties it", () => {
  // The host takes the box down for these, and a box that kept the last
  // transcript would show it again for a moment on the next raise.
  const page = textbox();
  present(page, "editing", "hello brave world", true);
  present(page, "listening", "", false);
  const words = page.element("words");
  assert.equal(words.value, "");
  assert.equal(words.readOnly, true);
});

test("a take longer than the box fades under its own edge", () => {
  const page = textbox();
  const words = page.element("words");
  words.clientHeight = 3 * 26;
  words.scrollHeight = 9 * 26;
  present(page, "editing", "a".repeat(400), true);
  assert.ok(words.classes.has("overflowing"), "a long take was not faded");

  words.scrollHeight = 26;
  words.dispatch("input", {});
  assert.ok(!words.classes.has("overflowing"));
});

test("what the person is typing is not overwritten by its own presentation", () => {
  const page = textbox();
  const words = page.element("words");
  present(page, "editing", "hello brave world", true);
  words.value = "hello brave worlds";
  // The same phase renders again - a failed save adds a notice to it - and the
  // words on screen are the person's, not the host's older copy.
  present(page, "editing", "hello brave world", true);
  assert.equal(words.value, "hello brave worlds");
});

test("the words arrive in a field that already holds the keyboard", () => {
  // The host gives this window the keyboard as it raises it, and the rescue
  // below puts the caret in the field before the transcript is published. A
  // page that read that as a person typing showed an empty box over a take
  // nobody could read.
  const page = textbox();
  const words = page.element("words");
  page.window.dispatch("focus", {});
  assert.equal(page.activeElement, words);
  present(page, "editing", "hello brave world", true);
  assert.equal(words.value, "hello brave world");
});

// ---------- the keys ----------

function editing(page: Page, words: string): Stub {
  present(page, "editing", words, true);
  const field = page.element("words");
  field.selectionStart = words.length;
  field.selectionEnd = words.length;
  return field;
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

test("enter sends what the field holds and escape throws it away", () => {
  const page = textbox();
  const words = editing(page, "hello brave world");
  words.value = "hello brave worlds";
  press(page, "Enter", false);
  assert.equal(page.lastCall("textbox_submit")["text"], "hello brave worlds");

  press(page, "Escape", false);
  assert.ok(page.commands().includes("textbox_cancel"));
});

test("a frozen transcript sends no words of its own", () => {
  // Enter on an uncertain transcript is the person saying "send it anyway",
  // and what is sent is the text the host kept, never an edit this window
  // would not have taken.
  const page = textbox();
  present(page, "uncertain", "the words nobody may edit", false);
  press(page, "Enter", false);
  assert.equal(page.lastCall("textbox_submit")["text"], null);
});

test("ctrl-c asks the host to copy, and a selection keeps the ordinary copy", () => {
  const page = textbox();
  const words = editing(page, "hello brave world");
  press(page, "c", true);
  assert.ok(page.commands().includes("textbox_copy"));

  // Copying part of the words is the field's own job.
  const before = page.commands().length;
  words.selectionStart = 6;
  words.selectionEnd = 11;
  press(page, "c", true);
  assert.equal(page.commands().length, before);
});

test("a window that gets the keyboard back takes the field again", () => {
  // The desktop can move the focus off the field on its own, and the window
  // comes back with the keyboard and nothing to type into.
  const page = textbox();
  const words = editing(page, "hello world");
  words.selectionStart = 5;
  words.selectionEnd = 5;
  words.blur();

  page.window.dispatch("focus", {});

  assert.equal(page.activeElement, words);
  assert.equal(words.selectionStart, 5);
  press(page, "Backspace", true);
  assert.equal(words.value, " world");
});

test("ctrl-backspace deletes a word", () => {
  const page = textbox();
  const words = editing(page, "hello brave world");
  press(page, "Backspace", true);
  // The word and the space that joined it to the one before: two spaces where
  // a word used to be is not what deleting a word means.
  assert.equal(words.value, "hello brave");
  assert.equal(words.selectionStart, 11);
  assert.equal(words.selectionEnd, 11);

  press(page, "Backspace", true);
  assert.equal(words.value, "hello");

  // The last word leaves nothing behind it, and the field is then empty.
  press(page, "Backspace", true);
  assert.equal(words.value, "");
  assert.equal(words.selectionStart, 0);
});

test("the port's own delete is used when it has one, with the same result", () => {
  const page = textbox();
  const words = editing(page, "hello brave world");
  page.execCommand = (command: string): boolean => {
    if (command !== "delete") return false;
    words.value =
      words.value.slice(0, words.selectionStart) +
      words.value.slice(words.selectionEnd);
    words.selectionEnd = words.selectionStart;
    return true;
  };
  press(page, "Backspace", true);
  assert.equal(words.value, "hello brave");
});

test("ctrl-delete, ctrl-u and ctrl-k cut forward, to the start and to the end", () => {
  const forward = textbox();
  const words = editing(forward, "hello brave world");
  words.selectionStart = 6;
  words.selectionEnd = 6;
  press(forward, "Delete", true);
  assert.equal(words.value, "hello world");

  const start = textbox();
  const line = editing(start, "hello brave world");
  line.selectionStart = 12;
  line.selectionEnd = 12;
  press(start, "u", true);
  assert.equal(line.value, "world");
  assert.equal(line.selectionStart, 0);

  const end = textbox();
  const rest = editing(end, "hello brave world");
  rest.selectionStart = 6;
  rest.selectionEnd = 6;
  press(end, "k", true);
  assert.equal(rest.value, "hello ");
});

test("the field keeps the keys it already carries, selections included", () => {
  const page = textbox();
  const words = editing(page, "hello brave world");
  // A selection is a range the field deletes itself: nothing here may turn
  // one backspace into a word.
  words.selectionStart = 6;
  words.selectionEnd = 11;
  press(page, "Backspace", true);
  assert.equal(words.value, "hello brave world");
  // A plain backspace is the field's too.
  words.selectionStart = 17;
  words.selectionEnd = 17;
  press(page, "Backspace", false);
  assert.equal(words.value, "hello brave world");
});

test("nothing is edited where nothing is editable", () => {
  const page = textbox();
  present(page, "uncertain", "the words nobody may edit", false);
  const words = page.element("words");
  words.selectionStart = words.value.length;
  words.selectionEnd = words.value.length;
  press(page, "Backspace", true);
  assert.equal(words.value, "the words nobody may edit");
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
  "editing",
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
      present(page, state, "", state === "editing");
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
