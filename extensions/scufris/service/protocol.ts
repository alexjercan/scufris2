/**
 * Version 3 of the Scufris wire protocol, as the agent speaks it.
 *
 * The inversion from version 2 is the whole point: the popup Pi process used to
 * be the server and the companion its client. Now `scufris-service` is the
 * server, and this extension, the desktop companion and `scufris-ctl` are all
 * clients of it. So this file decodes what the service sends and encodes what an
 * agent may say, which is the opposite of what `desktop/protocol.ts` did.
 *
 * The Rust side of the same protocol is `native/scufris-control/src/service.rs`,
 * and the two are meant to be read side by side.
 */

/** Wire protocol version this client speaks. */
export const SERVICE_VERSION = 3;

/** Maximum encoded message size, including its LF terminator. */
export const MAX_MESSAGE_BYTES = 64 * 1024;

/** Directory below XDG_RUNTIME_DIR that holds the service socket. */
export const SOCKET_DIRECTORY_NAME = "scufris";

/** Socket name below the socket directory. */
export const SERVICE_FILE_NAME = "service.sock";

/**
 * Maximum accepted length of one protocol identifier.
 *
 * Correlation, widget, and surface identifiers share one rule, so a peer that
 * can read one can read them all.
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
 * Maximum accepted size of one transcript or spoken line, in UTF-8 bytes.
 *
 * Bytes, not UTF-16 code units: the service measures the same way, and a
 * divergence would let text be accepted here that the service rejects.
 */
export const MAX_TRANSCRIPT_TEXT_BYTES = 4 * 1024;

const identifierPattern = /^[A-Za-z0-9._-]+$/;

/**
 * Returns true for a safe bounded protocol identifier.
 *
 * One rule for correlation, widget, and surface identifiers: a bounded ASCII
 * shape that is also safe as a window label and a file name.
 */
export function isIdentifier(value: unknown): value is string {
  return (
    typeof value === "string" &&
    value.length > 0 &&
    value.length <= MAX_IDENTIFIER_LENGTH &&
    identifierPattern.test(value)
  );
}

export class ProtocolError extends Error {
  readonly code: string;

  constructor(message: string, code: string) {
    super(message);
    this.code = code;
  }
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

/** One installed widget, as the frontend announces it. */
export interface CatalogEntry {
  id: string;
  name: string;
  description: string;
}

/** What the agent asks the frontend to do with a widget. */
export type WidgetCommand =
  | {
      type: "open";
      id: string;
      widget: string;
      posture: Posture;
      data: unknown;
    }
  | { type: "update"; id: string; surface: string; data: unknown }
  | { type: "close"; id: string; surface: string }
  | { type: "clear"; id: string };

/** What the frontend tells the agent about its widgets. */
export type WidgetReport =
  | { type: "opened"; id: string; surface: string }
  | { type: "done"; id: string }
  | { type: "failed"; id: string; code: string; detail: string }
  | { type: "closed"; surface: string }
  | { type: "catalog"; widgets: CatalogEntry[] };

/** What an agent may say to the service. */
export type ClientMessage =
  | { v: 3; type: "hello"; role: "agent" }
  | { v: 3; type: "said"; text: string }
  | { v: 3; type: "speak"; text: string }
  | { v: 3; type: "widget"; command: WidgetCommand }
  | { v: 3; type: "conversation"; id: string; up: boolean };

/** What the service says to an agent that this client acts on. */
export type ServiceMessage =
  | { v: 3; type: "welcome"; role: string }
  | { v: 3; type: "report"; report: WidgetReport }
  | { v: 3; type: "ok"; id: string }
  | { v: 3; type: "refused"; id: string; code: string; detail: string };

/**
 * Answers whether one string carries a surrogate that is not half of a pair.
 *
 * A lone surrogate is not text. `JSON.stringify` writes it out as an escape that
 * no strict decoder will read back, and the decoder at the far end of this
 * socket rejects the connection rather than the message.
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

/** Encodes one client message as a bounded LF-terminated JSON line. */
export function encodeClientMessage(message: ClientMessage): string {
  if (message.type === "said" || message.type === "speak") {
    if (
      message.text.length === 0 ||
      Buffer.byteLength(message.text, "utf8") > MAX_TRANSCRIPT_TEXT_BYTES
    ) {
      throw new ProtocolError(
        `${message.type} requires bounded text`,
        "invalid_text",
      );
    }
  }
  if (message.type === "widget") {
    const command = message.command;
    // Checked before the line is written, not after the service rejects it: an
    // oversized payload is a tool-call error, never a dropped connection.
    if (
      (command.type === "open" || command.type === "update") &&
      Buffer.byteLength(JSON.stringify(command.data) ?? "null", "utf8") >
        MAX_WIDGET_DATA_BYTES
    ) {
      throw new ProtocolError(
        "widget payload is too large",
        "widget_data_too_large",
      );
    }
    // The same rule the service applies, applied here. A widget name or a
    // surface this agent cannot write is one the service would refuse, and
    // refusing it here answers the tool call instead of dropping the link.
    if (command.type === "open") identifier(command.widget, "widget");
    if (command.type === "update" || command.type === "close") {
      identifier(command.surface, "surface");
    }
  }
  const line = `${JSON.stringify(message, wellFormed)}\n`;
  if (Buffer.byteLength(line, "utf8") > MAX_MESSAGE_BYTES) {
    throw new ProtocolError(
      "service message is too large",
      "message_too_large",
    );
  }
  return line;
}

/** Decodes one catalog entry, which types the widget tools the model sees. */
function catalogEntry(value: unknown): CatalogEntry {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new ProtocolError(
      "catalog entries must be objects",
      "invalid_widget",
    );
  }
  const { id, name, description } = value as Record<string, unknown>;
  if (typeof name !== "string" || typeof description !== "string") {
    throw new ProtocolError(
      "catalog entries require a name and a description",
      "invalid_widget",
    );
  }
  return { id: identifier(id, "catalog widget id"), name, description };
}

/** Decodes one widget report, the answer half of the widget relay. */
function widgetReport(value: unknown): WidgetReport {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new ProtocolError("a report must be an object", "invalid_widget");
  }
  const report = value as Record<string, unknown>;
  if (report.type === "opened") {
    return {
      type: "opened",
      id: identifier(report.id, "opened id"),
      surface: identifier(report.surface, "opened surface"),
    };
  }
  if (report.type === "done") {
    return { type: "done", id: identifier(report.id, "done id") };
  }
  if (report.type === "failed") {
    const detail = report.detail;
    return {
      type: "failed",
      id: identifier(report.id, "failed id"),
      code: identifier(report.code, "failed code"),
      detail: typeof detail === "string" ? detail : "",
    };
  }
  if (report.type === "closed") {
    return { type: "closed", surface: identifier(report.surface, "surface") };
  }
  if (report.type === "catalog") {
    if (!Array.isArray(report.widgets)) {
      throw new ProtocolError(
        "catalog requires a widget array",
        "invalid_widget",
      );
    }
    return { type: "catalog", widgets: report.widgets.map(catalogEntry) };
  }
  throw new ProtocolError(
    `unknown widget report ${String(report.type)}`,
    "unknown_type",
  );
}

/**
 * Decodes one service message.
 *
 * Messages this agent has no use for decode to `undefined` rather than
 * throwing. `state`, `transcript` and `speak` are for a surface, and `debug`
 * answers a verb an agent never sends. The agent already has the conversation,
 * so it reads only what is addressed to it, and a service that grows another
 * push must not drop this connection over it.
 *
 * `ok` and `refused` are read because the agent does send one verb the service
 * answers itself: the conversation window.
 */
export function decodeServiceMessage(line: string): ServiceMessage | undefined {
  if (line.endsWith("\r")) {
    throw new ProtocolError(
      "service message must end with LF",
      "invalid_framing",
    );
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(line);
  } catch {
    throw new ProtocolError(
      "service message is not valid JSON",
      "invalid_json",
    );
  }
  if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
    throw new ProtocolError(
      "service message must be an object",
      "invalid_json",
    );
  }
  const message = parsed as Record<string, unknown>;
  if (message.v !== SERVICE_VERSION) {
    throw new ProtocolError(
      `unsupported service protocol version ${String(message.v)}`,
      "unsupported_version",
    );
  }
  if (message.type === "welcome") {
    if (typeof message.role !== "string") {
      throw new ProtocolError("welcome requires a role", "invalid_welcome");
    }
    return { v: 3, type: "welcome", role: message.role };
  }
  if (message.type === "report") {
    return { v: 3, type: "report", report: widgetReport(message.report) };
  }
  if (message.type === "ok") {
    return { v: 3, type: "ok", id: identifier(message.id, "ok id") };
  }
  if (message.type === "refused") {
    const detail = message.detail;
    return {
      v: 3,
      type: "refused",
      id: identifier(message.id, "refused id"),
      code: identifier(message.code, "refused code"),
      detail: typeof detail === "string" ? detail : "",
    };
  }
  return undefined;
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
        "service message is too large",
        "message_too_large",
      );
    }
    lines.push(line);
  }
  if (Buffer.byteLength(rest, "utf8") + 1 > MAX_MESSAGE_BYTES) {
    throw new ProtocolError(
      "service message is too large",
      "message_too_large",
    );
  }
  return { lines, rest };
}
