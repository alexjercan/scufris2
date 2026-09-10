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

interface ScufrisMarkup {
  renderPlain(
    parent: HTMLElement,
    source: string,
    open: (url: string) => void,
  ): void;
  renderDetails(
    parent: HTMLElement,
    source: string,
    open: (url: string) => void,
  ): void;
}

interface Window {
  __TAURI__: { core: TauriCore; event: TauriEventModule };
  scufrisMarkup: ScufrisMarkup;
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

// Canonical protocol v10 conversation entry, relayed without reshaping.
interface AttachmentDescriptor {
  id: string;
  name: string;
  media_type: string;
  size: number;
}

/** One measured fact, in the only four words a badge has.
 *
 * `unknown` is not a no: it is a fact nobody could measure, and drawing it as
 * a refusal would invent the one thing the receipt was careful not to claim.
 */
interface ReceiptBadge {
  label: string;
  value: string;
  state: "measured" | "refuted" | "claimed" | "unknown";
}

/** One thing Scufris offers to do next. The words behind it stay with the
 * extension: this page sends the identifier and nothing else. */
interface Offer {
  id: string;
  label: string;
  taken?: boolean;
}

/** Every badge one message carries about one job, led by that job's id. */
interface Citation {
  job_id: string;
  badges?: ReceiptBadge[];
  offers?: Offer[];
}

interface ConversationEntry {
  role: "user" | "assistant";
  surface: string;
  text: string;
  details?: string;
  widgets?: Array<{ id: string; name: string; arguments: unknown }>;
  attachments?: AttachmentDescriptor[];
  receipts?: Citation[];
}

/** One delegated job. A row outlives its job: filing it is what clears it. */
interface JobRow {
  id: string;
  project?: string;
  state: "working" | "blocked" | "done" | "failed";
  /** Unix seconds the job started, which the row shows the age of. */
  since: number;
  summary: string;
}

/** One generation-fenced scheduled briefing. Lifecycle rows are inert. */
interface BriefingRow {
  id: string;
  date: string;
  profile: string;
  collection: "collecting" | "collected" | "failed";
  delivery: "pending" | "in_progress" | "failed" | "delivered";
  since: number;
  completed: number;
  total: number;
  failed: number;
  summary: string;
}

interface Notice {
  sending: boolean;
  thinking: boolean;
  attachments: AttachmentDescriptor[];
  trouble: string;
}

interface Backlog {
  lines: ConversationEntry[];
  jobs: JobRow[];
  briefings: BriefingRow[];
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
