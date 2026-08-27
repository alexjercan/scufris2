// How much of the Claude subscription is spent, and how long until it is not.
//
// The headline is the window closest to biting rather than a fixed one: a
// session at 90 percent is what stops the next question, and a weekly figure
// beside it would be the calmer of the two numbers and the wrong one to read
// first. Every window is listed under it, so the headline is never the whole
// answer.
//
// Its twin is the `codex` widget. They read the same shape and draw the same
// panel on purpose, because two subscriptions on one desktop should be one
// glance rather than two. They are separate files because a widget module is
// compiled and shipped whole, with nothing to import from a sibling.

/** Above this share of a window, the panel stops being calm about it. */
const WARN = 75;
const ALARM = 90;

interface Window {
  label: string;
  percent: number;
  resets: number | undefined;
}

interface Usage {
  plan: string | undefined;
  windows: Window[];
  error: string | undefined;
}

/** Returns the colour a share of a window is drawn in. */
function shade(percent: number): string {
  if (percent >= ALARM) return "var(--sw-alarm)";
  if (percent >= WARN) return "var(--sw-warn)";
  return "var(--sw-accent)";
}

/** Returns a duration as the two coarsest units that are not zero. */
function left(seconds: number | undefined): string {
  if (seconds === undefined) return "";
  const whole = Math.max(Math.floor(seconds), 0);
  const days = Math.floor(whole / 86400);
  const hours = Math.floor((whole % 86400) / 3600);
  const minutes = Math.floor((whole % 3600) / 60);
  if (days > 0) return `${days}d ${hours}h`;
  if (hours > 0) return `${hours}h ${minutes}m`;
  if (minutes > 0) return `${minutes}m`;
  return "<1m";
}

/** One small uppercase label. */
function caption(): HTMLSpanElement {
  const span = document.createElement("span");
  span.style.fontSize = "var(--sw-size-small)";
  span.style.letterSpacing = "var(--sw-track)";
  span.style.textTransform = "uppercase";
  span.style.color = "var(--sw-muted)";
  return span;
}

export function mount(root: HTMLElement, ctx: WidgetContext): WidgetView {
  // A column the height of the panel, with the meters pushed to the bottom of
  // it. An account with one window and an account with three then hang the
  // same way, instead of one of them leaving a hole under the headline. The
  // shell owns the element this goes in; a widget lays out inside its own.
  const frame = document.createElement("div");
  frame.style.display = "flex";
  frame.style.flexDirection = "column";
  frame.style.height = "100%";

  const head = document.createElement("div");
  head.style.display = "flex";
  head.style.alignItems = "baseline";
  head.style.justifyContent = "space-between";
  head.style.gap = "8px";

  const worst = document.createElement("span");
  worst.style.fontSize = "var(--sw-size-big)";
  worst.style.lineHeight = "1";
  worst.style.fontVariantNumeric = "tabular-nums";
  worst.style.color = "var(--sw-fg)";
  worst.textContent = "--";

  const plan = caption();
  head.append(worst, plan);

  const under = document.createElement("div");
  under.style.display = "flex";
  under.style.alignItems = "center";
  under.style.justifyContent = "space-between";
  under.style.gap = "8px";
  under.style.marginTop = "6px";

  const which = caption();
  which.style.overflow = "hidden";
  which.style.textOverflow = "ellipsis";
  which.style.whiteSpace = "nowrap";

  // `tick` is the shell's own control style, the one class a widget may wear.
  // The poll is slow on purpose, so the one control worth having is the one
  // that says "ask again now" after a long run.
  const again = document.createElement("button");
  again.type = "button";
  again.className = "tick";
  again.textContent = "rfr";
  again.title = "Ask again now";
  again.addEventListener("click", () => {
    ctx.send({ action: "refresh" });
  });

  under.append(which, again);

  const meters = document.createElement("div");
  meters.style.display = "flex";
  meters.style.flexDirection = "column";
  meters.style.gap = "6px";
  meters.style.marginTop = "auto";
  meters.style.paddingTop = "12px";

  frame.append(head, under, meters);
  root.append(frame);

  /** Builds one window's row: what it is, how full it is, and by how much. */
  const meter = (window: Window): HTMLDivElement => {
    const row = document.createElement("div");
    row.style.display = "flex";
    row.style.alignItems = "center";
    row.style.gap = "8px";

    const name = caption();
    name.textContent = window.label;
    name.style.flex = "0 0 72px";
    name.style.overflow = "hidden";
    name.style.textOverflow = "ellipsis";
    name.style.whiteSpace = "nowrap";

    // Square and flat, the way everything else on the panel is. A track that
    // is always drawn says how much is left as plainly as the fill says how
    // much is gone.
    const track = document.createElement("div");
    track.style.flex = "1";
    track.style.height = "8px";
    track.style.background = "var(--sw-line)";

    const fill = document.createElement("div");
    fill.style.height = "100%";
    fill.style.width = `${Math.min(Math.max(window.percent, 0), 100)}%`;
    fill.style.background = shade(window.percent);
    track.append(fill);

    const share = caption();
    share.textContent = `${window.percent.toFixed(0)}%`;
    share.style.flex = "0 0 34px";
    share.style.textAlign = "right";
    share.style.fontVariantNumeric = "tabular-nums";

    row.append(name, track, share);
    return row;
  };

  const read = (data: unknown): Usage | undefined => {
    if (typeof data !== "object" || data === null) return undefined;
    const fields = data as {
      plan?: unknown;
      windows?: unknown;
      error?: unknown;
    };
    const windows: Window[] = [];
    if (Array.isArray(fields.windows)) {
      for (const entry of fields.windows) {
        if (typeof entry !== "object" || entry === null) continue;
        const window = entry as {
          label?: unknown;
          percent?: unknown;
          resets?: unknown;
        };
        if (typeof window.percent !== "number") continue;
        windows.push({
          label: typeof window.label === "string" ? window.label : "limit",
          percent: window.percent,
          resets: typeof window.resets === "number" ? window.resets : undefined,
        });
      }
    }
    return {
      plan: typeof fields.plan === "string" ? fields.plan : undefined,
      windows,
      error: typeof fields.error === "string" ? fields.error : undefined,
    };
  };

  const view: WidgetView = {
    update(data: unknown): void {
      const usage = read(data);
      if (usage === undefined) return;
      plan.textContent = usage.plan ?? "";

      if (usage.windows.length === 0) {
        // A reading with no windows in it is a reading that did not happen.
        // The panel says which rather than freezing on the last good numbers,
        // because a stale percentage looks exactly like a live one.
        worst.textContent = "--";
        worst.style.color = "var(--sw-muted)";
        which.textContent = usage.error ?? "no limits";
        meters.replaceChildren();
        return;
      }

      const highest = usage.windows.reduce((held, window) =>
        window.percent > held.percent ? window : held,
      );
      worst.textContent = `${highest.percent.toFixed(0)}%`;
      worst.style.color = shade(highest.percent);
      const until = left(highest.resets);
      which.textContent =
        until === "" ? highest.label : `${highest.label} - ${until}`;
      meters.replaceChildren(...usage.windows.map(meter));
    },
    destroy(): void {
      frame.remove();
    },
  };

  return view;
}
