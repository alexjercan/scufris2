// The slice of the Tauri global both pages use. Declared here rather than in
// either page because pill.ts and textbox.ts are separate classic scripts in
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
