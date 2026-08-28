// One day of the journal, and what follows it.
//
// There is one rule here rather than two modes: the selected day in full, then
// the tasks dated after it. Today is only the day selected by default, so a
// future day shows an empty habit list - which is honest - and a past day shows
// what was done, without a second layout to keep in step with the first.
//
// The month is drawn from `marks`, the dates the backend found an incomplete
// task on. It looks forward: a day before the earlier of today and the
// selection carries no dot, because nothing scans a whole month to find one.
//
// Clicks, never keys. The window is built unfocusable so a panel landing
// mid-sentence cannot take the keyboard, and ticking a habit has never needed
// focus. A tick sends one action, the backend writes it through `today` and
// reads the journal back, so a habit ticked here and one ticked in the editor
// arrive the same way.

const MONTHS = [
  "January",
  "February",
  "March",
  "April",
  "May",
  "June",
  "July",
  "August",
  "September",
  "October",
  "November",
  "December",
];

/** Monday first, the way a week is planned rather than the way it is indexed. */
const WEEK = ["M", "T", "W", "T", "F", "S", "S"];

/** Rows in the month grid. Fixed, so the list below does not move when a
 * month needs one row more than the last. */
const ROWS = 6;

interface Habit {
  name: string;
  done: boolean;
}

interface Task {
  index: number;
  text: string;
  done: boolean;
}

interface Ahead {
  date: string;
  text: string;
}

/** Splits an ISO date, or nothing if it is not one. */
function parts(iso: string): [number, number, number] | undefined {
  const found = /^(\d{4})-(\d{2})-(\d{2})$/.exec(iso);
  if (found === null) return undefined;
  return [Number(found[1]), Number(found[2]), Number(found[3])];
}

/** Writes one ISO date. */
function iso(year: number, month: number, day: number): string {
  const pad = (value: number): string => String(value).padStart(2, "0");
  return `${String(year)}-${pad(month)}-${pad(day)}`;
}

/** How many days a month has.
 *
 * Day zero of the next month is the last day of this one, and `Date.UTC`
 * rather than the local constructor because a bare ISO date is midnight UTC
 * and west of Greenwich that is the day before. */
function length(year: number, month: number): number {
  return new Date(Date.UTC(year, month, 0)).getUTCDate();
}

/** Which column the first of the month falls in, counting from Monday. */
function lead(year: number, month: number): number {
  return (new Date(Date.UTC(year, month - 1, 1)).getUTCDay() + 6) % 7;
}

/** Names one month, counting from one. */
function named(month: number): string {
  return MONTHS[month - 1] ?? "";
}

/** Reads an ISO date as a short day and month. */
function shortly(value: string): string {
  const split = parts(value);
  if (split === undefined) return value;
  const [, month, day] = split;
  return `${String(day)} ${named(month).slice(0, 3)}`;
}

/** One small uppercase line, the shell's own quiet register. */
function label(): HTMLSpanElement {
  const span = document.createElement("span");
  span.style.fontSize = "var(--sw-size-small)";
  span.style.letterSpacing = "var(--sw-track)";
  span.style.textTransform = "uppercase";
  span.style.color = "var(--sw-muted)";
  return span;
}

/** Reads one array of objects out of an untyped frame. */
function items(data: unknown, key: string): Record<string, unknown>[] {
  const held = (data as Record<string, unknown>)[key];
  if (!Array.isArray(held)) return [];
  return held.filter(
    (item): item is Record<string, unknown> =>
      typeof item === "object" && item !== null,
  );
}

export function mount(root: HTMLElement, ctx: WidgetContext): WidgetView {
  const frame = document.createElement("div");
  frame.style.display = "flex";
  frame.style.flexDirection = "column";
  frame.style.height = "100%";
  frame.style.gap = "8px";

  const head = document.createElement("div");
  head.style.display = "flex";
  head.style.alignItems = "center";
  head.style.justifyContent = "space-between";
  head.style.gap = "6px";

  const step = (glyph: string, title: string): HTMLButtonElement => {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "tick";
    button.textContent = glyph;
    button.title = title;
    return button;
  };

  const back = step("<", "Previous month");
  const on = step(">", "Next month");

  // The title is also the way home: one place that puts the calendar back on
  // the month it is and the selection back on the day it is.
  const title = document.createElement("button");
  title.type = "button";
  title.className = "tick";
  title.style.flex = "1";
  title.style.letterSpacing = "var(--sw-track)";
  title.style.textTransform = "uppercase";
  title.title = "Back to today";
  title.textContent = "--";

  head.append(back, title, on);

  const week = document.createElement("div");
  week.style.display = "grid";
  week.style.gridTemplateColumns = "repeat(7, 1fr)";
  for (const day of WEEK) {
    const cell = label();
    cell.style.textAlign = "center";
    cell.style.color = "var(--sw-line)";
    cell.textContent = day;
    week.append(cell);
  }

  const month = document.createElement("div");
  month.style.display = "grid";
  month.style.gridTemplateColumns = "repeat(7, 1fr)";
  month.style.gridAutoRows = "22px";
  month.style.flex = "0 0 auto";

  const cells: HTMLButtonElement[] = [];
  for (let index = 0; index < ROWS * 7; index += 1) {
    const cell = document.createElement("button");
    cell.type = "button";
    cell.style.font = "inherit";
    cell.style.fontSize = "var(--sw-size-small)";
    cell.style.fontVariantNumeric = "tabular-nums";
    cell.style.background = "transparent";
    cell.style.border = "1px solid transparent";
    cell.style.cursor = "default";
    cell.style.position = "relative";
    cell.style.padding = "0";
    month.append(cell);
    cells.push(cell);
  }

  const list = document.createElement("div");
  list.style.flex = "1";
  list.style.minHeight = "0";
  list.style.overflowY = "auto";
  list.style.display = "flex";
  list.style.flexDirection = "column";
  list.style.gap = "8px";

  frame.append(head, week, month, list);
  root.append(frame);

  /** The month on screen. Paged on its own, so looking ahead is not selecting. */
  let shown: [number, number] = [1970, 1];
  let selected = "";
  let now = "";
  let marks = new Set<string>();
  let seen = "";

  const paint = (): void => {
    const [year, number] = shown;
    title.textContent = `${named(number)} ${String(year)}`;
    const skip = lead(year, number);
    const days = length(year, number);
    cells.forEach((cell, index) => {
      const day = index - skip + 1;
      if (day < 1 || day > days) {
        cell.replaceChildren();
        cell.style.visibility = "hidden";
        cell.onclick = null;
        return;
      }
      const date = iso(year, number, day);
      cell.style.visibility = "";
      cell.textContent = String(day);
      // Selection is the loud mark and today is the quiet one, because the
      // panel is read to find out what is on the day it is showing.
      cell.style.borderColor =
        date === selected ? "var(--sw-accent)" : "transparent";
      cell.style.color =
        date === selected
          ? "var(--sw-fg-strong)"
          : date === now
            ? "var(--sw-accent)"
            : "var(--sw-muted)";
      if (marks.has(date)) {
        const dot = document.createElement("span");
        dot.style.position = "absolute";
        dot.style.left = "50%";
        dot.style.bottom = "1px";
        dot.style.width = "3px";
        dot.style.height = "3px";
        dot.style.marginLeft = "-1.5px";
        dot.style.background =
          date === selected ? "var(--sw-fg-strong)" : "var(--sw-accent)";
        cell.append(dot);
      }
      cell.onclick = (): void => {
        ctx.send({ action: "select", date });
      };
    });
  };

  const page = (by: number): void => {
    const [year, number] = shown;
    const moved = number - 1 + by;
    shown = [year + Math.floor(moved / 12), (((moved % 12) + 12) % 12) + 1];
    paint();
  };

  back.addEventListener("click", () => {
    page(-1);
  });
  on.addEventListener("click", () => {
    page(1);
  });
  title.addEventListener("click", () => {
    ctx.send({ action: "select", date: null });
  });

  /** A heading over one group, so an empty group takes no room at all. */
  const group = (name: string): HTMLElement => {
    const block = document.createElement("div");
    block.style.display = "flex";
    block.style.flexDirection = "column";
    block.style.gap = "3px";
    const heading = label();
    heading.style.color = "var(--sw-line)";
    heading.textContent = name;
    block.append(heading);
    list.append(block);
    return block;
  };

  /** One line that can be ticked, or one that only reads. */
  const line = (
    box: string,
    text: string,
    done: boolean,
    act: (() => void) | undefined,
  ): HTMLElement => {
    const row = document.createElement("button");
    row.type = "button";
    row.style.font = "inherit";
    row.style.display = "flex";
    row.style.gap = "8px";
    row.style.width = "100%";
    row.style.textAlign = "left";
    row.style.background = "transparent";
    row.style.border = "none";
    row.style.padding = "0";
    row.style.cursor = "default";
    row.style.color = done ? "var(--sw-muted)" : "var(--sw-fg)";

    const mark = document.createElement("span");
    mark.style.flex = "0 0 auto";
    mark.style.color = done ? "var(--sw-accent)" : "var(--sw-line)";
    mark.textContent = box;

    const what = document.createElement("span");
    what.style.flex = "1";
    what.style.minWidth = "0";
    what.style.overflow = "hidden";
    what.style.textOverflow = "ellipsis";
    what.style.whiteSpace = "nowrap";
    if (done) what.style.textDecoration = "line-through";
    what.textContent = text;

    row.append(mark, what);
    if (act !== undefined) row.addEventListener("click", act);
    return row;
  };

  const say = (text: string, colour: string): void => {
    const note = document.createElement("div");
    note.style.color = colour;
    note.style.fontSize = "var(--sw-size-small)";
    note.style.lineHeight = "var(--sw-lead)";
    note.textContent = text;
    list.append(note);
  };

  const fill = (data: unknown): void => {
    list.replaceChildren();

    const trouble = (data as { trouble?: unknown }).trouble;
    if (typeof trouble === "string" && trouble !== "") {
      say(trouble, "var(--sw-attention)");
    }

    const habits = items(data, "habits").flatMap((item): Habit[] =>
      typeof item.name === "string"
        ? [{ name: item.name, done: item.done === true }]
        : [],
    );
    if (habits.length > 0) {
      const block = group("habits");
      for (const habit of habits) {
        block.append(
          line(habit.done ? "[x]" : "[ ]", habit.name, habit.done, () => {
            ctx.send({ action: "habit", name: habit.name });
          }),
        );
      }
    }

    const tasks = items(data, "tasks").flatMap((item): Task[] =>
      typeof item.text === "string" && typeof item.index === "number"
        ? [{ index: item.index, text: item.text, done: item.done === true }]
        : [],
    );
    const block = group("tasks");
    if (tasks.length === 0) {
      const empty = label();
      empty.style.textTransform = "none";
      empty.style.letterSpacing = "normal";
      empty.textContent = "Nothing for this day.";
      block.append(empty);
    }
    for (const task of tasks) {
      block.append(
        line(task.done ? "[x]" : "[ ]", task.text, task.done, () => {
          ctx.send({ action: "task", index: task.index });
        }),
      );
    }

    const ahead = items(data, "ahead").flatMap((item): Ahead[] =>
      typeof item.text === "string" && typeof item.date === "string"
        ? [{ date: item.date, text: item.text }]
        : [],
    );
    if (ahead.length > 0) {
      const later = group("ahead");
      for (const task of ahead) {
        // The date reads as the tick, because clicking one of these is how the
        // day it belongs to is opened.
        later.append(
          line(shortly(task.date).padStart(6, " "), task.text, false, () => {
            ctx.send({ action: "select", date: task.date });
          }),
        );
      }
    }
  };

  const view: WidgetView = {
    update(data: unknown): void {
      if (typeof data !== "object" || data === null) return;
      // The backend repeats its last reading every beat so the panel is never
      // marked stale. Redrawing an unchanged one would throw away where the
      // list was scrolled to, so an unchanged one is left alone.
      const key = JSON.stringify(data);
      if (key === seen) return;
      seen = key;

      const fields = data as {
        date?: unknown;
        today?: unknown;
        marks?: unknown;
      };
      const was = selected;
      if (typeof fields.date === "string") selected = fields.date;
      if (typeof fields.today === "string") now = fields.today;
      marks = new Set(
        Array.isArray(fields.marks)
          ? fields.marks.filter(
              (mark): mark is string => typeof mark === "string",
            )
          : [],
      );

      // The month follows the selection when the selection moves, and stays
      // where it was put when only the day's contents changed.
      const split = parts(selected);
      if (split !== undefined && (was !== selected || shown[0] === 1970)) {
        shown = [split[0], split[1]];
      }
      paint();
      fill(data);
    },
    destroy(): void {
      frame.remove();
    },
  };

  // The spawn payload only says which view this is and how far ahead to look;
  // the backend reports the day. Nothing is drawn until its first line.
  return view;
}
