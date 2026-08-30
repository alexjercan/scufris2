// The slice of the Tauri global the pages use. Declared here rather than in any
// one of them because pill.ts, textbox.ts, hud.ts, and form.ts are separate
// classic scripts in one tsc project: a global declaration in one of them is a
// global declaration for the others, which is confusing to read and easy to
// break.

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

// Canonical protocol v5 conversation entry, relayed without reshaping.
interface AttachmentDescriptor {
  id: string;
  name: string;
  media_type: string;
  size: number;
}

interface ConversationEntry {
  role: "user" | "assistant";
  surface: string;
  text: string;
  details?: string;
  widgets?: Array<{ id: string; name: string; arguments: unknown }>;
  attachments?: AttachmentDescriptor[];
}

interface Notice {
  sending: boolean;
  thinking: boolean;
  attachments: AttachmentDescriptor[];
  trouble: string;
}

interface Backlog {
  lines: ConversationEntry[];
  notice: Notice;
}

// The form box's shapes, from src/form.rs. A widget asked for these fields and
// the host bounded them; the page draws what it is given and answers with what
// was typed into it. What the answers mean never reaches this page.
interface FormField {
  name: string;
  label: string;
  value: string;
  lines: number;
  hint: string;
  /** Whether the field offers candidates. What it asks for them with stays
   * with the host: the page sends a field name and what is in it. */
  suggest: boolean;
}

/** One candidate a backend offered, read out of an ordinary reading. */
interface FormChoice {
  id: string;
  label: string;
}

interface FormAsk {
  title: string;
  fields: FormField[];
}
