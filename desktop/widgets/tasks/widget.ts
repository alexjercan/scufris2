// The task list, and the two things you can do to a line of it.
//
// The file is the truth. This panel renders what the backend read out of it
// and sends back what was clicked, so a task ticked off here and a task an
// editor changed are the same change arriving the same way.
//
// Clicks, never keys. The window is built unfocusable, so nothing typed can
// land here; writing a new task is the file's job until keyboard routing is
// settled. What the panel owns is acting on what is already on the list.

/** How many rows are drawn before the rest are counted instead. */
const ROOM = 7;

interface Item {
  at: number;
  text: string;
  done: boolean;
}

/** Reads one row out of the backend's reading, or nothing if it is not one. */
function item(value: unknown): Item | undefined {
  if (typeof value !== "object" || value === null) return undefined;
  const fields = value as Partial<Record<keyof Item, unknown>>;
  if (typeof fields.at !== "number" || typeof fields.text !== "string") {
    return undefined;
  }
  return { at: fields.at, text: fields.text, done: fields.done === true };
}

export function mount(root: HTMLElement, ctx: WidgetContext): WidgetView {
  const frame = document.createElement("div");
  frame.style.display = "flex";
  frame.style.flexDirection = "column";
  frame.style.gap = "4px";
  frame.style.height = "100%";
  frame.style.overflow = "hidden";

  const list = document.createElement("div");
  list.style.display = "flex";
  list.style.flexDirection = "column";
  list.style.gap = "3px";
  list.style.flex = "1 1 auto";
  list.style.overflow = "hidden";

  // The one line that is not a task: what is not on screen, and where the list
  // lives. A panel that silently showed seven of twenty would be lying.
  const rest = document.createElement("span");
  rest.style.fontSize = "var(--sw-size-small)";
  rest.style.color = "var(--sw-muted)";
  rest.style.letterSpacing = "var(--sw-track)";
  rest.style.textTransform = "uppercase";
  rest.style.flex = "0 0 auto";

  frame.append(list, rest);
  root.append(frame);

  // `tick` is the shell's own control style, the one class a widget may wear:
  // the chrome's ticks and a widget's own controls are the same affordance.
  const tick = (label: string, title: string, act: () => void): HTMLElement => {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "tick";
    button.textContent = label;
    button.title = title;
    button.style.flex = "0 0 auto";
    // The chrome's own padding, tightened. A list is a column of these rather
    // than one control in a corner, and everything else about a tick - its
    // colour, its border, what it does on hover - is left where it was.
    button.style.padding = "2px 5px";
    button.addEventListener("click", act);
    return button;
  };

  const row = (task: Item): HTMLElement => {
    const line = document.createElement("div");
    line.style.display = "flex";
    line.style.alignItems = "center";
    line.style.gap = "6px";

    const said = document.createElement("span");
    said.style.flex = "1 1 auto";
    said.style.minWidth = "0";
    said.style.fontSize = "var(--sw-size-body)";
    said.style.lineHeight = "var(--sw-lead)";
    said.style.overflow = "hidden";
    said.style.textOverflow = "ellipsis";
    said.style.whiteSpace = "nowrap";
    said.textContent = task.text;
    said.title = task.text;
    // A finished task stays on the list until it is dropped, and recedes
    // rather than disappearing: what was done today is worth seeing.
    said.style.color = task.done ? "var(--sw-muted)" : "var(--sw-fg)";
    said.style.textDecoration = task.done ? "line-through" : "none";

    const sent = (action: string) => (): void => {
      ctx.send({ action, at: task.at, text: task.text });
    };

    line.append(
      tick(
        task.done ? "[x]" : "[ ]",
        task.done ? "Not done" : "Done",
        sent("done"),
      ),
      said,
      tick("-", "Take it off the list", sent("drop")),
    );
    return line;
  };

  const view: WidgetView = {
    update(data: unknown): void {
      if (typeof data !== "object" || data === null) return;
      const held = (data as { items?: unknown }).items;
      if (!Array.isArray(held)) return;
      const items = held
        .map(item)
        .filter((task): task is Item => task !== undefined);

      list.replaceChildren(...items.slice(0, ROOM).map(row));
      const over = items.length - ROOM;
      if (items.length === 0) {
        rest.textContent = "nothing on the list";
      } else if (over > 0) {
        rest.textContent = `${String(over)} more`;
      } else {
        rest.textContent = "";
      }
    },
    destroy(): void {
      frame.remove();
    },
  };

  // Nothing is drawn from the spawn payload: it names the file at most, and
  // the list itself only ever comes from the backend that read it.
  void ctx.spawn;
  return view;
}
