// The box a panel borrows when it needs words.
//
// It draws the fields the host describes and answers with what was typed into
// them. It never learns what they mean: a widget said what to ask and what to
// do with the answer, the host is holding that, and this page is the field.
//
// A field may also offer candidates. The page sends what has been typed and the
// backend answers in its next reading; the list under the field is that answer.
// The page never learns what it asked - the host builds the question from the
// field's own name - and a field picked from answers with the candidate's id
// rather than the words the person read.
//
// The keys are the same bargain the transcript box and the conversation window
// make. Enter saves, Shift+Enter is a newline, Escape closes with nothing
// written. One rule in every window of this companion that takes words.
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
      invoke("pill_log", { level: `form.${level}`, message }).catch(() => {});
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
    if (found === null) throw new Error(`the form page is missing #${id}`);
    return found as T;
  };

  const title = element<HTMLElement>("title");
  const fields = element<HTMLElement>("fields");

  /** One line inside a field, and the padding and rule around them all.
   *
   * The same two numbers as LINE and BOX in src/form.rs, which sizes the
   * window before it maps. A field measured differently here is a box with a
   * gap under it or a field the frame cuts off. */
  const LINE = 20;
  const BOX = 14;

  /** How long a field waits after a keystroke before asking.
   *
   * A backend answering a typeahead runs a command per question, so every
   * keystroke asking would be a process per keystroke. Short enough that the
   * list arrives while the person is still looking at the field. */
  const SETTLE = 120;

  /** One field on screen, and the candidate it has been given. */
  interface Asked {
    name: string;
    input: HTMLTextAreaElement;
    /** The list under it, or nothing for a field with no candidates. */
    list: HTMLElement | undefined;
    /** What a taken candidate answers with, until the field is typed in. */
    picked: string | undefined;
    /** Which row the keys are on, or -1 for none. */
    row: number;
    /** What is in the list now. */
    choices: FormChoice[];
  }

  /** The fields on screen, in the order they were asked. */
  let asked: Asked[] = [];

  /** The keystroke waiting to be asked about, if any. */
  let settling: number | undefined;

  /** The field the person is in, or the first that offers candidates. */
  const current = (): Asked | undefined => {
    const focused = asked.find((one) => one.input === document.activeElement);
    if (focused !== undefined) return focused;
    return asked.find((one) => one.list !== undefined);
  };

  const put = (one: Asked, choices: FormChoice[]): void => {
    if (one.list === undefined) return;
    one.choices = choices;
    one.row = -1;
    one.list.replaceChildren();
    for (const [index, choice] of choices.entries()) {
      const row = document.createElement("button");
      row.type = "button";
      row.className = "choice";
      row.textContent = choice.label;
      // The pointer is the plain way in and the keys are the fast one. Mousedown
      // rather than click: a click would first take the keyboard off the field,
      // and the field is where the person goes back to.
      row.addEventListener("mousedown", (event) => {
        event.preventDefault();
        take(one, index);
      });
      one.list.append(row);
    }
  };

  const mark = (one: Asked): void => {
    if (one.list === undefined) return;
    for (const [index, row] of [...one.list.children].entries()) {
      row.classList.toggle("on", index === one.row);
    }
    const row = one.list.children[one.row];
    if (row !== undefined) row.scrollIntoView({ block: "nearest" });
  };

  const take = (one: Asked, index: number): void => {
    const choice = one.choices[index];
    if (choice === undefined) return;
    one.picked = choice.id;
    one.input.value = choice.label;
    put(one, []);
    // On to the next field, because a taken candidate is a finished answer and
    // the amount is what the person came to say next. The last field keeps the
    // keys: there is nowhere after it but Enter.
    const next = asked[asked.indexOf(one) + 1];
    if (next === undefined) one.input.focus();
    else next.input.focus();
  };

  const looked = (one: Asked): void => {
    if (one.list === undefined) return;
    // A candidate stands only for the words it was taken for. Typing again is
    // the person saying those were not the words.
    one.picked = undefined;
    const text = one.input.value.trim();
    if (settling !== undefined) window.clearTimeout(settling);
    if (text === "") {
      put(one, []);
      return;
    }
    settling = window.setTimeout(() => {
      settling = undefined;
      void invoke("form_look", { field: one.name, text });
    }, SETTLE);
  };

  const draw = (field: FormField): Asked => {
    const block = document.createElement("div");
    block.className = "field";

    const name = document.createElement("label");
    name.htmlFor = `field-${field.name}`;
    name.textContent = field.label;

    const input = document.createElement("textarea");
    input.id = `field-${field.name}`;
    input.name = field.name;
    input.rows = field.lines;
    // Set rather than left to `rows`, so the field is exactly as tall as the
    // window was built for. A browser's own row height is its line box plus
    // whatever it decides, and the window cannot be resized to meet it.
    input.style.height = `${String(field.lines * LINE + BOX)}px`;
    input.value = field.value;
    input.placeholder = field.hint;
    input.autocomplete = "off";
    input.spellcheck = false;

    block.append(name, input);

    let list: HTMLElement | undefined;
    if (field.suggest) {
      list = document.createElement("div");
      list.className = "choices";
      block.append(list);
    }

    fields.append(block);
    const one: Asked = {
      name: field.name,
      input,
      list,
      picked: undefined,
      row: -1,
      choices: [],
    };
    if (list !== undefined) {
      input.addEventListener("input", () => {
        looked(one);
      });
    }
    return one;
  };

  const show = (ask: FormAsk): void => {
    title.textContent = ask.title;
    fields.replaceChildren();
    if (settling !== undefined) window.clearTimeout(settling);
    settling = undefined;
    asked = ask.fields.map(draw);
    const first = asked[0];
    if (first === undefined) return;
    first.input.focus();
    // A field that arrived with something in it is a field the person is
    // correcting, so the old value is selected and typing replaces it. The
    // weight tick is the reason: it opens on the weight already logged.
    const value = first.input.value;
    if (value !== "") first.input.setSelectionRange(0, value.length);
  };

  const save = (): void => {
    const answers: Record<string, string> = {};
    // A picked candidate answers with its id. The words in the field are what
    // the person read, and the backend was never told them.
    for (const one of asked) answers[one.name] = one.picked ?? one.input.value;
    void invoke("form_submit", { answers });
  };

  window.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      event.preventDefault();
      void invoke("form_cancel");
      return;
    }
    const one = current();
    const open = one !== undefined && one.choices.length > 0;
    if (open && (event.key === "ArrowDown" || event.key === "ArrowUp")) {
      event.preventDefault();
      const down = event.key === "ArrowDown";
      const count = one.choices.length;
      // Wraps, and starts at whichever end the key was pointing at. There is no
      // row to land back on once a list is open: the field is still there to
      // type in, and typing is what closes it.
      one.row =
        one.row < 0
          ? down
            ? 0
            : count - 1
          : (one.row + (down ? 1 : -1) + count) % count;
      mark(one);
      return;
    }
    if (event.key === "Enter" && !event.shiftKey) {
      // A highlighted candidate is what Enter means while a list is open. It is
      // one more Enter to save, which is the same key doing the same thing to
      // whatever is in front of the person.
      if (open && one.row >= 0) {
        event.preventDefault();
        take(one, one.row);
        return;
      }
      // Shift+Enter is the newline, in a block field and in a one-line one
      // alike. The host flattens a one-line answer on the way out, so a
      // newline that gets in never reaches the journal.
      event.preventDefault();
      save();
    }
  });

  // A window manager can hand this window the keyboard without a field taking
  // it - a click on the rule, a focus-follows-mouse enter. The keys are the
  // first field's unless the person has moved to another one.
  window.addEventListener("focus", () => {
    const first = asked[0];
    if (first === undefined) return;
    if (asked.some((one) => one.input === document.activeElement)) return;
    first.input.focus();
  });

  void listen("scufris://ask", (event) => {
    show(event.payload as FormAsk);
  });

  const choices = (data: unknown): FormChoice[] => {
    if (typeof data !== "object" || data === null) return [];
    const held = (data as { choices?: unknown }).choices;
    if (!Array.isArray(held)) return [];
    return held.flatMap((item): FormChoice[] => {
      if (typeof item !== "object" || item === null) return [];
      const choice = item as { id?: unknown; label?: unknown };
      if (typeof choice.id !== "string") return [];
      if (typeof choice.label !== "string") return [];
      return [{ id: choice.id, label: choice.label }];
    });
  };

  // Every reading from the backend the box is asking on behalf of, which is
  // where a typeahead's answer arrives. A reading with no candidates in it is
  // the backend saying there are none, so the list is emptied rather than left
  // showing the last question's answer.
  void listen("scufris://look", (event) => {
    const one = current();
    if (one === undefined || one.list === undefined) return;
    put(one, choices(event.payload));
  });

  // The window is built at startup and the question is pushed just before it
  // comes up, so a page that is still loading misses it. This is how that page
  // catches up; there is nothing to draw when nobody has asked anything.
  void (async () => {
    const pending = (await invoke("form_ready")) as FormAsk | null;
    if (pending !== null) show(pending);
  })();
}
