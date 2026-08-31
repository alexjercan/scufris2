// The day's notes, as they were written.
//
// The journal's own notes view: a heading and a body, in the order they were
// kept. This page holds no keyboard - a widget window is built unfocusable so
// that a panel landing mid-sentence cannot take the keys of whoever was
// typing - so the tick asks for the words instead, and the companion takes
// them in a window of its own and sends the finished action to the backend.
//
// A note keeps its own line breaks. The block field is what makes that possible
// from here rather than only from `today note add`.
//
// A note on screen is also the way back into itself: clicking one opens the
// same two fields with what it says already in them, and what comes back
// replaces it. So the panel is where a note is read and where it is corrected,
// which is the two things a person does with a note they wrote that morning.

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

/** The red x that removes what its row is.
 *
 * Drawn on the note rather than behind a click, because a control that deletes
 * is one the person has to see before they reach for the block beside it. */
function erase(hint: string, act: () => void): HTMLButtonElement {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "erase";
  button.textContent = "x";
  button.title = hint;
  button.addEventListener("click", act);
  return button;
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

  // The date on the left and the one tick on the right: what the panel is
  // showing, and the way to add to it.
  const head = document.createElement("div");
  head.style.display = "flex";
  head.style.alignItems = "center";
  head.style.justifyContent = "space-between";
  head.style.gap = "6px";
  head.style.flex = "0 0 auto";

  const when = label();
  when.textContent = "--";

  const write = document.createElement("button");
  write.type = "button";
  write.className = "tick";
  write.textContent = "+";
  write.title = "Write a note for this day";

  head.append(when, write);

  // The panel scrolls, not the page: the shell's root is `overflow: hidden` so
  // that a widget drawing past its window clips instead of moving the chrome.
  const list = document.createElement("div");
  list.style.flex = "1";
  list.style.minHeight = "0";
  list.style.overflowY = "auto";
  list.style.display = "flex";
  list.style.flexDirection = "column";
  list.style.gap = "12px";

  frame.append(head, list);
  root.append(frame);

  /** The day on screen, which is the day a note written here belongs to. */
  let showing = "";

  write.addEventListener("click", () => {
    ctx.ask({
      title: showing === "" ? "New note" : `Note for ${stamp(showing)}`,
      fields: [
        { name: "heading", label: "Heading", hint: "what it is about" },
        { name: "body", label: "Note", lines: 6 },
      ],
      action: { action: "note" },
    });
  });

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
      if (typeof fields.date === "string") {
        showing = fields.date;
        when.textContent = stamp(fields.date);
      }
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
        // The note and the one control that removes it, side by side.
        const row = document.createElement("div");
        row.style.display = "flex";
        row.style.alignItems = "flex-start";
        row.style.gap = "6px";

        // A button, because a note is the way back into itself: clicking one
        // opens the same two fields it was written with, filled in. The chrome
        // is stripped rather than styled - a note reads as a note, and the
        // pointer is what says it can be changed.
        const block = document.createElement("button");
        block.type = "button";
        // What the note says, in full: the panel scrolls, and a note read at
        // the edge of it is one the person should not have to scroll to.
        block.title = `${note.heading}\n${note.body}`;
        block.style.flex = "1";
        block.style.minWidth = "0";
        block.style.font = "inherit";
        block.style.background = "transparent";
        block.style.border = "none";
        block.style.padding = "0";
        block.style.textAlign = "left";
        block.style.cursor = "pointer";
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

        block.addEventListener("click", () => {
          ctx.ask({
            title: `Note ${String(note.index)}`,
            fields: [
              {
                name: "heading",
                label: "Heading",
                value: note.heading,
                hint: "what it is about",
              },
              { name: "body", label: "Note", value: note.body, lines: 6 },
            ],
            action: { action: "edit", index: note.index },
          });
        });

        block.append(heading, body);
        row.append(
          block,
          erase(`Remove ${note.heading}`, () => {
            ctx.send({ action: "unnote", index: note.index });
          }),
        );
        list.append(row);
      }
    },
    destroy(): void {
      frame.remove();
    },
  };

  // The spawn payload only says which view this is; the backend reports the
  // day. Nothing is drawn until its first line, one beat away.
  return view;
}
