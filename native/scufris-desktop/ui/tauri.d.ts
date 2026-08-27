// The slice of the Tauri global both pages use. Declared here rather than in
// either page because pill.ts and review.ts are separate classic scripts in
// one tsc project: a global declaration in one of them is a global
// declaration for the other, which is confusing to read and easy to break.

interface TauriCore {
  invoke(command: string, args?: Record<string, unknown>): Promise<unknown>;
}

interface TauriEventModule {
  listen(
    event: string,
    handler: (event: { payload: unknown }) => void,
  ): Promise<unknown>;
  emitTo(target: string, event: string, payload?: unknown): Promise<void>;
}

interface Window {
  __TAURI__: { core: TauriCore; event: TauriEventModule };
}

// The payload shapes are owned by the Rust side (app.rs); the casts at the
// listen boundaries are the one place the frontend takes them on trust.
interface Presentation {
  state: string;
  detail: string;
  text: string;
  editable: boolean;
  recording: boolean;
}

interface Tick {
  seconds: number;
  level: number;
}

/**
 * What the person has typed into the orb window's field so far.
 *
 * The whole selection travels, not just the caret: the review window draws the
 * selection under the words the same way it draws the caret, and `caret` is the
 * end of it the person is dragging.
 */
interface Draft {
  text: string;
  start: number;
  end: number;
  caret: number;
}
