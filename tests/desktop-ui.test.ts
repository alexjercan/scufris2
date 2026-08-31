// The four desktop webview pages, run headlessly against a stub DOM.
//
// The pages are compiled by build.rs and loaded by windows that need an X
// display, a compositor-less desktop and a microphone, so nothing about them is
// exercised by the Rust tests. What can be exercised is what they compute: the
// textbox reports which key was pressed and what its field holds, the deletions
// it binds itself cut the right words, every state paints an orb at the size
// the frame is built for, the conversation window decides when to follow the
// scroll and when a typed line has actually left the field, and the form box
// decides when to ask about a field, which candidate the keys are on, and what
// a taken one answers with.
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
const ui = join(root, "surfaces", "desktop", "ui");

interface Compiled {
  pill: string;
  textbox: string;
  hud: string;
  form: string;
  engine: string;
}

/** The pages, compiled the way build.rs compiles them. */
function pages(): Compiled {
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
      hud: readFileSync(join(ui, "dist", "hud.js"), "utf8"),
      form: readFileSync(join(ui, "dist", "form.js"), "utf8"),
      engine: readFileSync(join(ui, "orb-engine.js"), "utf8"),
    };
  }
  return compiled;
}

let compiled: Compiled | null = null;

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
  src = "";
  alt = "";
  loading = "";
  controls = false;
  preload = "";
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
  parent: Stub | null = null;

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
    node.parent = this;
    this.children.push(node);
    return node;
  }

  append(...nodes: Stub[]): void {
    for (const node of nodes) node.parent = this;
    this.children.push(...nodes);
  }

  replaceChildren(...nodes: Stub[]): void {
    for (const child of this.children) child.parent = null;
    this.children.length = 0;
    for (const node of nodes) node.parent = this;
    this.children.push(...nodes);
  }

  remove(): void {
    if (this.parent === null) return;
    const index = this.parent.children.indexOf(this);
    if (index >= 0) this.parent.children.splice(index, 1);
    this.parent = null;
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
  /** What the host answers each command with, for the pages that read one. */
  readonly answers: Record<string, unknown> = { pill_cues: true };
  /** What `document.execCommand` does, which is a port's choice. */
  execCommand: ((command: string) => boolean) | null = null;
  /** The timers the page is waiting on, so a debounce can be driven here. */
  readonly timers = new Map<number, () => void>();
  timer = 0;

  /** Runs every timer that is waiting, the way the clock would. */
  elapse(): void {
    const waiting = [...this.timers.values()];
    this.timers.clear();
    for (const fire of waiting) fire();
  }

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
          return Promise.resolve(page.answers[command]);
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
    // Driven from the test rather than by the clock: a debounce measured in
    // real milliseconds is a test that waits for one.
    setTimeout: (fire: () => void): number => {
      page.timer += 1;
      page.timers.set(page.timer, fire);
      return page.timer;
    },
    clearTimeout: (id: number): void => {
      page.timers.delete(id);
    },
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

/** One line of the conversation, as the service pushes it. */
function line(role: string, text: string): Record<string, string> {
  return { role, surface: "desktop", text };
}

/**
 * The conversation window, loaded with its list, its notice and its field.
 *
 * `taken` is what the host answers `hud_submit` with: true when the line was
 * accepted, false when one was already in flight.
 */
function hud(taken = true, lines: Record<string, string>[] = []): Page {
  const page = new Page();
  page.answers["hud_ready"] = {
    lines,
    notice: { sending: false, thinking: false, attachments: [], trouble: "" },
  };
  page.answers["hud_submit"] = taken;
  run(page, ["lines", "notice", "words", "selected", "attach"], [pages().hud]);
  return page;
}

/** The form box, loaded with its title and the block the fields go in. */
function form(): Page {
  const page = new Page();
  // What the host answers when nothing has been asked yet.
  page.answers["form_ready"] = null;
  run(page, ["title", "fields"], [pages().form]);
  return page;
}

/** One field of an ask, as src/form.rs serializes it for the page. */
interface FormField {
  name: string;
  label: string;
  value: string;
  lines: number;
  hint: string;
  suggest: boolean;
}

/** One field, filled out around what the test cares about. */
function asking(field: Partial<FormField> & { name: string }): FormField {
  return {
    label: field.name,
    value: "",
    lines: 1,
    hint: "",
    suggest: false,
    ...field,
  };
}

/** Puts one question on the box, the way the host pushes it. */
function put(page: Page, fields: FormField[], title = "Food"): void {
  page.publish("scufris://ask", { title, fields });
}

/** The textarea of one field, and the list under it when it has one. */
function box(page: Page, index: number): Stub {
  const block = page.element("fields").children[index];
  assert.ok(block !== undefined, `the form drew no field ${String(index)}`);
  const input = block.children[1];
  assert.ok(input !== undefined, "a field with no input");
  return input;
}

function offered(page: Page, index: number): Stub | undefined {
  const block = page.element("fields").children[index];
  assert.ok(block !== undefined, `the form drew no field ${String(index)}`);
  return block.children[2];
}

/** One object the page built, lifted out of the context it was built in.
 *
 * A page runs in a vm of its own, so what it hands the host carries that vm's
 * prototype and never compares equal to a plain object written here. */
function said(value: unknown): Record<string, unknown> {
  return { ...(value as Record<string, unknown>) };
}

/** Types into a field and lets the debounce run out. */
function type(page: Page, index: number, text: string): void {
  const input = box(page, index);
  input.value = text;
  input.dispatch("input", {});
  page.elapse();
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

// ---------- the conversation window ----------

/** Presses one key on the conversation page, answering whether it was taken. */
function tap(page: Page, key: string, shiftKey = false): boolean {
  let prevented = false;
  page.window.dispatch("keydown", {
    key,
    shiftKey,
    preventDefault: () => {
      prevented = true;
    },
  });
  return prevented;
}

/** Waits for the promises the page started to settle. */
const settle = (): Promise<void> => new Promise((done) => setImmediate(done));

test("the conversation window draws every line it is handed, and who said it", async () => {
  const page = hud(true, [
    line("user", "what is on my calendar"),
    line("assistant", "nothing until three"),
  ]);
  await settle();
  const lines = page.element("lines");
  assert.equal(lines.children.length, 2);
  assert.deepEqual(
    lines.children.map((entry) => entry.dataset["speaker"]),
    ["user", "assistant"],
  );
  assert.deepEqual(
    lines.children.map((entry) => entry.children.map((part) => part.content)),
    [
      ["you", "what is on my calendar"],
      ["scufris", "nothing until three"],
    ],
  );

  // A speaker this build does not know is drawn rather than dropped: a line
  // that was said belongs on screen whoever the service says said it.
  page.publish("scufris://said", line("oracle", "mind the step"));
  const added = lines.children[2];
  assert.ok(added !== undefined);
  assert.deepEqual(
    added.children.map((part) => part.content),
    ["oracle", "mind the step"],
  );
});

test("managed attachments are selected, rendered inline, saved, and removed by id", async () => {
  const descriptor = {
    id: "att_one",
    name: "diagram.png",
    media_type: "image/png",
    size: 2048,
  };
  const page = hud();
  page.answers["hud_attach"] = {
    sending: false,
    thinking: false,
    attachments: [descriptor],
    trouble: "",
  };
  page.answers["hud_detach"] = {
    sending: false,
    thinking: false,
    attachments: [],
    trouble: "",
  };
  await settle();

  page.element("attach").dispatch("click", {});
  await settle();
  assert.deepEqual(page.commands(), ["hud_ready", "hud_attach"]);
  const selected = page.element("selected");
  assert.equal(selected.children.length, 1);
  assert.equal(
    selected.children[0]?.children[0]?.textContent,
    "diagram.png - 2 KiB",
  );

  page.publish("scufris://said", {
    ...line("assistant", "Here it is."),
    attachments: [descriptor],
  });
  const rendered = page.element("lines").children[0]?.children[2];
  assert.ok(rendered !== undefined);
  const card = rendered.children[0];
  assert.ok(card !== undefined);
  assert.equal(card.children[0]?.className, "attachment-preview");
  assert.equal(card.children[0]?.src, "scufris-attachment://content/att_one");
  assert.equal(card.children[1]?.children[0]?.textContent, "diagram.png");
  card.children[2]?.dispatch("click", { stopPropagation: () => {} });
  await settle();
  assert.ok(!page.commands().includes("hud_open_attachment"));
  assert.deepEqual(
    page.lastCall("hud_save_attachment")["descriptor"],
    descriptor,
  );

  selected.children[0]?.children[1]?.dispatch("click", {
    stopPropagation: () => {},
  });
  await settle();
  assert.equal(page.lastCall("hud_detach")["id"], "att_one");
  assert.equal(selected.children.length, 0);
});

test("octet-stream videos use their filename for inline playback", async () => {
  const descriptor = {
    id: "att_video",
    name: "answer.mp4",
    media_type: "application/octet-stream",
    size: 4096,
  };
  const page = hud();
  await settle();

  page.publish("scufris://said", {
    ...line("assistant", "Here is the video."),
    attachments: [descriptor],
  });
  const card = page.element("lines").children[0]?.children[2]?.children[0];
  assert.ok(card !== undefined);
  const video = card.children[0];
  assert.equal(video?.className, "attachment-preview attachment-video");
  assert.equal(video?.src, "scufris-attachment://content/att_video");
  assert.equal(video?.controls, true);
  assert.equal(video?.preload, "metadata");
  assert.equal(card.children[1]?.children[1]?.textContent, "video/mp4 - 4 KiB");
});

test("a person who has scrolled up to read is not dragged back down", async () => {
  const page = hud();
  await settle();
  const lines = page.element("lines");
  lines.scrollHeight = 1000;
  lines.clientHeight = 200;

  // Reading the bottom: within the threshold, so the next line follows.
  lines.scrollTop = 790;
  page.publish("scufris://said", line("assistant", "one"));
  assert.equal(lines.scrollTop, 1000);

  // Scrolled up to read something. 24 pixels is the threshold, and one past
  // it is a person who is reading rather than one who is at the bottom.
  lines.scrollTop = 776;
  page.publish("scufris://said", line("assistant", "two"));
  assert.equal(lines.scrollTop, 776);

  // Exactly at the threshold is still reading.
  lines.scrollTop = 1000 - 200 - 24;
  page.publish("scufris://said", line("assistant", "three"));
  assert.equal(lines.scrollTop, 776);
});

test("the notice says the worst thing that is true", async () => {
  const page = hud();
  await settle();
  const notice = page.element("notice");
  assert.equal(notice.dataset["tone"], "keys");
  assert.equal(notice.textContent, "enter sends - + attaches - esc closes");

  page.publish("scufris://notice", {
    sending: true,
    thinking: false,
    trouble: "",
  });
  assert.equal(notice.dataset["tone"], "sending");
  assert.equal(notice.textContent, "sending");

  // Trouble outranks sending. A line that is still in flight and a service
  // that has gone away are both true at once, and the second is the one the
  // person can do something about.
  page.publish("scufris://notice", {
    sending: true,
    thinking: false,
    trouble: "Scufris is not reachable.",
  });
  assert.equal(notice.dataset["tone"], "trouble");
  assert.equal(notice.textContent, "Scufris is not reachable.");

  page.publish("scufris://notice", {
    sending: false,
    thinking: false,
    trouble: "",
  });
  assert.equal(notice.dataset["tone"], "keys");
});

test("working adds one transient thinking line and terminal state hides it", async () => {
  const page = hud(true, [line("user", "check this")]);
  await settle();
  const lines = page.element("lines");

  page.publish("scufris://notice", {
    sending: false,
    thinking: true,
    trouble: "",
  });
  assert.equal(lines.children.length, 2);
  const thinking = lines.children[1];
  assert.ok(thinking !== undefined);
  assert.equal(thinking.dataset["transient"], "thinking");
  assert.deepEqual(
    thinking.children.map((part) => part.content),
    ["scufris", "thinking..."],
  );

  page.publish("scufris://notice", {
    sending: false,
    thinking: true,
    trouble: "",
  });
  assert.equal(lines.children.length, 2);

  page.publish("scufris://notice", {
    sending: false,
    thinking: false,
    trouble: "",
  });
  assert.equal(lines.children.length, 1);

  page.publish("scufris://notice", {
    sending: false,
    thinking: true,
    trouble: "",
  });
  const secondThinking = lines.children[1];
  assert.ok(secondThinking !== undefined);
  page.publish("scufris://said", line("assistant", "done"));
  assert.equal(lines.children.length, 2);
  assert.equal(secondThinking.parent, null);
});

test("one Enter is one message and Shift+Enter is a newline", async () => {
  const page = hud();
  await settle();
  const words = page.element("words");

  words.value = "  ";
  assert.ok(tap(page, "Enter"));
  await settle();
  assert.deepEqual(page.commands(), ["hud_ready"]);

  words.value = "what is on my calendar";
  assert.ok(tap(page, "Enter"));
  await settle();
  assert.equal(page.lastCall("hud_submit")["text"], "what is on my calendar");

  // Shift+Enter is the field's own newline. The page does not take the key
  // and does not send.
  words.value = "first line";
  assert.equal(tap(page, "Enter", true), false);
  await settle();
  assert.equal(
    page.commands().filter((command) => command === "hud_submit").length,
    1,
  );
});

test("a line the host would not take stays in the field", async () => {
  // M4. The page cleared the field and then asked. `Conversation::typed`
  // refuses a second line while one is in flight and answers nothing at all -
  // no transcript entry, no notice, no trouble - so the sentence went
  // nowhere and nothing said so. Refusing rather than queueing is only
  // acceptable because the words stay where the person can send them again.
  const page = hud(false);
  await settle();
  const words = page.element("words");
  words.value = "and one more thing";
  tap(page, "Enter");
  await settle();
  assert.equal(page.lastCall("hud_submit")["text"], "and one more thing");
  assert.equal(words.value, "and one more thing");

  // Taken is what clears it, and it clears without waiting for the service.
  const accepted = hud(true);
  await settle();
  const field = accepted.element("words");
  field.value = "and one more thing";
  tap(accepted, "Enter");
  await settle();
  assert.equal(field.value, "");
});

test("Escape asks the host to put the window away", async () => {
  const page = hud();
  await settle();
  assert.ok(tap(page, "Escape"));
  await settle();
  assert.deepEqual(page.commands(), ["hud_ready", "hud_close"]);
});

test("the keys are the field's, however the window came by them", async () => {
  const page = hud();
  await settle();
  const words = page.element("words");
  assert.equal(page.activeElement, words);

  // A click on the border, or a focus-follows-mouse enter, gives the window
  // the keyboard without giving it to the field.
  page.activeElement = null;
  page.window.dispatch("focus", {});
  assert.equal(page.activeElement, words);
});

// ---------- the form box ----------

/** The food ask, which is the one with a typeahead on it. */
function food(page: Page): void {
  put(page, [
    asking({ name: "name", label: "Food", suggest: true }),
    asking({ name: "amount", label: "Amount" }),
  ]);
}

/** What the backend answers a search with, inside an ordinary reading. */
const CHICKEN = [
  { id: "chicken:g", label: "Chicken (g)" },
  { id: "chicken breast:g", label: "Chicken breast (g)" },
  { id: "chicken thigh:g", label: "Chicken thigh (g)" },
];

test("the box draws the fields it was given and takes the first one", () => {
  const page = form();
  put(
    page,
    [asking({ name: "value", label: "Kilograms", value: "81.4" })],
    "Weight for 2026-08-28",
  );
  assert.equal(page.element("title").textContent, "Weight for 2026-08-28");
  const field = box(page, 0);
  assert.equal(field.value, "81.4");
  assert.equal(page.activeElement, field);
  // A field that arrived with something in it is one the person is
  // correcting, so what is there is selected and typing replaces it.
  assert.equal(field.selectionStart, 0);
  assert.equal(field.selectionEnd, "81.4".length);
  assert.equal(offered(page, 0), undefined, "a field nobody offered a list");
});

test("a field that offers candidates asks once the typing stops", () => {
  const page = form();
  food(page);
  const field = box(page, 0);

  field.value = "chi";
  field.dispatch("input", {});
  field.value = "chick";
  field.dispatch("input", {});
  // One question for a burst of keys. A backend answering a typeahead runs a
  // command per question, so a question per keystroke is a process per key.
  assert.deepEqual(page.commands(), ["form_ready"]);
  page.elapse();
  assert.deepEqual(said(page.lastCall("form_look")), {
    field: "name",
    text: "chick",
  });

  // The page names a field and what is in it, and nothing else. What the
  // backend is asked stays with the host.
  assert.deepEqual(Object.keys(page.lastCall("form_look")), ["field", "text"]);
});

test("a field emptied again asks nothing and offers nothing", () => {
  const page = form();
  food(page);
  type(page, 0, "chick");
  page.publish("scufris://look", { choices: CHICKEN });
  const list = offered(page, 0);
  assert.ok(list !== undefined);
  assert.equal(list.children.length, 3);

  const before = page.commands().length;
  type(page, 0, "   ");
  assert.equal(page.commands().length, before, "an empty field asked anyway");
  assert.equal(list.children.length, 0);
});

test("the list under the field is the backend's own reading", () => {
  const page = form();
  food(page);
  type(page, 0, "chick");
  page.publish("scufris://look", {
    date: "2026-08-28",
    choices: [...CHICKEN, { id: "oats:g" }, "oats"],
  });
  const list = offered(page, 0);
  assert.ok(list !== undefined);
  // Read the way every reading is read: what is not a candidate is dropped
  // rather than drawn as one.
  assert.deepEqual(
    list.children.map((row) => row.textContent),
    CHICKEN.map((choice) => choice.label),
  );

  // A reading with no candidates in it is the backend saying there are none,
  // not a reading to leave the last answer standing under.
  page.publish("scufris://look", { date: "2026-08-28" });
  assert.equal(list.children.length, 0);
});

test("a candidate taken from the list answers with its id", () => {
  const page = form();
  food(page);
  type(page, 0, "chick");
  page.publish("scufris://look", { choices: CHICKEN });
  const list = offered(page, 0);
  assert.ok(list !== undefined);
  const row = list.children[1];
  assert.ok(row !== undefined);
  row.dispatch("mousedown", { preventDefault: () => {} });

  // What the person reads is the label; what the backend is told is the id.
  assert.equal(box(page, 0).value, "Chicken breast (g)");
  assert.equal(list.children.length, 0, "the list stayed open");
  // On to the amount, which is what the person came to say next.
  assert.equal(page.activeElement, box(page, 1));

  box(page, 1).value = "150";
  tap(page, "Enter");
  assert.deepEqual(said(page.lastCall("form_submit")["answers"]), {
    name: "chicken breast:g",
    amount: "150",
  });
});

test("typing again drops the candidate that was taken", () => {
  // A candidate stands only for the words it was taken for. The id would
  // otherwise outlive them and log a food nobody asked for.
  const page = form();
  food(page);
  type(page, 0, "chick");
  page.publish("scufris://look", { choices: CHICKEN });
  const list = offered(page, 0);
  assert.ok(list !== undefined);
  const row = list.children[0];
  assert.ok(row !== undefined);
  row.dispatch("mousedown", { preventDefault: () => {} });

  type(page, 0, "oats");
  tap(page, "Enter");
  assert.deepEqual(said(page.lastCall("form_submit")["answers"]), {
    name: "oats",
    amount: "",
  });
});

test("the arrow keys walk the list and wrap at either end", () => {
  const page = form();
  food(page);
  type(page, 0, "chick");
  page.publish("scufris://look", { choices: CHICKEN });
  const list = offered(page, 0);
  assert.ok(list !== undefined);
  const on = (): number[] =>
    list.children.flatMap((row, index) =>
      row.classes.has("on") ? [index] : [],
    );

  assert.deepEqual(on(), [], "a list opens on no row");
  tap(page, "ArrowDown");
  assert.deepEqual(on(), [0]);
  tap(page, "ArrowDown");
  tap(page, "ArrowDown");
  assert.deepEqual(on(), [2]);
  tap(page, "ArrowDown");
  assert.deepEqual(on(), [0], "the list did not wrap");

  // Up from nothing starts at the end, which is the key pointing that way.
  page.publish("scufris://look", { choices: CHICKEN });
  tap(page, "ArrowUp");
  assert.deepEqual(on(), [2]);
});

test("enter takes the row the keys are on before it saves", () => {
  const page = form();
  food(page);
  type(page, 0, "chick");
  page.publish("scufris://look", { choices: CHICKEN });
  tap(page, "ArrowDown");
  tap(page, "ArrowDown");
  tap(page, "Enter");
  assert.ok(
    !page.commands().includes("form_submit"),
    "enter saved with a candidate highlighted",
  );
  assert.equal(box(page, 0).value, "Chicken breast (g)");

  // One more Enter is the save. The same key does the same thing to whatever
  // is in front of the person.
  box(page, 1).value = "150";
  tap(page, "Enter");
  assert.deepEqual(said(page.lastCall("form_submit")["answers"]), {
    name: "chicken breast:g",
    amount: "150",
  });
});

test("enter with no list open saves, and escape writes nothing", () => {
  const page = form();
  food(page);
  type(page, 0, "oats:g");
  box(page, 1).value = "80";
  tap(page, "Enter");
  assert.deepEqual(said(page.lastCall("form_submit")["answers"]), {
    name: "oats:g",
    amount: "80",
  });

  tap(page, "Escape");
  assert.ok(page.commands().includes("form_cancel"));
});

test("a question replacing another leaves nothing of the first behind", () => {
  // The box is one window, reused. A keystroke still settling from the last
  // question would ask about a field this one never carried.
  const page = form();
  food(page);
  const field = box(page, 0);
  field.value = "chick";
  field.dispatch("input", {});
  put(page, [asking({ name: "body", label: "Note", lines: 6 })], "Note 1");
  page.elapse();
  assert.ok(!page.commands().includes("form_look"), "the old field asked");
  assert.equal(page.element("fields").children.length, 1);

  box(page, 0).value = "A month view.";
  tap(page, "Enter");
  assert.deepEqual(said(page.lastCall("form_submit")["answers"]), {
    body: "A month view.",
  });
});
