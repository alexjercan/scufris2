/** Protocol v11 agent channel. */

export const SERVICE_VERSION = 11;
export const MAX_MESSAGE_BYTES = 64 * 1024;
export const SOCKET_DIRECTORY_NAME = "scufris";
export const AGENT_FILE_NAME = "agent.sock";
export const CONTENT_FILE_NAME = "content.sock";
export const CONTROL_FILE_NAME = "control.sock";
export const MAX_IDENTIFIER_LENGTH = 64;
export const MAX_TEXT_BYTES = 8 * 1024;
export const MAX_DETAILS_BYTES = 32 * 1024;
export const MAX_DETAIL_BYTES = 4 * 1024;
export const MAX_WIDGETS = 32;
export const MAX_WIDGET_ARGUMENTS_BYTES = 16 * 1024;
export const MAX_ATTACHMENTS = 8;
export const MAX_ATTACHMENT_NAME_BYTES = 255;
export const MAX_MEDIA_TYPE_BYTES = 127;
export const MAX_ATTACHMENT_BYTES = 16 * 1024 * 1024;
export const MAX_CITATIONS = 4;
export const MAX_RECEIPTS = 6;
export const MAX_OFFERS = 2;
export const MAX_BADGE_BYTES = 64;
export const MAX_JOB_ROWS = 8;
export const MAX_JOB_SUMMARY_BYTES = 512;
export const MAX_BRIEFING_ROWS = 64;
export const MAX_BRIEFING_SUMMARY_BYTES = 256;
export const MAX_CONVERSATION_PAGE = 64;
export const MAX_TURN_IMAGES = 8;
export const MAX_PATH_BYTES = 4 * 1024;

/** The surface name a terminal's words and answers are recorded under. */
export const TERMINAL_SURFACE = "terminal";
/** The job owner token one holder at a time carries. */
export const FOREGROUND_OWNER = "foreground";

export interface WidgetDefinition {
  name: string;
  description: string;
  input_schema: unknown;
}

export interface WidgetCall {
  id: string;
  name: string;
  arguments: unknown;
}

export interface AttachmentDescriptor {
  id: string;
  name: string;
  media_type: string;
  size: number;
}

/** What one measured fact says, in the only four words a badge has.
 *
 * `unknown` is not a no. A fetch that failed leaves `pushed` unmeasured, and
 * drawing that as "not pushed" would invent the one fact the receipt was
 * careful not to claim.
 */
export type ReceiptState = "measured" | "refuted" | "claimed" | "unknown";

export interface ReceiptBadge {
  label: string;
  value: string;
  state: ReceiptState;
}

/** One thing Scufris offers to do next about one job.
 *
 * The prompt behind the words never crosses the socket. A surface sends the
 * identifier back and this extension knows what it stored against it, so no
 * button can put words into the conversation.
 */
export interface Offer {
  id: string;
  label: string;
}

/** Every badge one message carries about one job. */
export interface Citation {
  job_id: string;
  badges: ReceiptBadge[];
  offers: Offer[];
}

export type JobRowState = "working" | "blocked" | "done" | "failed";

/** One delegated job, as the surfaces draw it. */
export interface JobRow {
  id: string;
  project?: string;
  state: JobRowState;
  /** Unix seconds the job started, for the age the row shows. */
  since: number;
  summary: string;
}

export type JobAction = "cancel" | "archive";

export type BriefingCollectionState = "collecting" | "collected" | "failed";
export type BriefingDeliveryState =
  | "pending"
  | "in_progress"
  | "failed"
  | "delivered";

export interface BriefingRow {
  id: string;
  date: string;
  profile: string;
  collection: BriefingCollectionState;
  delivery: BriefingDeliveryState;
  since: number;
  completed: number;
  total: number;
  failed: number;
  summary: string;
}

/** Which process is the agent right now. */
export type AgentHolder = "managed" | "terminal";

/**
 * The Pi session one agent is writing.
 *
 * `parent` is what a fork writes in its own header. It is how the service
 * tells an agent that already has the conversation from one that only has a
 * new file, and so whether catch-up is worth sending.
 */
export interface AgentSession {
  id: string;
  file: string;
  cwd: string;
  parent?: string;
}

/** One canonical conversation entry, text only. */
export interface ConversationEntry {
  sequence: number;
  role: "user" | "assistant";
  surface: string;
  text: string;
}

export type AgentRequest =
  | { v: 11; type: "agent.hello"; lease?: number; session?: AgentSession }
  | {
      v: 11;
      type: "agent.session";
      id: string;
      file: string;
      cwd: string;
      parent?: string;
    }
  | { v: 11; type: "agent.turn"; id: string; text: string; images?: number }
  | { v: 11; type: "agent.activity"; working: boolean }
  | { v: 11; type: "agent.proactive_started"; proactive_id: string }
  | { v: 11; type: "agent.proactive_settled"; proactive_id: string }
  | {
      v: 11;
      type: "agent.response";
      text: string;
      turn_id?: string;
      proactive_id?: string;
      details?: string;
      widgets?: WidgetCall[];
      attachments?: string[];
      receipts?: Citation[];
    }
  | { v: 11; type: "agent.jobs"; jobs: JobRow[] };

export type AgentResponse =
  | { v: 11; type: "agent.ready" }
  | {
      v: 11;
      type: "agent.message";
      id: string;
      text: string;
      widgets: WidgetDefinition[];
      attachments: AttachmentDescriptor[];
    }
  | {
      v: 11;
      type: "agent.wake";
      proactive_id?: string;
      custom_type: string;
      text: string;
      details?: unknown;
    }
  | { v: 11; type: "agent.abort"; id: string }
  | { v: 11; type: "agent.turn_ack"; id: string; sequence: number }
  | { v: 11; type: "agent.handoff"; generation: number; next: AgentHolder }
  | {
      v: 11;
      type: "agent.catch_up";
      since: number;
      entries: ConversationEntry[];
    }
  | { v: 11; type: "agent.job_command"; id: string; action: JobAction }
  | { v: 11; type: "agent.offer_take"; id: string }
  | {
      v: 11;
      type: "agent.rejected";
      id?: string;
      code: string;
      detail: string;
    };

/** The state the service computes, in the order it prefers. */
export type ScufrisState =
  | "failed"
  | "blocked"
  | "working"
  | "starting"
  | "idle";

/**
 * What a terminal says about itself when it asks for the lease.
 *
 * None of it is trusted with anything. The pid names who took the agent in the
 * log, and the paths are recorded and forked from, never run.
 */
export interface LeaseHolderInfo {
  pid: number;
  session_file?: string;
  cwd: string;
}

/**
 * The control channel, which is local only.
 *
 * It never crosses the surface gateway, so nothing here is reachable from the
 * phone. A terminal uses it to take the agent, to say it is still there, and to
 * read what it missed.
 */
export type ControlRequest =
  | { v: 11; type: "control.hello" }
  | { v: 11; type: "control.state"; id: string }
  | {
      v: 11;
      type: "control.lease_acquire";
      id: string;
      holder: LeaseHolderInfo;
      abort_working?: boolean;
    }
  | { v: 11; type: "control.lease_ping"; id: string }
  | { v: 11; type: "control.lease_release"; id: string }
  | { v: 11; type: "control.conversation"; id: string; since: number };

export type ControlResponse =
  | { v: 11; type: "control.ready" }
  | {
      v: 11;
      type: "control.state";
      id: string;
      state: ScufrisState;
      detail: string;
      holder?: AgentHolder;
      generation?: number;
      session_dir: string;
      lineage_file?: string;
    }
  | {
      v: 11;
      type: "control.lease";
      id: string;
      generation: number;
      session_dir: string;
      lineage_file?: string;
      sequence: number;
      owner: string;
    }
  | { v: 11; type: "control.lease_pong"; id: string; generation: number }
  | { v: 11; type: "control.lease_released"; id: string }
  | {
      v: 11;
      type: "control.conversation_entries";
      id: string;
      entries: ConversationEntry[];
      more?: boolean;
    }
  | {
      v: 11;
      type: "control.rejected";
      id: string;
      code: string;
      detail: string;
    };

/**
 * Every stable refusal code, mirroring `shared/control/src/refusal.rs`.
 *
 * A refusal crosses the socket as a bare string in `code`, and nothing checks
 * it: a service that sends `attachments_unavailable` and a client that matches
 * `attachment_unavailable` both run, and what breaks is the behaviour that
 * depended on the match. Naming them here is what turns a typo into a
 * type error on this side.
 *
 * These are the wire codes only. `ProtocolError` also carries codes that never
 * leave this process - `invalid_json`, `invalid_framing`, and the rest - which
 * are this extension's own vocabulary for `tell()` and are not part of the
 * protocol.
 *
 * `tests/service.test.ts` reads the Rust module and holds the two together.
 */
export const REFUSAL = {
  DUPLICATE_HELLO: "duplicate_hello",
  AGENT_EXISTS: "agent_exists",
  AGENT_UNAVAILABLE: "agent_unavailable",
  ATTACHMENTS_UNAVAILABLE: "attachments_unavailable",
  ATTACHMENT_TOO_LARGE: "attachment_too_large",
  INVALID_ATTACHMENT: "invalid_attachment",
  ATTACHMENT_INCOMPLETE: "attachment_incomplete",
  ATTACHMENT_NOT_FOUND: "attachment_not_found",
  ATTACHMENT_QUOTA: "attachment_quota",
  ATTACHMENT_UNAVAILABLE: "attachment_unavailable",
  INVALID_RANGE: "invalid_range",
  OFFER_UNAVAILABLE: "offer_unavailable",
  BRIEFING_UNAVAILABLE: "briefing_unavailable",
  BRIEFING_NOT_DISMISSIBLE: "briefing_not_dismissible",
  BRIEFING_DISMISSAL_FAILED: "briefing_dismissal_failed",
  INVALID_WIDGETS: "invalid_widgets",
  WIDGET_NOT_FOUND: "widget_not_found",
  SURFACE_NOT_FOUND: "surface_not_found",
  NO_FREE_SLOT: "no_free_slot",
  NOT_SHOWN: "not_shown",
  LEASE_DISABLED: "lease_disabled",
  LEASE_HELD: "lease_held",
  LEASE_REQUIRED: "lease_required",
  NOT_LEASE_HOLDER: "not_lease_holder",
  AGENT_BUSY: "agent_busy",
  LEASE_PING_STALE: "lease_ping_stale",
} as const;

export class ProtocolError extends Error {
  readonly code: string;

  constructor(message: string, code: string) {
    super(message);
    this.code = code;
  }
}

const identifier = /^[A-Za-z0-9._-]{1,64}$/;
// The host's `text()` in shared/control/src/service.rs is the authority, and
// these are its rules. A value this side lets through and the host refuses is
// not a refusal Alex ever sees: the host breaks the agent connection on an
// invalid submission, so the answer in flight is lost and the channel simply
// reconnects. Refusing here turns that into a ProtocolError that `tell()`
// catches and reports.
function bounded(
  value: unknown,
  maximum: number,
  field: string,
  allowEmpty = false,
): string {
  if (
    typeof value !== "string" ||
    (!allowEmpty && value.trim().length === 0) ||
    Buffer.byteLength(value, "utf8") > maximum ||
    value.includes("\0") ||
    value.includes("\r")
  )
    throw new ProtocolError(`${field} is invalid`, `invalid_${field}`);
  return value;
}
function id(value: unknown, field: string): string {
  if (typeof value !== "string" || !identifier.test(value))
    throw new ProtocolError(`${field} is invalid`, `invalid_${field}`);
  return value;
}
/** A protocol counter: a non-negative integer the runtime can hold exactly. */
function sequence(value: unknown, field: string): number {
  if (typeof value !== "number" || !Number.isSafeInteger(value) || value < 0)
    throw new ProtocolError(`${field} is invalid`, `invalid_${field}`);
  return value;
}
const STATES: readonly ScufrisState[] = [
  "failed",
  "blocked",
  "working",
  "starting",
  "idle",
];

/** A lease generation, which only ever counts up from one. */
function generation(value: unknown): number {
  if (typeof value !== "number" || !Number.isSafeInteger(value) || value < 1)
    throw new ProtocolError("generation is invalid", "invalid_generation");
  return value;
}

export function decodeHolder(value: unknown): AgentHolder {
  if (value !== "managed" && value !== "terminal")
    throw new ProtocolError("holder is invalid", "invalid_holder");
  return value;
}

/** An absolute path, recorded and forked from and never run. */
export function decodePath(value: unknown, field: string): string {
  const path = bounded(value, MAX_PATH_BYTES, field);
  if (!path.startsWith("/") || path.includes("\n"))
    throw new ProtocolError(`${field} is invalid`, `invalid_${field}`);
  return path;
}

export function decodeConversationEntry(value: unknown): ConversationEntry {
  if (typeof value !== "object" || value === null || Array.isArray(value))
    throw new ProtocolError("invalid conversation entry", "invalid_entries");
  const entry = value as Record<string, unknown>;
  if (entry.role !== "user" && entry.role !== "assistant")
    throw new ProtocolError("invalid conversation role", "invalid_entries");
  return {
    sequence: sequence(entry.sequence, "sequence"),
    role: entry.role,
    surface: id(entry.surface, "surface"),
    text: bounded(entry.text, MAX_TEXT_BYTES, "text", true),
  };
}

export function decodeAttachmentDescriptor(
  value: unknown,
): AttachmentDescriptor {
  if (typeof value !== "object" || value === null || Array.isArray(value))
    throw new ProtocolError("invalid attachment", "invalid_attachments");
  const attachment = value as Record<string, unknown>;
  const keys = Object.keys(attachment).sort();
  if (keys.join(",") !== "id,media_type,name,size")
    throw new ProtocolError("invalid attachment", "invalid_attachments");
  const name = bounded(
    attachment.name,
    MAX_ATTACHMENT_NAME_BYTES,
    "attachment_name",
  );
  const mediaType = bounded(
    attachment.media_type,
    MAX_MEDIA_TYPE_BYTES,
    "attachment_media_type",
  );
  if (
    /[\\/\x00-\x1f\x7f]/.test(name) ||
    !/^[A-Za-z0-9!#$&^_.+-]+\/[A-Za-z0-9!#$&^_.+-]+$/.test(mediaType) ||
    typeof attachment.size !== "number" ||
    !Number.isSafeInteger(attachment.size) ||
    attachment.size < 1 ||
    attachment.size > MAX_ATTACHMENT_BYTES
  )
    throw new ProtocolError("invalid attachment", "invalid_attachments");
  return {
    id: id(attachment.id, "attachment_id"),
    name,
    media_type: mediaType,
    size: attachment.size,
  };
}

function safeStringify(value: unknown): string {
  const line = JSON.stringify(value, (_key, item) => {
    if (typeof item === "string" && /[\ud800-\udfff]/u.test(item)) {
      // Reject lone surrogates but retain well-formed pairs.
      for (let index = 0; index < item.length; index += 1) {
        const unit = item.charCodeAt(index);
        if (unit < 0xd800 || unit > 0xdfff) continue;
        if (unit >= 0xdc00)
          throw new ProtocolError("unpaired surrogate", "not_well_formed");
        const next = item.charCodeAt(++index);
        if (next < 0xdc00 || next > 0xdfff)
          throw new ProtocolError("unpaired surrogate", "not_well_formed");
      }
    }
    return item;
  });
  if (line === undefined)
    throw new ProtocolError("message is not JSON", "invalid_json");
  return line;
}

/**
 * Clamps one job row's summary to what the host accepts instead of losing it.
 *
 * A summary is a caught error message or a worker's captured output, so its
 * length and its control characters are not ours to choose. Refusing the whole
 * row list would leave the surfaces showing nothing while a job is failed,
 * which is the wrong half to keep.
 */
export function jobSummary(value: string): string {
  const clean = value.replace(/[\0\r]/g, " ");
  const bytes = Buffer.from(clean, "utf8");
  if (bytes.length <= MAX_JOB_SUMMARY_BYTES) return clean;
  // Cutting bytes can split a codepoint; the tail decodes to U+FFFD.
  return bytes
    .subarray(0, MAX_JOB_SUMMARY_BYTES)
    .toString("utf8")
    .replace(/�+$/, "");
}

/** Checks one message's citations against the bounds the host enforces.
 *
 * Nothing here is about the receipt. Every bound is on presentation: a strip
 * is read at a glance, and a message citing five jobs is not one.
 */
function checkCitations(citations: Citation[]): void {
  if (citations.length > MAX_CITATIONS)
    throw new ProtocolError("too many citations", "invalid_receipts");
  const jobs = new Set<string>();
  const offers = new Set<string>();
  for (const citation of citations) {
    const job = id(citation.job_id, "job_id");
    if (jobs.has(job))
      throw new ProtocolError("duplicate citation", "invalid_receipts");
    jobs.add(job);
    if (
      citation.badges.length > MAX_RECEIPTS ||
      citation.offers.length > MAX_OFFERS
    )
      throw new ProtocolError("citation is too large", "invalid_receipts");
    for (const badge of citation.badges) {
      bounded(badge.label, MAX_BADGE_BYTES, "receipt_label");
      bounded(badge.value, MAX_BADGE_BYTES, "receipt_value");
    }
    for (const offer of citation.offers) {
      const offerId = id(offer.id, "offer_id");
      // The identifier is all a surface sends back, so it names one offer in
      // the whole message and not one inside its own group.
      if (offers.has(offerId))
        throw new ProtocolError("duplicate offer", "invalid_offers");
      offers.add(offerId);
      bounded(offer.label, MAX_BADGE_BYTES, "offer_label");
    }
  }
}

function checkJobRows(rows: JobRow[]): void {
  if (rows.length > MAX_JOB_ROWS)
    throw new ProtocolError("too many job rows", "invalid_jobs");
  const seen = new Set<string>();
  for (const row of rows) {
    const rowId = id(row.id, "job_id");
    if (seen.has(rowId))
      throw new ProtocolError("duplicate job row", "invalid_jobs");
    seen.add(rowId);
    // A project identifier is a relative path, so it carries slashes an
    // identifier never may. It is drawn, not resolved.
    if (row.project !== undefined)
      bounded(row.project, MAX_IDENTIFIER_LENGTH, "job_project");
    // A summary is built from a worker's own output, which is neither
    // length-bounded nor stripped of control characters at its source.
    bounded(row.summary, MAX_JOB_SUMMARY_BYTES, "job_summary", true);
  }
}

/** The session bounds the host holds, applied before the line is written. */
function checkSession(session: {
  id: string;
  file: string;
  cwd: string;
  parent?: string;
}): void {
  bounded(session.id, MAX_IDENTIFIER_LENGTH, "session_id");
  decodePath(session.file, "session_file");
  decodePath(session.cwd, "session_cwd");
  if (session.parent !== undefined)
    decodePath(session.parent, "session_parent");
}

/**
 * Clamps words a terminal has already shown to what the host will record.
 *
 * The turn or the answer is already in Pi and already on the screen; this copy
 * exists so the HUD shows it too. Something past the bound is cut rather than
 * refused, because refusing would leave the conversation with no record of
 * what everyone in the room already read.
 */
export function recordedText(value: string): string {
  const clean = value.replace(/[\0\r]/g, " ");
  const bytes = Buffer.from(clean, "utf8");
  if (bytes.length <= MAX_TEXT_BYTES) return clean;
  return bytes
    .subarray(0, MAX_TEXT_BYTES)
    .toString("utf8")
    .replace(/\uFFFD+$/, "");
}

export function encodeAgentRequest(message: AgentRequest): string {
  if (message.type === "agent.jobs") checkJobRows(message.jobs);
  if (
    message.type === "agent.proactive_started" ||
    message.type === "agent.proactive_settled"
  )
    id(message.proactive_id, "proactive_id");
  if (message.type === "agent.hello") {
    // A generation is a fence, and a fence that is not a real generation is
    // the one thing this must never send: the host would refuse the hello and
    // the terminal would look like a broken agent instead of an unfenced one.
    if (message.lease !== undefined) generation(message.lease);
    if (message.session) checkSession(message.session);
  }
  if (message.type === "agent.session") checkSession(message);
  if (message.type === "agent.turn") {
    id(message.id, "turn_id");
    bounded(message.text, MAX_TEXT_BYTES, "text");
    if (
      message.images !== undefined &&
      (!Number.isSafeInteger(message.images) ||
        message.images < 0 ||
        message.images > MAX_TURN_IMAGES)
    )
      throw new ProtocolError("too many images", "invalid_images");
  }
  if (message.type === "agent.response") {
    if (message.receipts) checkCitations(message.receipts);
    if (message.turn_id !== undefined) id(message.turn_id, "turn_id");
    if (message.proactive_id !== undefined)
      id(message.proactive_id, "proactive_id");
    bounded(message.text, MAX_TEXT_BYTES, "text");
    if ((message.attachments?.length ?? 0) > MAX_ATTACHMENTS)
      throw new ProtocolError("too many attachments", "invalid_attachments");
    const attachmentIds = new Set<string>();
    for (const attachment of message.attachments ?? []) {
      const attachmentId = id(attachment, "attachment_id");
      if (attachmentIds.has(attachmentId))
        throw new ProtocolError("duplicate attachment", "invalid_attachments");
      attachmentIds.add(attachmentId);
    }
    if (message.details !== undefined)
      bounded(message.details, MAX_DETAILS_BYTES, "details");
    if ((message.widgets?.length ?? 0) > MAX_WIDGETS)
      throw new ProtocolError("too many widget calls", REFUSAL.INVALID_WIDGETS);
    for (const call of message.widgets ?? []) {
      id(call.id, "widget_id");
      id(call.name, "widget_name");
      if (
        Buffer.byteLength(safeStringify(call.arguments), "utf8") >
        MAX_WIDGET_ARGUMENTS_BYTES
      )
        throw new ProtocolError(
          "widget arguments are too large",
          REFUSAL.INVALID_WIDGETS,
        );
    }
  }
  const line = `${safeStringify(message)}\n`;
  if (Buffer.byteLength(line, "utf8") > MAX_MESSAGE_BYTES)
    throw new ProtocolError("message is too large", "message_too_large");
  return line;
}

export function decodeAgentResponse(line: string): AgentResponse {
  if (line.endsWith("\r"))
    throw new ProtocolError("invalid framing", "invalid_framing");
  let value: unknown;
  try {
    value = JSON.parse(line);
  } catch {
    throw new ProtocolError("invalid JSON", "invalid_json");
  }
  if (typeof value !== "object" || value === null || Array.isArray(value))
    throw new ProtocolError("message is not an object", "invalid_json");
  const message = value as Record<string, unknown>;
  if (message.v !== SERVICE_VERSION)
    throw new ProtocolError(
      "unsupported protocol version",
      "unsupported_version",
    );
  if (message.type === "agent.ready") return { v: 11, type: "agent.ready" };
  if (message.type === "agent.abort")
    return { v: 11, type: "agent.abort", id: id(message.id, "id") };
  if (message.type === "agent.rejected")
    return {
      v: 11,
      type: "agent.rejected",
      ...(message.id === undefined ? {} : { id: id(message.id, "id") }),
      code: id(message.code, "code"),
      detail: typeof message.detail === "string" ? message.detail : "",
    };
  if (message.type === "agent.turn_ack")
    return {
      v: 11,
      type: "agent.turn_ack",
      id: id(message.id, "id"),
      sequence: sequence(message.sequence, "sequence"),
    };
  if (message.type === "agent.handoff")
    return {
      v: 11,
      type: "agent.handoff",
      generation: generation(message.generation),
      next: decodeHolder(message.next),
    };
  if (message.type === "agent.catch_up") {
    if (
      !Array.isArray(message.entries) ||
      message.entries.length > MAX_CONVERSATION_PAGE
    )
      throw new ProtocolError("invalid catch-up", "invalid_entries");
    return {
      v: 11,
      type: "agent.catch_up",
      since: sequence(message.since, "since"),
      entries: message.entries.map(decodeConversationEntry),
    };
  }
  if (message.type === "agent.job_command") {
    // A surface named a row and a verb. What stopping costs and what filing
    // means are both decided here, because this is what owns the job.
    if (message.action !== "cancel" && message.action !== "archive")
      throw new ProtocolError("invalid job action", "invalid_action");
    return {
      v: 11,
      type: "agent.job_command",
      id: id(message.id, "id"),
      action: message.action,
    };
  }
  if (message.type === "agent.offer_take")
    return { v: 11, type: "agent.offer_take", id: id(message.id, "id") };
  if (message.type === "agent.wake") {
    // A wake is words from outside the agent process, carried under the
    // caller's own custom type so an existing wake handler still matches.
    const details = message.details;
    if (details !== undefined) {
      if (
        typeof details !== "object" ||
        details === null ||
        Array.isArray(details) ||
        Buffer.byteLength(safeStringify(details), "utf8") > MAX_DETAILS_BYTES
      )
        throw new ProtocolError("invalid wake details", "invalid_details");
    }
    return {
      v: 11,
      type: "agent.wake",
      ...(message.proactive_id === undefined
        ? {}
        : { proactive_id: id(message.proactive_id, "proactive_id") }),
      custom_type: id(message.custom_type, "custom_type"),
      text: bounded(message.text, MAX_TEXT_BYTES, "text"),
      ...(details === undefined ? {} : { details }),
    };
  }
  if (message.type === "agent.message") {
    if (!Array.isArray(message.widgets) || message.widgets.length > MAX_WIDGETS)
      throw new ProtocolError("invalid widgets", REFUSAL.INVALID_WIDGETS);
    const widgets = message.widgets.map((entry) => {
      if (typeof entry !== "object" || entry === null || Array.isArray(entry))
        throw new ProtocolError("invalid widget", REFUSAL.INVALID_WIDGETS);
      const widget = entry as Record<string, unknown>;
      return {
        name: id(widget.name, "widget_name"),
        description:
          typeof widget.description === "string" ? widget.description : "",
        input_schema: widget.input_schema,
      };
    });
    const attachmentValues = message.attachments ?? [];
    if (
      !Array.isArray(attachmentValues) ||
      attachmentValues.length > MAX_ATTACHMENTS
    )
      throw new ProtocolError("invalid attachments", "invalid_attachments");
    const attachments = attachmentValues.map(decodeAttachmentDescriptor);
    if (
      new Set(attachments.map((attachment) => attachment.id)).size !==
      attachments.length
    )
      throw new ProtocolError("duplicate attachment", "invalid_attachments");
    return {
      v: 11,
      type: "agent.message",
      id: id(message.id, "id"),
      text: bounded(message.text, MAX_TEXT_BYTES, "text"),
      widgets,
      attachments,
    };
  }
  throw new ProtocolError("unknown agent message", "unknown_type");
}

export function encodeControlRequest(message: ControlRequest): string {
  if (message.type !== "control.hello") id(message.id, "id");
  if (message.type === "control.lease_acquire") {
    const holder = message.holder;
    if (!Number.isSafeInteger(holder.pid) || holder.pid < 1)
      throw new ProtocolError("pid is invalid", "invalid_pid");
    decodePath(holder.cwd, "cwd");
    if (holder.session_file !== undefined)
      decodePath(holder.session_file, "session_file");
  }
  if (message.type === "control.conversation") sequence(message.since, "since");
  const line = `${safeStringify(message)}\n`;
  if (Buffer.byteLength(line, "utf8") > MAX_MESSAGE_BYTES)
    throw new ProtocolError("message is too large", "message_too_large");
  return line;
}

export function decodeControlResponse(line: string): ControlResponse {
  if (line.endsWith("\r"))
    throw new ProtocolError("invalid framing", "invalid_framing");
  let value: unknown;
  try {
    value = JSON.parse(line);
  } catch {
    throw new ProtocolError("invalid JSON", "invalid_json");
  }
  if (typeof value !== "object" || value === null || Array.isArray(value))
    throw new ProtocolError("message is not an object", "invalid_json");
  const message = value as Record<string, unknown>;
  if (message.v !== SERVICE_VERSION)
    throw new ProtocolError(
      "unsupported protocol version",
      "unsupported_version",
    );
  if (message.type === "control.ready") return { v: 11, type: "control.ready" };
  if (message.type === "control.lease_released")
    return { v: 11, type: "control.lease_released", id: id(message.id, "id") };
  if (message.type === "control.lease_pong")
    return {
      v: 11,
      type: "control.lease_pong",
      id: id(message.id, "id"),
      generation: generation(message.generation),
    };
  if (message.type === "control.rejected")
    return {
      v: 11,
      type: "control.rejected",
      id: id(message.id, "id"),
      code: id(message.code, "code"),
      detail: typeof message.detail === "string" ? message.detail : "",
    };
  if (message.type === "control.state") {
    if (!STATES.includes(message.state as ScufrisState))
      throw new ProtocolError("invalid state", "invalid_state");
    return {
      v: 11,
      type: "control.state",
      id: id(message.id, "id"),
      state: message.state as ScufrisState,
      detail: typeof message.detail === "string" ? message.detail : "",
      ...(message.holder === undefined
        ? {}
        : { holder: decodeHolder(message.holder) }),
      ...(message.generation === undefined
        ? {}
        : { generation: generation(message.generation) }),
      session_dir: decodePath(message.session_dir, "session_dir"),
      ...(message.lineage_file === undefined
        ? {}
        : { lineage_file: decodePath(message.lineage_file, "lineage_file") }),
    };
  }
  if (message.type === "control.lease")
    return {
      v: 11,
      type: "control.lease",
      id: id(message.id, "id"),
      generation: generation(message.generation),
      session_dir: decodePath(message.session_dir, "session_dir"),
      ...(message.lineage_file === undefined
        ? {}
        : { lineage_file: decodePath(message.lineage_file, "lineage_file") }),
      sequence: sequence(message.sequence, "sequence"),
      owner: id(message.owner, "owner"),
    };
  if (message.type === "control.conversation_entries") {
    if (!Array.isArray(message.entries))
      throw new ProtocolError("entries is invalid", "invalid_entries");
    if (message.entries.length > MAX_CONVERSATION_PAGE)
      throw new ProtocolError("too many entries", "invalid_entries");
    return {
      v: 11,
      type: "control.conversation_entries",
      id: id(message.id, "id"),
      entries: message.entries.map(decodeConversationEntry),
      ...(message.more === undefined ? {} : { more: message.more === true }),
    };
  }
  throw new ProtocolError("unknown control message", "unknown_type");
}

export function takeLines(buffer: string): { lines: string[]; rest: string } {
  const parts = buffer.split("\n");
  const rest = parts.pop() ?? "";
  for (const line of [...parts, rest]) {
    if (Buffer.byteLength(line, "utf8") + 1 > MAX_MESSAGE_BYTES)
      throw new ProtocolError("message is too large", "message_too_large");
  }
  return { lines: parts, rest };
}

/** Deterministic, XML-safe, self-contained Pi user message. */
export function surfacePrompt(
  text: string,
  widgets: WidgetDefinition[],
  attachments: AttachmentDescriptor[],
): string {
  const ordered = widgets.map((widget) => ({
    name: widget.name,
    description: widget.description,
    input_schema: widget.input_schema,
  }));
  const escapeXml = (json: string) =>
    json
      .replaceAll("&", "\\u0026")
      .replaceAll("<", "\\u003c")
      .replaceAll(">", "\\u003e");
  return `<scufris_surface_message>\n<widgets>\n${escapeXml(safeStringify(ordered))}\n</widgets>\n<attachments>\n${escapeXml(safeStringify(attachments))}\n</attachments>\n<user_message>\n${escapeXml(safeStringify(text))}\n</user_message>\n</scufris_surface_message>`;
}
