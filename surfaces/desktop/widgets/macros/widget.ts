// What the day was eaten, weighed and lifted.
//
// One day, three things that belong to it. They share a panel because they
// share a day: what was eaten, what it weighs, and what was lifted are read
// together or not at all, and a second window for the third would be a second
// window to put somewhere.
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

interface Lift {
  index: number;
  split: string;
  exercise: string;
  weight: number;
  reps: number;
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

/** One heading row over a list, with the tick that adds to it. */
function header(
  name: string,
  hint: string,
): [HTMLElement, HTMLSpanElement, HTMLButtonElement] {
  const bar = document.createElement("div");
  bar.style.display = "flex";
  bar.style.alignItems = "center";
  bar.style.justifyContent = "space-between";
  bar.style.gap = "6px";
  bar.style.flex = "0 0 auto";

  const heading = figure();
  heading.textContent = name;
  heading.style.color = "var(--sw-line)";

  // Between the name and the tick, because it is the one figure that says
  // what the list below adds up to.
  const total = figure();
  total.style.flex = "1";
  total.style.textAlign = "right";
  total.style.textTransform = "none";

  const tick = document.createElement("button");
  tick.type = "button";
  tick.className = "tick";
  tick.textContent = "+";
  tick.title = hint;

  bar.append(heading, total, tick);
  return [bar, total, tick];
}

/** A list that scrolls on its own, so one long day does not push out another. */
function column(): HTMLDivElement {
  const held = document.createElement("div");
  held.style.flex = "1";
  held.style.minHeight = "0";
  held.style.overflowY = "auto";
  held.style.display = "flex";
  held.style.flexDirection = "column";
  held.style.gap = "2px";
  return held;
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

  const [bar, eaten, eat] = header("food", "Log a food for this day");
  const list = column();

  const [board, lifted, train] = header("workout", "Log a set for this day");
  const sets = column();

  frame.append(head, split, trend, bar, list, board, sets);
  root.append(frame);

  /** The day on screen, and the weight it carries. Both go into the box the
   * ticks open: the day so the title says which one, the weight so correcting
   * it starts from what is already logged. */
  let showing = "";
  let logged: number | undefined;
  /** The last set written today, so the next one starts from it: a second set
   * of the same movement is a number of reps away, not four fields away. */
  let last3: Lift | undefined;

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

  train.addEventListener("click", () => {
    ctx.ask({
      title: showing === "" ? "Set" : `Set for ${showing}`,
      fields: [
        {
          name: "split",
          label: "Split",
          value: last3?.split ?? "",
          hint: "push, pull, legs",
          // Offered out of the journal rather than a list somebody has to fill
          // in first: the splits worth naming are the ones already trained.
          suggest: { action: "splits" },
        },
        {
          name: "exercise",
          label: "Exercise",
          value: last3?.exercise ?? "",
          hint: "start typing a name",
          suggest: { action: "moves" },
        },
        {
          name: "weight",
          label: "Kilograms",
          value: last3 === undefined ? "" : grams(last3.weight),
          hint: "0 for bodyweight",
        },
        {
          name: "reps",
          label: "Reps",
          value: last3 === undefined ? "" : String(last3.reps),
        },
      ],
      action: { action: "lift" },
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

  const lifts = (data: unknown): Lift[] => {
    const held = (data as { lifts?: unknown }).lifts;
    if (!Array.isArray(held)) return [];
    return held.flatMap((item): Lift[] => {
      if (typeof item !== "object" || item === null) return [];
      const row = item as {
        index?: unknown;
        split?: unknown;
        exercise?: unknown;
        weight?: unknown;
        reps?: unknown;
      };
      if (typeof row.exercise !== "string") return [];
      return [
        {
          index: number(row.index) ?? 0,
          split: typeof row.split === "string" ? row.split : "",
          exercise: row.exercise,
          weight: number(row.weight) ?? 0,
          reps: number(row.reps) ?? 0,
        },
      ];
    });
  };

  /** One row: what was on the left, the numbers on the right. */
  const pair = (what: string, behind: string): HTMLElement => {
    const item = document.createElement("div");
    item.style.display = "flex";
    item.style.justifyContent = "space-between";
    item.style.gap = "10px";

    const left = document.createElement("span");
    left.style.color = "var(--sw-fg)";
    left.style.overflow = "hidden";
    left.style.textOverflow = "ellipsis";
    left.style.whiteSpace = "nowrap";
    left.textContent = what;

    const right = figure();
    right.style.flex = "0 0 auto";
    right.style.textTransform = "none";
    right.textContent = behind;

    item.append(left, right);
    return item;
  };

  /** The day's sets, one row per movement.
   *
   * Grouped rather than listed, because three sets of the same movement are
   * one line of a training log and three lines of a file. The order is the
   * order they were done in, which is the order they were written. */
  const rack = (rows: Lift[]): void => {
    sets.replaceChildren();
    const order: string[] = [];
    const held = new Map<string, Lift[]>();
    for (const row of rows) {
      const key = row.exercise.toLowerCase();
      if (!held.has(key)) {
        held.set(key, []);
        order.push(key);
      }
      held.get(key)?.push(row);
    }
    for (const key of order) {
      const group = held.get(key) ?? [];
      const first = group[0];
      if (first === undefined) continue;
      sets.append(
        pair(
          first.exercise,
          group
            .map((row) => `${grams(row.weight)}x${String(row.reps)}`)
            .join("  "),
        ),
      );
    }
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

      // The split leads the workout total, because a day is judged on which
      // one it was before it is judged on how much was moved.
      const done = lifts(data);
      last3 = done[done.length - 1];
      const volume = number((data as { volume?: unknown }).volume) ?? 0;
      const splits = Array.isArray((data as { splits?: unknown }).splits)
        ? ((data as { splits: unknown[] }).splits.filter(
            (name): name is string => typeof name === "string",
          ) as string[])
        : [];
      lifted.textContent =
        done.length === 0
          ? ""
          : `${splits.join(", ")}  ${grams(volume)} kg`.trim();
      rack(done);
      if (done.length === 0) {
        const empty = figure();
        empty.style.textTransform = "none";
        empty.style.letterSpacing = "normal";
        empty.textContent = "No sets.";
        sets.append(empty);
      }

      if (typeof fields.trouble === "string" && fields.trouble !== "") return;
      const rows = foods(data);
      eaten.textContent = rows.length === 0 ? "" : String(rows.length);
      if (rows.length === 0) {
        say("Nothing logged.", "var(--sw-muted)");
        return;
      }
      list.replaceChildren();
      for (const row of rows) {
        list.append(
          pair(
            row.name,
            `${grams(row.protein)}/${grams(row.carbs)}/${grams(row.fat)}`,
          ),
        );
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
