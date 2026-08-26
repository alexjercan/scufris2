// The whole widget contract, shared by every widget in this directory.
//
// A widget renders into the element the shell hands it and repaints when new
// data arrives. It never draws chrome, never asks who sent the data, and never
// runs on a clock of its own: the shell owns the frame, and WebKitGTK throttles
// a hidden page, so anything that ticks in here stops when the window is put
// away and lies about the time when it comes back.

/** What the shell hands a widget when it mounts it. */
interface WidgetContext {
  /** The spawn payload the open carried. */
  spawn: unknown;
  /** Sends one action back toward whatever is feeding this widget. */
  send(action: unknown): void;
}

/** What a widget hands back, so the shell can drive and unmount it. */
interface WidgetView {
  /** Renders new data. Called with the spawn payload once at mount. */
  update(data: unknown): void;
  /** Releases anything the widget holds. The window closes right after. */
  destroy(): void;
}
