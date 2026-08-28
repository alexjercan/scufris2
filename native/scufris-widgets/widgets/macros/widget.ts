// What the day was eaten and weighed.
//
// Calories lead because they are the number the day is judged on; the three
// macronutrients behind them are the reason for it, and sit smaller. The
// weight trend is a line rather than a figure because a single weight says
// almost nothing and a month of them says the whole thing.
//
// Two things are written from here. The weight is one field, so clicking the
// number opens it with the number already in it. A food is a name and an
// amount, and the name is a database row rather than words: the field offers
// the database as it is typed and what is taken from that list is the row.
//
// The words themselves are taken elsewhere. A widget window is built
// unfocusable so a panel arriving mid-sentence cannot take the keys of whoever
// was typing, so `ctx.ask` is how a page with no keyboard gets any.

/** The trend's own coordinates. CSS scales it to whatever the panel is. */
const WIDE = 300;
const TALL = 40;

/** How much room is left above and below the line, as a share of the range. */
const AIR = 0.15;

interface Weighing {
  date: string;
  weight: number;
}

interface Food {
  index: number;
  name: string;
  protein: number;
  carbs: number;
  fat: number;
}

/** One small uppercase figure, the shell's own quiet register. */
function figure(): HTMLSpanElement {
  const span = document.createElement("span");
  span.style.fontSize = "var(--sw-size-small)";
  span.style.letterSpacing = "var(--sw-track)";
  span.style.textTransform = "uppercase";
  span.style.fontVariantNumeric = "tabular-nums";
  span.style.color = "var(--sw-muted)";
  return span;
}

/** Reads a gram figure without trailing noise: 128, not 128.0. */
function grams(value: number): string {
  return Number.isInteger(value) ? String(value) : value.toFixed(1);
}

/** Reads a number out of an untyped field, or nothing. */
function number(value: unknown): number | undefined {
  return typeof value === "number" && Number.isFinite(value)
    ? value
    : undefined;
}

export function mount(root: HTMLElement, ctx: WidgetContext): WidgetView {
  const frame = document.createElement("div");
  frame.style.display = "flex";
  frame.style.flexDirection = "column";
  frame.style.height = "100%";
  frame.style.gap = "6px";

  const head = document.createElement("div");
  head.style.display = "flex";
  head.style.alignItems = "baseline";
  head.style.justifyContent = "space-between";
  head.style.gap = "8px";

  const calories = document.createElement("span");
  calories.style.fontSize = "var(--sw-size-big)";
  calories.style.fontVariantNumeric = "tabular-nums";
  calories.style.color = "var(--sw-fg)";
  calories.textContent = "--";

  // A button rather than a figure, because the way to log a weight is to
  // correct the one on screen. The dashes under it are the only chrome it
  // needs: a bordered tick here would read as a control beside the number
  // rather than as the number itself.
  const weight = document.createElement("button");
  weight.type = "button";
  weight.title = "Log the weight for this day";
  weight.style.font = "inherit";
  weight.style.fontSize = "var(--sw-size-body)";
  weight.style.letterSpacing = "var(--sw-track)";
  weight.style.fontVariantNumeric = "tabular-nums";
  weight.style.color = "var(--sw-muted)";
  weight.style.background = "transparent";
  weight.style.border = "none";
  weight.style.borderBottom = "1px dashed var(--sw-line)";
  weight.style.padding = "0 0 1px";
  weight.style.cursor = "default";
  weight.textContent = "--";

  head.append(calories, weight);

  const split = document.createElement("div");
  split.style.display = "flex";
  split.style.justifyContent = "space-between";
  split.style.gap = "8px";

  const protein = figure();
  const carbs = figure();
  const fat = figure();
  split.append(protein, carbs, fat);

  const trend = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  trend.setAttribute("viewBox", `0 0 ${WIDE} ${TALL}`);
  trend.setAttribute("preserveAspectRatio", "none");
  trend.setAttribute("role", "img");
  trend.setAttribute("aria-label", "Weight over the last month");
  trend.style.width = "100%";
  trend.style.height = "40px";
  trend.style.flex = "0 0 auto";
  trend.style.display = "block";

  const line = document.createElementNS(
    "http://www.w3.org/2000/svg",
    "polyline",
  );
  line.setAttribute("fill", "none");
  line.setAttribute("stroke", "var(--sw-accent)");
  line.setAttribute("stroke-width", "1.5");
  line.setAttribute("stroke-linejoin", "round");
  line.setAttribute("vector-effect", "non-scaling-stroke");

  // The newest weighing, so the eye lands on where the line ended rather than
  // having to trace it.
  const last = document.createElementNS("http://www.w3.org/2000/svg", "circle");
  last.setAttribute("r", "2.5");
  last.setAttribute("fill", "var(--sw-accent)");
  last.style.display = "none";

  trend.append(line, last);

  const bar = document.createElement("div");
  bar.style.display = "flex";
  bar.style.alignItems = "center";
  bar.style.justifyContent = "space-between";
  bar.style.gap = "6px";
  bar.style.flex = "0 0 auto";

  const heading = figure();
  heading.textContent = "food";
  heading.style.color = "var(--sw-line)";

  const eat = document.createElement("button");
  eat.type = "button";
  eat.className = "tick";
  eat.textContent = "+";
  eat.title = "Log a food for this day";

  bar.append(heading, eat);

  const list = document.createElement("div");
  list.style.flex = "1";
  list.style.minHeight = "0";
  list.style.overflowY = "auto";
  list.style.display = "flex";
  list.style.flexDirection = "column";
  list.style.gap = "2px";

  frame.append(head, split, trend, bar, list);
  root.append(frame);

  /** The day on screen, and the weight it carries. Both go into the box the
   * ticks open: the day so the title says which one, the weight so correcting
   * it starts from what is already logged. */
  let showing = "";
  let logged: number | undefined;

  weight.addEventListener("click", () => {
    ctx.ask({
      title: showing === "" ? "Weight" : `Weight for ${showing}`,
      fields: [
        {
          name: "value",
          label: "Kilograms",
          value: logged === undefined ? "" : logged.toFixed(1),
        },
      ],
      action: { action: "weight" },
    });
  });

  eat.addEventListener("click", () => {
    ctx.ask({
      title: showing === "" ? "Food" : `Food for ${showing}`,
      fields: [
        {
          name: "name",
          label: "Food",
          hint: "start typing a name",
          // The database is what knows the foods, and it is asked as the
          // person types. Taking one from the list answers with its id, so the
          // amount is scaled against the row rather than against a guess.
          suggest: { action: "search" },
        },
        { name: "amount", label: "Amount", hint: "grams, or pieces" },
      ],
      action: { action: "food" },
    });
  });

  const say = (text: string, colour: string): void => {
    list.replaceChildren();
    const note = document.createElement("div");
    note.style.color = colour;
    note.style.fontSize = "var(--sw-size-small)";
    note.style.lineHeight = "var(--sw-lead)";
    note.textContent = text;
    list.append(note);
  };

  const draw = (recent: Weighing[]): void => {
    if (recent.length < 2) {
      line.removeAttribute("points");
      last.style.display = "none";
      return;
    }
    const values = recent.map((point) => point.weight);
    const low = Math.min(...values);
    const high = Math.max(...values);
    // A flat month is a straight line down the middle, not a divide by zero.
    const span = high - low;
    const pad = span === 0 ? 1 : span * AIR;
    const floor = low - pad;
    const ceiling = high + pad;
    const step = WIDE / (recent.length - 1);
    const places = recent.map((point, index): [number, number] => [
      index * step,
      TALL - ((point.weight - floor) / (ceiling - floor)) * TALL,
    ]);
    line.setAttribute(
      "points",
      places
        .map(([at, height]) => `${at.toFixed(1)},${height.toFixed(1)}`)
        .join(" "),
    );
    const [end, height] = places[places.length - 1] ?? [WIDE, TALL / 2];
    last.setAttribute("cx", end.toFixed(1));
    last.setAttribute("cy", height.toFixed(1));
    last.style.display = "";
  };

  const weighings = (data: unknown): Weighing[] => {
    const held = (data as { recent?: unknown }).recent;
    if (!Array.isArray(held)) return [];
    return held.flatMap((item): Weighing[] => {
      if (typeof item !== "object" || item === null) return [];
      const point = item as { date?: unknown; weight?: unknown };
      const value = number(point.weight);
      if (value === undefined) return [];
      return [
        {
          date: typeof point.date === "string" ? point.date : "",
          weight: value,
        },
      ];
    });
  };

  const foods = (data: unknown): Food[] => {
    const held = (data as { foods?: unknown }).foods;
    if (!Array.isArray(held)) return [];
    return held.flatMap((item): Food[] => {
      if (typeof item !== "object" || item === null) return [];
      const row = item as {
        index?: unknown;
        name?: unknown;
        protein?: unknown;
        carbs?: unknown;
        fat?: unknown;
      };
      if (typeof row.name !== "string") return [];
      return [
        {
          index: number(row.index) ?? 0,
          name: row.name,
          protein: number(row.protein) ?? 0,
          carbs: number(row.carbs) ?? 0,
          fat: number(row.fat) ?? 0,
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
      const fields = data as {
        date?: unknown;
        macros?: unknown;
        weight?: unknown;
        change?: unknown;
        trouble?: unknown;
      };
      if (typeof fields.date === "string") showing = fields.date;

      if (typeof fields.trouble === "string" && fields.trouble !== "") {
        say(fields.trouble, "var(--sw-attention)");
      }

      const totals =
        typeof fields.macros === "object" && fields.macros !== null
          ? (fields.macros as Record<string, unknown>)
          : {};
      const kcal = number(totals.calories);
      calories.textContent = kcal === undefined ? "--" : kcal.toFixed(0);
      // A dash rather than a zero where there is no reading at all: a day
      // nobody logged and a day of nothing are not the same day.
      const gram = (value: unknown): string => {
        const held = number(value);
        return held === undefined ? "--" : `${grams(held)} g`;
      };
      protein.textContent = `p ${gram(totals.protein)}`;
      carbs.textContent = `c ${gram(totals.carbs)}`;
      fat.textContent = `f ${gram(totals.fat)}`;

      const today = number(fields.weight);
      logged = today;
      const change = number(fields.change);
      // The change is signed on purpose. Which direction is wanted is the
      // person's business, so it is reported rather than coloured.
      const moved =
        change === undefined
          ? ""
          : `  ${change >= 0 ? "+" : "-"}${Math.abs(change).toFixed(1)}`;
      weight.textContent =
        today === undefined ? `--${moved}` : `${today.toFixed(1)} kg${moved}`;

      draw(weighings(data));

      if (typeof fields.trouble === "string" && fields.trouble !== "") return;
      const rows = foods(data);
      if (rows.length === 0) {
        say("Nothing logged.", "var(--sw-muted)");
        return;
      }
      list.replaceChildren();
      for (const row of rows) {
        const item = document.createElement("div");
        item.style.display = "flex";
        item.style.justifyContent = "space-between";
        item.style.gap = "10px";

        const what = document.createElement("span");
        what.style.color = "var(--sw-fg)";
        what.style.overflow = "hidden";
        what.style.textOverflow = "ellipsis";
        what.style.whiteSpace = "nowrap";
        what.textContent = row.name;

        const behind = figure();
        behind.style.flex = "0 0 auto";
        behind.style.textTransform = "none";
        behind.textContent = `${grams(row.protein)}/${grams(row.carbs)}/${grams(row.fat)}`;

        item.append(what, behind);
        list.append(item);
      }
    },
    destroy(): void {
      frame.remove();
    },
  };

  // The spawn payload only says which view this is and how far the trend
  // reaches; the backend reports the day.
  return view;
}
