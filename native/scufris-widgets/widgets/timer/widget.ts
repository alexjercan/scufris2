// A count going down, and the three things you can do to it.
//
// The first widget that sends as well as receives. Its buttons write one JSON
// action onto the backend's input, the backend answers with the refreshed
// count, and it comes back the way every other reading does - so a timer paused
// from the panel and a timer paused some other way look identical from here.
//
// Clicks, never keys. The window is built unfocusable so a panel landing
// mid-sentence cannot take the keyboard, and a click has never needed focus.

/** How much of the ring one whole count fills. */
const SWEEP = 2 * Math.PI;

/** The ring's own coordinates. */
const SIDE = 96;
const RADIUS = 40;

interface Count {
  left: number;
  of: number;
  running: boolean;
  done: boolean;
}

/** Reads a count as mm:ss, and past an hour as h:mm:ss. */
function clock(seconds: number): string {
  const whole = Math.ceil(Math.max(seconds, 0));
  const hours = Math.floor(whole / 3600);
  const minutes = Math.floor((whole % 3600) / 60);
  const rest = whole % 60;
  const pad = (value: number): string => String(value).padStart(2, "0");
  return hours > 0
    ? `${String(hours)}:${pad(minutes)}:${pad(rest)}`
    : `${pad(minutes)}:${pad(rest)}`;
}

export function mount(root: HTMLElement, ctx: WidgetContext): WidgetView {
  const frame = document.createElement("div");
  frame.style.display = "flex";
  frame.style.alignItems = "center";
  frame.style.gap = "12px";
  frame.style.height = "100%";

  const dial = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  dial.setAttribute("viewBox", `0 0 ${SIDE} ${SIDE}`);
  dial.setAttribute("role", "img");
  dial.setAttribute("aria-label", "Time remaining");
  dial.style.width = "72px";
  dial.style.height = "72px";
  dial.style.flex = "0 0 auto";

  const track = document.createElementNS(
    "http://www.w3.org/2000/svg",
    "circle",
  );
  track.setAttribute("cx", String(SIDE / 2));
  track.setAttribute("cy", String(SIDE / 2));
  track.setAttribute("r", String(RADIUS));
  track.setAttribute("fill", "none");
  track.setAttribute("stroke", "var(--sw-line)");
  track.setAttribute("stroke-width", "3");

  // Drawn from the top and clockwise, the way a clock face is read.
  const arc = document.createElementNS("http://www.w3.org/2000/svg", "circle");
  arc.setAttribute("cx", String(SIDE / 2));
  arc.setAttribute("cy", String(SIDE / 2));
  arc.setAttribute("r", String(RADIUS));
  arc.setAttribute("fill", "none");
  arc.setAttribute("stroke", "var(--sw-accent)");
  arc.setAttribute("stroke-width", "3");
  arc.setAttribute("stroke-linecap", "butt");
  arc.setAttribute("transform", `rotate(-90 ${SIDE / 2} ${SIDE / 2})`);
  const round = SWEEP * RADIUS;
  arc.setAttribute("stroke-dasharray", String(round));

  dial.append(track, arc);

  const beside = document.createElement("div");
  beside.style.display = "flex";
  beside.style.flexDirection = "column";
  beside.style.gap = "8px";
  beside.style.minWidth = "0";

  const left = document.createElement("span");
  left.style.fontSize = "var(--sw-size-big)";
  left.style.fontVariantNumeric = "tabular-nums";
  left.style.color = "var(--sw-fg)";
  left.textContent = "--:--";

  const actions = document.createElement("div");
  actions.style.display = "flex";
  actions.style.gap = "4px";

  // `tick` is the shell's own control style, the one class a widget may wear:
  // the chrome's ticks and a widget's own controls are the same affordance, and
  // a widget that styled its own would be the one that stops matching.
  const control = (label: string, title: string): HTMLButtonElement => {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "tick";
    button.textContent = label;
    button.title = title;
    actions.append(button);
    return button;
  };

  const act = (
    label: string,
    title: string,
    action: Record<string, unknown>,
  ): void => {
    control(label, title).addEventListener("click", () => {
      ctx.send(action);
    });
  };

  // The only control that reads back as well as acts, so it is the only one
  // whose handler is set from the count rather than once at mount.
  const hold = control("||", "Pause");
  act("+1", "Add a minute", { action: "add", seconds: 60 });
  act("rst", "Start over", { action: "reset" });

  beside.append(left, actions);
  frame.append(dial, beside);
  root.append(frame);

  const read = (data: unknown): Count | undefined => {
    if (typeof data !== "object" || data === null) return undefined;
    const fields = data as Partial<Record<keyof Count, unknown>>;
    if (typeof fields.left !== "number" || typeof fields.of !== "number") {
      return undefined;
    }
    return {
      left: fields.left,
      of: fields.of,
      running: fields.running === true,
      done: fields.done === true,
    };
  };

  const view: WidgetView = {
    update(data: unknown): void {
      const count = read(data);
      if (count === undefined) return;
      left.textContent = clock(count.left);
      const share = count.of > 0 ? Math.min(count.left / count.of, 1) : 0;
      arc.setAttribute("stroke-dashoffset", String(round * (1 - share)));
      // The one tick that changes what it says, because it is the one that
      // reads back what the timer is doing rather than only acting on it.
      hold.textContent = count.running ? "||" : ">";
      hold.title = count.running ? "Pause" : "Resume";
      hold.onclick = (): void => {
        ctx.send({ action: count.running ? "pause" : "resume" });
      };
      // Finished timers wear the attention colour rather than announcing
      // themselves: the panel is on screen, and it is already the notice.
      left.style.color = count.done ? "var(--sw-attention)" : "var(--sw-fg)";
      arc.setAttribute(
        "stroke",
        count.done ? "var(--sw-attention)" : "var(--sw-accent)",
      );
    },
    destroy(): void {
      frame.remove();
    },
  };

  // The spawn payload is the length; the backend reports the count. Drawn once
  // from it so the panel opens showing the time asked for rather than dashes.
  const asked = ctx.spawn;
  if (typeof asked === "object" && asked !== null) {
    const seconds = (asked as { seconds?: unknown }).seconds;
    if (typeof seconds === "number") {
      view.update({ left: seconds, of: seconds, running: true, done: false });
    }
  }
  return view;
}
