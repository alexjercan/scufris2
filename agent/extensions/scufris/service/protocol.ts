/** Protocol v7 agent channel. */

export const SERVICE_VERSION = 7;
export const MAX_MESSAGE_BYTES = 64 * 1024;
export const SOCKET_DIRECTORY_NAME = "scufris";
export const AGENT_FILE_NAME = "agent.sock";
export const CONTENT_FILE_NAME = "content.sock";
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

export type AgentRequest =
  | { v: 7; type: "agent.hello" }
  | {
      v: 7;
      type: "agent.response";
      text: string;
      details?: string;
      widgets?: WidgetCall[];
      attachments?: string[];
      receipts?: Citation[];
    }
  | { v: 7; type: "agent.jobs"; jobs: JobRow[] };

export type AgentResponse =
  | { v: 7; type: "agent.ready" }
  | {
      v: 7;
      type: "agent.message";
      id: string;
      text: string;
      widgets: WidgetDefinition[];
      attachments: AttachmentDescriptor[];
    }
  | {
      v: 7;
      type: "agent.wake";
      custom_type: string;
      text: string;
      details?: unknown;
    }
  | { v: 7; type: "agent.abort"; id: string }
  | { v: 7; type: "agent.job_command"; id: string; action: JobAction }
  | { v: 7; type: "agent.offer_take"; id: string }
  | { v: 7; type: "agent.rejected"; code: string; detail: string };

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
  INVALID_WIDGETS: "invalid_widgets",
  WIDGET_NOT_FOUND: "widget_not_found",
  SURFACE_NOT_FOUND: "surface_not_found",
  NO_FREE_SLOT: "no_free_slot",
  NOT_SHOWN: "not_shown",
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

export function encodeAgentRequest(message: AgentRequest): string {
  if (message.type === "agent.jobs") checkJobRows(message.jobs);
  if (message.type === "agent.response") {
    if (message.receipts) checkCitations(message.receipts);
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
      throw new ProtocolError("too many widget calls", "invalid_widgets");
    for (const call of message.widgets ?? []) {
      id(call.id, "widget_id");
      id(call.name, "widget_name");
      if (
        Buffer.byteLength(safeStringify(call.arguments), "utf8") >
        MAX_WIDGET_ARGUMENTS_BYTES
      )
        throw new ProtocolError(
          "widget arguments are too large",
          "invalid_widgets",
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
  if (message.type === "agent.ready") return { v: 7, type: "agent.ready" };
  if (message.type === "agent.abort")
    return { v: 7, type: "agent.abort", id: id(message.id, "id") };
  if (message.type === "agent.rejected")
    return {
      v: 7,
      type: "agent.rejected",
      code: id(message.code, "code"),
      detail: typeof message.detail === "string" ? message.detail : "",
    };
  if (message.type === "agent.job_command") {
    // A surface named a row and a verb. What stopping costs and what filing
    // means are both decided here, because this is what owns the job.
    if (message.action !== "cancel" && message.action !== "archive")
      throw new ProtocolError("invalid job action", "invalid_action");
    return {
      v: 7,
      type: "agent.job_command",
      id: id(message.id, "id"),
      action: message.action,
    };
  }
  if (message.type === "agent.offer_take")
    return { v: 7, type: "agent.offer_take", id: id(message.id, "id") };
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
      v: 7,
      type: "agent.wake",
      custom_type: id(message.custom_type, "custom_type"),
      text: bounded(message.text, MAX_TEXT_BYTES, "text"),
      ...(details === undefined ? {} : { details }),
    };
  }
  if (message.type === "agent.message") {
    if (!Array.isArray(message.widgets) || message.widgets.length > MAX_WIDGETS)
      throw new ProtocolError("invalid widgets", "invalid_widgets");
    const widgets = message.widgets.map((entry) => {
      if (typeof entry !== "object" || entry === null || Array.isArray(entry))
        throw new ProtocolError("invalid widget", "invalid_widgets");
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
      v: 7,
      type: "agent.message",
      id: id(message.id, "id"),
      text: bounded(message.text, MAX_TEXT_BYTES, "text"),
      widgets,
      attachments,
    };
  }
  throw new ProtocolError("unknown agent message", "unknown_type");
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
