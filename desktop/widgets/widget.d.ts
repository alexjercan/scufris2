// The whole widget contract, and the only copy of it.
//
// Shared by every widget in this directory and by the shell that mounts them:
// `shell/tsconfig.json` reads this same file. Two copies of a contract are two
// contracts, and the day they drift is a day both sides compile and the panel
// breaks in front of the person.
//
// A widget renders into the element the shell hands it and repaints when new
// data arrives. It never draws chrome, never asks who sent the data, and never
// runs on a clock of its own: the shell owns the frame, and WebKitGTK throttles
// a hidden page, so anything that ticks in here stops when the window is put
// away and lies about the time when it comes back.

/** What the shell hands a widget when it mounts it. */
interface WidgetContext {
  /**
   * The spawn payload the open carried.
   *
   * The widget draws its own first frame from this, inside `mount`. It is not
   * handed to `update` afterwards: for some widgets the spawn payload is the
   * data, and for others it is only the request the data answers.
   */
  spawn: unknown;
  /** Sends one action back toward whatever is feeding this widget. */
  send(action: unknown): void;
}

/** What a widget hands back, so the shell can drive and unmount it. */
interface WidgetView {
  /** Renders new data. Never called with the spawn payload. */
  update(data: unknown): void;
  /** Releases anything the widget holds. The window closes right after. */
  destroy(): void;
}

/** The one export every `widget.ts` has. */
interface WidgetModule {
  mount(root: HTMLElement, ctx: WidgetContext): WidgetView;
}
