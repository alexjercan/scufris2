import type { AssistantState } from "../shared/assistant-state.ts";

/** Wire protocol version this daemon implements. */
export const PROTOCOL_VERSION = 2;

/** Maximum encoded message size, including its LF terminator. */
export const MAX_MESSAGE_BYTES = 64 * 1024;

/** Directory below XDG_RUNTIME_DIR that holds the daemon socket. */
export const SOCKET_DIRECTORY_NAME = "scufris";

/** Socket name below the socket directory. */
export const SOCKET_FILE_NAME = "daemon.sock";

/**
 * Maximum accepted length of one protocol identifier.
 *
 * Submission, correlation, widget, and surface identifiers share one rule, so
 * a peer that can read one can read them all.
 */
export const MAX_IDENTIFIER_LENGTH = 64;

/**
 * Maximum accepted size of one encoded widget payload, in UTF-8 bytes.
 *
 * Well below the message cap: the same payload crosses the companion's
 * per-window channel, where small ordered messages are the contract.
 */
export const MAX_WIDGET_DATA_BYTES = 8 * 1024;

/**
 * Maximum accepted size of one submitted transcript, in UTF-8 bytes.
 *
 * Bytes, not UTF-16 code units: the companion measures the same way, and a
 * divergence would let text be accepted here that the companion's own durable
 * record rejects as corrupt on the next start.
 */
export const MAX_SUBMISSION_TEXT_BYTES = 8 * 1024;

const identifierPattern = /^[A-Za-z0-9._-]+$/;

/**
 * Returns true for a safe bounded protocol identifier.
 *
 * One rule for submission, correlation, widget, and surface identifiers: a
 * bounded ASCII shape that is also safe as a window label and a file name.
 */
export function isIdentifier(value: unknown): value is string {
  return (
    typeof value === "string" &&
    value.length > 0 &&
    value.length <= MAX_IDENTIFIER_LENGTH &&
    identifierPattern.test(value)
  );
}

/** Returns the value when it is an identifier, and throws when it is not. */
function identifier(value: unknown, field: string): string {
  if (!isIdentifier(value)) {
    throw new ProtocolError(
      `${field} must be a bounded identifier`,
      "invalid_widget",
    );
  }
  return value;
}

/** Where a surface lives once it is open. */
export type Posture = "exhibit" | "instrument";

/** A change to one surface that this daemon did not ask for. */
export type SurfaceEvent = "closed";

const surfaceEvents: readonly SurfaceEvent[] = ["closed"];

/** One installed widget, as the companion announces it. */
export interface CatalogEntry {
  id: string;
  name: string;
  description: string;
}

export type ClientMessage =
  | { v: 2; type: "hello" }
  | {
      v: 2;
      type: "submit";
      id: string;
      text: string;
      /**
       * The person's own decision to send words that may already be in the
       * conversation. Absent on every ordinary submission.
       */
      force?: boolean;
    }
  | { v: 2; type: "ping" }
  | { v: 2; type: "widget_opened"; id: string; surface: string }
  | { v: 2; type: "widget_done"; id: string }
  | {
      v: 2;
      type: "widget_failed";
      id: string;
      code: string;
      detail?: string;
    }
  | { v: 2; type: "widget_event"; surface: string; event: SurfaceEvent }
  | { v: 2; type: "catalog"; widgets: CatalogEntry[] };

export type DaemonMessage =
  | { v: 2; type: "welcome"; session: string }
  | { v: 2; type: "ack"; id: string }
  | {
      /**
       * The submission was handed to the conversation once already and the
       * daemon cannot say whether it landed. Only the person can decide what
       * happens next, so this is answered to the companion that asked.
       */
      v: 2;
      type: "uncertain";
      id: string;
      detail: string;
    }
  | {
      /**
       * The submission never left the daemon, so the conversation never saw
       * it. The companion may edit these words and retry them ordinarily.
       */
      v: 2;
      type: "refused";
      id: string;
      detail: string;
    }
  | { v: 2; type: "state"; state: AssistantState; detail: string }
  | { v: 2; type: "pong" }
  | {
      /**
       * The first daemon-originated commands. Each carries a correlation id
       * that the companion echoes, so a caller can wait for its own command.
       */
      v: 2;
      type: "widget_open";
      id: string;
      widget: string;
      posture: Posture;
      data: unknown;
    }
  | { v: 2; type: "widget_update"; id: string; surface: string; data: unknown }
  | { v: 2; type: "widget_close"; id: string; surface: string }
  | { v: 2; type: "widget_clear"; id: string };

export class ProtocolError extends Error {
  readonly code: string;

  constructor(message: string, code: string) {
    super(message);
    this.code = code;
  }
}

/**
 * Answers whether one string carries a surrogate that is not half of a pair.
 *
 * A lone surrogate is not text. `JSON.stringify` writes it out as an escape
 * that no strict decoder will read back, and the decoder at the far end of this
 * socket rejects the connection rather than the message: the companion drops
 * the link, backs off, and flashes "backend unavailable" at the person.
 */
function lonely(text: string): boolean {
  for (let index = 0; index < text.length; index += 1) {
    const unit = text.charCodeAt(index);
    if (unit < 0xd800 || unit > 0xdfff) continue;
    // A trailing half with no leading half in front of it.
    if (unit >= 0xdc00) return true;
    const next = text.charCodeAt(index + 1);
    if (!(next >= 0xdc00 && next <= 0xdfff)) return true;
    index += 1;
  }
  return false;
}

/**
 * Refuses any key or string in a message that is not text a decoder reads back.
 *
 * A replacer rather than a walk of our own: it reaches every string
 * `JSON.stringify` would write, including the ones inside a model-supplied
 * widget payload, which is where an unpaired surrogate comes from.
 */
function wellFormed(key: string, value: unknown): unknown {
  if (lonely(key) || (typeof value === "string" && lonely(value))) {
    throw new ProtocolError(
      "a message carries an unpaired surrogate",
      "not_well_formed",
    );
  }
  return value;
}

/** Encodes one daemon message as a bounded LF-terminated JSON line. */
export function encodeDaemonMessage(message: DaemonMessage): string {
  // Checked before the line is written, not after the companion rejects it: an
  // oversized payload is a tool-call error, never a dropped connection.
  if (
    (message.type === "widget_open" || message.type === "widget_update") &&
    Buffer.byteLength(JSON.stringify(message.data) ?? "null", "utf8") >
      MAX_WIDGET_DATA_BYTES
  ) {
    throw new ProtocolError(
      "widget payload is too large",
      "widget_data_too_large",
    );
  }
  // The same rule the decoder applies, applied here. A widget name or a surface
  // this daemon cannot write is one the companion would refuse, and refusing it
  // here answers the tool call instead of spending a round trip on it.
  if (message.type === "widget_open") identifier(message.widget, "widget");
  if (message.type === "widget_update" || message.type === "widget_close") {
    identifier(message.surface, "surface");
  }
  const line = `${JSON.stringify(message, wellFormed)}\n`;
  if (Buffer.byteLength(line, "utf8") > MAX_MESSAGE_BYTES) {
    throw new ProtocolError(
      "control message is too large",
      "message_too_large",
    );
  }
  return line;
}

/**
 * Decodes one companion message. Unknown types, other protocol versions, and
 * out-of-bounds submissions are rejected rather than ignored.
 */
export function decodeClientMessage(line: string): ClientMessage {
  if (line.endsWith("\r")) {
    throw new ProtocolError(
      "control message must end with LF",
      "invalid_framing",
    );
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(line);
  } catch {
    throw new ProtocolError(
      "control message is not valid JSON",
      "invalid_json",
    );
  }
  if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
    throw new ProtocolError(
      "control message must be an object",
      "invalid_json",
    );
  }
  const message = parsed as Record<string, unknown>;
  if (message.v !== PROTOCOL_VERSION) {
    throw new ProtocolError(
      `unsupported control protocol version ${String(message.v)}`,
      "unsupported_version",
    );
  }
  if (message.type === "hello") return { v: 2, type: "hello" };
  if (message.type === "ping") return { v: 2, type: "ping" };
  if (message.type === "submit") {
    const { id, text, force } = message;
    if (!isIdentifier(id)) {
      throw new ProtocolError("submit requires a bounded id", "invalid_submit");
    }
    if (
      typeof text !== "string" ||
      text.trim().length === 0 ||
      Buffer.byteLength(text, "utf8") > MAX_SUBMISSION_TEXT_BYTES ||
      /[\r\0]/.test(text)
    ) {
      throw new ProtocolError(
        "submit requires bounded transcript text",
        "invalid_submit",
      );
    }
    if (force !== undefined && typeof force !== "boolean") {
      throw new ProtocolError(
        "submit force must be a boolean when present",
        "invalid_submit",
      );
    }
    return force === true
      ? { v: 2, type: "submit", id, text, force: true }
      : { v: 2, type: "submit", id, text };
  }
  if (message.type === "widget_opened") {
    return {
      v: 2,
      type: "widget_opened",
      id: identifier(message.id, "widget_opened id"),
      surface: identifier(message.surface, "widget_opened surface"),
    };
  }
  if (message.type === "widget_done") {
    return {
      v: 2,
      type: "widget_done",
      id: identifier(message.id, "widget_done id"),
    };
  }
  if (message.type === "widget_failed") {
    const id = identifier(message.id, "widget_failed id");
    const code = identifier(message.code, "widget_failed code");
    const detail = message.detail;
    if (detail !== undefined && typeof detail !== "string") {
      throw new ProtocolError(
        "widget_failed detail must be a string when present",
        "invalid_widget",
      );
    }
    return detail === undefined
      ? { v: 2, type: "widget_failed", id, code }
      : { v: 2, type: "widget_failed", id, code, detail };
  }
  if (message.type === "widget_event") {
    const surface = identifier(message.surface, "widget_event surface");
    const event = message.event;
    if (!surfaceEvents.includes(event as SurfaceEvent)) {
      throw new ProtocolError(
        `unknown surface event ${String(event)}`,
        "invalid_widget",
      );
    }
    return {
      v: 2,
      type: "widget_event",
      surface,
      event: event as SurfaceEvent,
    };
  }
  if (message.type === "catalog") {
    const widgets = message.widgets;
    if (!Array.isArray(widgets)) {
      throw new ProtocolError(
        "catalog requires a widget array",
        "invalid_widget",
      );
    }
    const entries = widgets.map((entry: unknown): CatalogEntry => {
      if (typeof entry !== "object" || entry === null || Array.isArray(entry)) {
        throw new ProtocolError(
          "catalog entries must be objects",
          "invalid_widget",
        );
      }
      const { id, name, description } = entry as Record<string, unknown>;
      if (typeof name !== "string" || typeof description !== "string") {
        throw new ProtocolError(
          "catalog entries require a name and a description",
          "invalid_widget",
        );
      }
      return { id: identifier(id, "catalog widget id"), name, description };
    });
    return { v: 2, type: "catalog", widgets: entries };
  }
  throw new ProtocolError(
    `unknown control message type ${String(message.type)}`,
    "unknown_type",
  );
}

/**
 * Splits a stream chunk into complete LF-terminated lines and the remaining
 * partial line, rejecting any line that exceeds the message cap.
 */
export function takeLines(buffer: string): { lines: string[]; rest: string } {
  const lines: string[] = [];
  let rest = buffer;
  for (;;) {
    const index = rest.indexOf("\n");
    if (index === -1) break;
    const line = rest.slice(0, index);
    rest = rest.slice(index + 1);
    if (Buffer.byteLength(line, "utf8") + 1 > MAX_MESSAGE_BYTES) {
      throw new ProtocolError(
        "control message is too large",
        "message_too_large",
      );
    }
    lines.push(line);
  }
  if (Buffer.byteLength(rest, "utf8") + 1 > MAX_MESSAGE_BYTES) {
    throw new ProtocolError(
      "control message is too large",
      "message_too_large",
    );
  }
  return { lines, rest };
}
