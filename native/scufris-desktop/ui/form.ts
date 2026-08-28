// The box a panel borrows when it needs words.
//
// It draws the fields the host describes and answers with what was typed into
// them. It never learns what they mean: a widget said what to ask and what to
// do with the answer, the host is holding that, and this page is the field.
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

  /** The fields on screen, in the order they were asked. */
  let asked: HTMLTextAreaElement[] = [];

  const draw = (field: FormField): HTMLTextAreaElement => {
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
    fields.append(block);
    return input;
  };

  const show = (ask: FormAsk): void => {
    title.textContent = ask.title;
    fields.replaceChildren();
    asked = ask.fields.map(draw);
    const first = asked[0];
    if (first === undefined) return;
    first.focus();
    // A field that arrived with something in it is a field the person is
    // correcting, so the old value is selected and typing replaces it. The
    // weight tick is the reason: it opens on the weight already logged.
    if (first.value !== "") first.setSelectionRange(0, first.value.length);
  };

  const save = (): void => {
    const answers: Record<string, string> = {};
    for (const input of asked) answers[input.name] = input.value;
    void invoke("form_submit", { answers });
  };

  window.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      event.preventDefault();
      void invoke("form_cancel");
      return;
    }
    if (event.key === "Enter" && !event.shiftKey) {
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
    if (asked.includes(document.activeElement as HTMLTextAreaElement)) return;
    first.focus();
  });

  void listen("scufris://ask", (event) => {
    show(event.payload as FormAsk);
  });

  // The window is built at startup and the question is pushed just before it
  // comes up, so a page that is still loading misses it. This is how that page
  // catches up; there is nothing to draw when nobody has asked anything.
  void (async () => {
    const pending = (await invoke("form_ready")) as FormAsk | null;
    if (pending !== null) show(pending);
  })();
}
