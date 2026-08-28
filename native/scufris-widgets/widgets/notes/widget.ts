// Today's notes, as they were written.
//
// The read half of the journal's own notes view. There is no add form here and
// that is deliberate: a note is long-form, the window is built unfocusable so a
// panel landing mid-sentence cannot take the keyboard, and a one-line field is
// the wrong shape for a paragraph anyway. `today note add` is where a note is
// written; this is where it is read back.

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

const DAYS = [
  "Sunday",
  "Monday",
  "Tuesday",
  "Wednesday",
  "Thursday",
  "Friday",
  "Saturday",
];

interface Note {
  index: number;
  heading: string;
  body: string;
}

/** Splits an ISO date, or nothing if it is not one. */
function parts(iso: string): [number, number, number] | undefined {
  const found = /^(\d{4})-(\d{2})-(\d{2})$/.exec(iso);
  if (found === null) return undefined;
  return [Number(found[1]), Number(found[2]), Number(found[3])];
}

/** Reads an ISO date as a person would say it.
 *
 * Built through `Date.UTC` rather than by parsing the string: a bare ISO date
 * is midnight UTC, and west of Greenwich that is the day before. */
function stamp(iso: string): string {
  const split = parts(iso);
  if (split === undefined) return iso;
  const [year, month, day] = split;
  const at = new Date(Date.UTC(year, month - 1, day));
  return `${DAYS[at.getUTCDay()]} ${String(day)} ${MONTHS[month - 1]} ${String(year)}`;
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

export function mount(root: HTMLElement, ctx: WidgetContext): WidgetView {
  const frame = document.createElement("div");
  frame.style.display = "flex";
  frame.style.flexDirection = "column";
  frame.style.height = "100%";
  frame.style.gap = "10px";

  const when = label();
  when.textContent = "--";

  // The panel scrolls, not the page: the shell's root is `overflow: hidden` so
  // that a widget drawing past its window clips instead of moving the chrome.
  const list = document.createElement("div");
  list.style.flex = "1";
  list.style.minHeight = "0";
  list.style.overflowY = "auto";
  list.style.display = "flex";
  list.style.flexDirection = "column";
  list.style.gap = "12px";

  frame.append(when, list);
  root.append(frame);

  const say = (text: string, colour: string): void => {
    list.replaceChildren();
    const line = document.createElement("div");
    line.style.color = colour;
    line.style.fontSize = "var(--sw-size-small)";
    line.style.lineHeight = "var(--sw-lead)";
    line.textContent = text;
    list.append(line);
  };

  const read = (data: unknown): Note[] => {
    if (typeof data !== "object" || data === null) return [];
    const held = (data as { notes?: unknown }).notes;
    if (!Array.isArray(held)) return [];
    return held.flatMap((item): Note[] => {
      if (typeof item !== "object" || item === null) return [];
      const note = item as {
        index?: unknown;
        heading?: unknown;
        body?: unknown;
      };
      if (typeof note.heading !== "string") return [];
      return [
        {
          index: typeof note.index === "number" ? note.index : 0,
          heading: note.heading,
          body: typeof note.body === "string" ? note.body : "",
        },
      ];
    });
  };

  let seen = "";

  const view: WidgetView = {
    update(data: unknown): void {
      if (typeof data !== "object" || data === null) return;
      // The backend repeats its last reading every beat so the panel is never
      // marked stale. Redrawing an unchanged one would throw away where the
      // list was scrolled to, so an unchanged one is left alone.
      const key = JSON.stringify(data);
      if (key === seen) return;
      seen = key;
      const fields = data as { date?: unknown; trouble?: unknown };
      if (typeof fields.date === "string")
        when.textContent = stamp(fields.date);
      if (typeof fields.trouble === "string" && fields.trouble !== "") {
        say(fields.trouble, "var(--sw-attention)");
        return;
      }
      const notes = read(data);
      if (notes.length === 0) {
        say("No notes today.", "var(--sw-muted)");
        return;
      }
      list.replaceChildren();
      for (const note of notes) {
        const block = document.createElement("div");
        block.style.display = "flex";
        block.style.flexDirection = "column";
        block.style.gap = "3px";

        const heading = label();
        heading.style.color = "var(--sw-accent)";
        heading.textContent = note.heading;

        const body = document.createElement("div");
        body.style.color = "var(--sw-fg)";
        body.style.lineHeight = "var(--sw-lead)";
        // The journal's own line breaks are the note's shape, so they are kept
        // rather than reflowed.
        body.style.whiteSpace = "pre-wrap";
        body.style.overflowWrap = "anywhere";
        body.textContent = note.body;

        block.append(heading, body);
        list.append(block);
      }
    },
    destroy(): void {
      frame.remove();
    },
  };

  // The spawn payload only says which view this is; the backend reports the
  // day. Nothing is drawn until its first line, one beat away.
  void ctx;
  return view;
}
