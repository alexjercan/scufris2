// What this machine is doing: load as a line that moves, and the three numbers
// worth reading beside it.
//
// The first widget with a backend behind it. Everything on screen came from a
// line the `system` backend wrote, which is what makes this the widget to check
// the supervisor with: a graph that stops moving is a backend that stopped
// writing, and the chrome says which.
//
// The history is the widget's, not the backend's. A backend hands over one
// reading and knows nothing about who is drawing it, so a second panel opened
// on the same sampler starts its own line rather than inheriting one.

/** How many readings the graph holds. At a one second interval, most a minute. */
const SPAN = 48;

/** The drawing's own coordinates. CSS scales it to whatever the panel is. */
const WIDE = 300;
const TALL = 78;

/** Degrees Celsius at which the temperature stops being worth ignoring. */
const HOT = 85;

interface Reading {
  cpu: number;
  memory: number;
  temperature: number | undefined;
  load: number | undefined;
}

/** One small uppercase figure in the row under the graph. */
function figure(): HTMLSpanElement {
  const span = document.createElement("span");
  span.style.fontSize = "var(--sw-size-small)";
  span.style.letterSpacing = "var(--sw-track)";
  span.style.textTransform = "uppercase";
  span.style.fontVariantNumeric = "tabular-nums";
  span.style.color = "var(--sw-muted)";
  return span;
}

export function mount(root: HTMLElement, ctx: WidgetContext): WidgetView {
  // A column the height of the panel, so the graph takes whatever room the
  // figures above and below it leave rather than a height fixed here. The
  // shell owns the element this goes in; a widget lays out inside its own.
  const frame = document.createElement("div");
  frame.style.display = "flex";
  frame.style.flexDirection = "column";
  frame.style.height = "100%";

  const figures = document.createElement("div");
  figures.style.display = "flex";
  figures.style.alignItems = "baseline";
  figures.style.justifyContent = "space-between";
  figures.style.gap = "8px";

  const load = document.createElement("span");
  load.style.fontSize = "var(--sw-size-big)";
  load.style.fontVariantNumeric = "tabular-nums";
  load.style.color = "var(--sw-fg)";
  load.textContent = "--";

  // Beside the headline rather than in the row below, because a processor that
  // is hot is the one number here that is worth interrupting a glance for.
  const heat = document.createElement("span");
  heat.style.fontSize = "var(--sw-size-body)";
  heat.style.letterSpacing = "var(--sw-track)";
  heat.style.fontVariantNumeric = "tabular-nums";
  heat.style.color = "var(--sw-muted)";
  heat.textContent = "--";

  figures.append(load, heat);

  const graph = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  graph.setAttribute("viewBox", `0 0 ${WIDE} ${TALL}`);
  graph.setAttribute("preserveAspectRatio", "none");
  graph.setAttribute("role", "img");
  graph.setAttribute("aria-label", "Processor load over the last minute");
  graph.style.width = "100%";
  graph.style.flex = "1";
  graph.style.minHeight = "0";
  graph.style.margin = "8px 0";
  graph.style.display = "block";

  // Under the line, so the shape reads at a glance rather than only the line.
  const fill = document.createElementNS(
    "http://www.w3.org/2000/svg",
    "polygon",
  );
  fill.setAttribute("fill", "var(--sw-accent)");
  fill.setAttribute("opacity", "0.22");

  const line = document.createElementNS(
    "http://www.w3.org/2000/svg",
    "polyline",
  );
  line.setAttribute("fill", "none");
  line.setAttribute("stroke", "var(--sw-accent)");
  line.setAttribute("stroke-width", "1.5");
  line.setAttribute("stroke-linejoin", "round");
  line.setAttribute("vector-effect", "non-scaling-stroke");

  graph.append(fill, line);

  const row = document.createElement("div");
  row.style.display = "flex";
  row.style.justifyContent = "space-between";
  row.style.gap = "8px";

  const used = figure();
  used.textContent = "mem --";
  const average = figure();
  average.textContent = "load --";
  row.append(used, average);

  frame.append(figures, graph, row);
  root.append(frame);

  const history: number[] = [];

  const read = (data: unknown): Reading | undefined => {
    if (typeof data !== "object" || data === null) return undefined;
    const fields = data as {
      cpu?: unknown;
      memory?: unknown;
      temperature?: unknown;
      load?: unknown;
    };
    if (typeof fields.cpu !== "number") return undefined;
    return {
      cpu: fields.cpu,
      memory: typeof fields.memory === "number" ? fields.memory : 0,
      // Absent rather than zero: a machine that reports no temperature is not
      // a machine at zero degrees, and the panel says so with a dash.
      temperature:
        typeof fields.temperature === "number" ? fields.temperature : undefined,
      load: typeof fields.load === "number" ? fields.load : undefined,
    };
  };

  const draw = (): void => {
    if (history.length === 0) return;
    // Anchored to the right, so the newest reading is always at the same edge
    // and a graph that is still filling grows toward the left rather than
    // stretching what is already drawn.
    const step = WIDE / (SPAN - 1);
    const points = history.map((value, index) => {
      const at = WIDE - (history.length - 1 - index) * step;
      const height = TALL - (Math.min(Math.max(value, 0), 100) / 100) * TALL;
      return `${at.toFixed(1)},${height.toFixed(1)}`;
    });
    line.setAttribute("points", points.join(" "));
    const first = WIDE - (history.length - 1) * step;
    fill.setAttribute(
      "points",
      `${first.toFixed(1)},${TALL} ${points.join(" ")} ${WIDE},${TALL}`,
    );
  };

  const view: WidgetView = {
    update(data: unknown): void {
      const reading = read(data);
      if (reading === undefined) return;
      load.textContent = `${reading.cpu.toFixed(0)}%`;
      used.textContent = `mem ${reading.memory.toFixed(0)}%`;
      average.textContent =
        reading.load === undefined
          ? "load --"
          : `load ${reading.load.toFixed(2)}`;
      if (reading.temperature === undefined) {
        heat.textContent = "--";
        heat.style.color = "var(--sw-muted)";
      } else {
        heat.textContent = `${reading.temperature.toFixed(0)}°C`;
        heat.style.color =
          reading.temperature >= HOT ? "var(--sw-warn)" : "var(--sw-muted)";
      }
      history.push(reading.cpu);
      if (history.length > SPAN) history.shift();
      draw();
    },
    destroy(): void {
      frame.remove();
    },
  };

  // The spawn payload says how often to sample; it carries no reading. The
  // graph stays empty until the backend writes its first line, which is one
  // interval away because a percentage is a difference between two samples.
  void ctx;
  return view;
}
