// The slice of the Tauri global the shell page uses, and the contract between
// the host and the page. Declared here rather than inside shell.ts because the
// page is a classic script: a declaration at its top level would be a global
// anyway, and this is the file that says so out loud.
//
// The contract between the page and the widget it mounts is not here. That one
// is `widgets/widget.d.ts`, which this project reads directly, because it is
// also what every widget is written against and two copies of it would be two
// contracts.
//
// Named for what it holds rather than for the page that reads it. A
// `shell.d.ts` beside `shell.ts` is what tsc would emit as that file's own
// declarations, so it drops it from the project and every name in it goes
// missing.

interface TauriChannel<T> {
  onmessage: (message: T) => void;
}

interface TauriChannelConstructor {
  new <T>(): TauriChannel<T>;
}

interface TauriCore {
  invoke(command: string, args?: Record<string, unknown>): Promise<unknown>;
  Channel: TauriChannelConstructor;
}

interface Window {
  __TAURI__: { core: TauriCore };
}

/** What the host tells one shell window to do. Mirrors `pool::ShellMsg`. */
type ShellMsg =
  | {
      kind: "become";
      surface: string;
      widget: string;
      name: string;
      data: unknown;
    }
  | { kind: "update"; data: unknown }
  | { kind: "life"; state: "live" | "dim" | "pinned" }
  | { kind: "health"; state: "fresh" | "stale" | "dead" }
  | { kind: "refused"; detail: string }
  | { kind: "retire" };
