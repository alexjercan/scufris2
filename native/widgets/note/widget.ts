// The generic "show this" exhibit: a few lines of text and nothing else.
//
// It needs no backend, which is what makes it the widget to check the runtime
// with: everything on screen came from the open or from an update, so what it
// shows is exactly what crossed the socket.

export function mount(root: HTMLElement, ctx: WidgetContext): WidgetView {
  const lines = document.createElement("p");
  lines.className = "lines";
  lines.style.fontSize = "var(--sw-size-body)";
  lines.style.lineHeight = "var(--sw-lead)";
  lines.style.color = "var(--sw-fg)";
  // Long words wrap rather than run off the panel: the window cannot grow, so
  // a line that does not fit is a line the person cannot read.
  lines.style.overflowWrap = "anywhere";
  root.append(lines);

  const read = (data: unknown): string => {
    if (typeof data === "string") return data;
    if (typeof data === "object" && data !== null) {
      const text = (data as { text?: unknown }).text;
      if (typeof text === "string") return text;
    }
    return "";
  };

  const view: WidgetView = {
    update(data: unknown): void {
      lines.textContent = read(data);
    },
    destroy(): void {
      lines.remove();
    },
  };

  view.update(ctx.spawn);
  return view;
}
