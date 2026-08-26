// Processor load as a line that moves, with memory in use beside it.
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
const WIDE = 240;
const TALL = 56;

interface Reading {
  cpu: number;
  memory: number;
}

export function mount(root: HTMLElement, ctx: WidgetContext): WidgetView {
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

  const used = document.createElement("span");
  used.style.fontSize = "var(--sw-size-small)";
  used.style.letterSpacing = "var(--sw-track)";
  used.style.textTransform = "uppercase";
  used.style.fontVariantNumeric = "tabular-nums";
  used.style.color = "var(--sw-muted)";
  used.textContent = "mem --";

  figures.append(load, used);

  const graph = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  graph.setAttribute("viewBox", `0 0 ${WIDE} ${TALL}`);
  graph.setAttribute("preserveAspectRatio", "none");
  graph.setAttribute("role", "img");
  graph.setAttribute("aria-label", "Processor load over the last minute");
  graph.style.width = "100%";
  graph.style.height = `${TALL}px`;
  graph.style.marginTop = "6px";
  graph.style.display = "block";

  // Under the line, so the shape reads at a glance rather than only the line.
  const fill = document.createElementNS(
    "http://www.w3.org/2000/svg",
    "polygon",
  );
  fill.setAttribute("fill", "var(--sw-accent)");
  fill.setAttribute("opacity", "0.18");

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
  root.append(figures, graph);

  const history: number[] = [];

  const read = (data: unknown): Reading | undefined => {
    if (typeof data !== "object" || data === null) return undefined;
    const fields = data as { cpu?: unknown; memory?: unknown };
    if (typeof fields.cpu !== "number") return undefined;
    return {
      cpu: fields.cpu,
      memory: typeof fields.memory === "number" ? fields.memory : 0,
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
      history.push(reading.cpu);
      if (history.length > SPAN) history.shift();
      draw();
    },
    destroy(): void {
      figures.remove();
      graph.remove();
    },
  };

  // The spawn payload says how often to sample; it carries no reading. The
  // graph stays empty until the backend writes its first line, which is one
  // interval away because a percentage is a difference between two samples.
  void ctx;
  return view;
}
