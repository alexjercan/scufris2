import type { AssistantState } from "../shared/assistant-state.ts";

/** Wire protocol version this daemon implements. */
export const PROTOCOL_VERSION = 1;

/** Maximum encoded message size, including its LF terminator. */
export const MAX_MESSAGE_BYTES = 64 * 1024;

/** Directory below XDG_RUNTIME_DIR that holds the daemon socket. */
export const SOCKET_DIRECTORY_NAME = "scufris";

/** Socket name below the socket directory. */
export const SOCKET_FILE_NAME = "daemon.sock";

/** Maximum accepted length of one submission identifier. */
export const MAX_SUBMISSION_ID_LENGTH = 64;

/**
 * Maximum accepted size of one submitted transcript, in UTF-8 bytes.
 *
 * Bytes, not UTF-16 code units: the companion measures the same way, and a
 * divergence would let text be accepted here that the companion's own durable
 * record rejects as corrupt on the next start.
 */
export const MAX_SUBMISSION_TEXT_BYTES = 8 * 1024;

const submissionIdPattern = /^[A-Za-z0-9._-]+$/;

export type ClientMessage =
  | { v: 1; type: "hello" }
  | {
      v: 1;
      type: "submit";
      id: string;
      text: string;
      /**
       * The person's own decision to send words that may already be in the
       * conversation. Absent on every ordinary submission.
       */
      force?: boolean;
    }
  | { v: 1; type: "ping" };

export type DaemonMessage =
  | { v: 1; type: "welcome"; session: string }
  | { v: 1; type: "ack"; id: string }
  | {
      /**
       * The submission was handed to the conversation once already and the
       * daemon cannot say whether it landed. Only the person can decide what
       * happens next, so this is answered to the companion that asked.
       */
      v: 1;
      type: "uncertain";
      id: string;
      detail: string;
    }
  | {
      /**
       * The submission never left the daemon, so the conversation never saw
       * it. The companion may edit these words and retry them ordinarily.
       */
      v: 1;
      type: "refused";
      id: string;
      detail: string;
    }
  | { v: 1; type: "state"; state: AssistantState; detail: string }
  | { v: 1; type: "pong" };

export class ProtocolError extends Error {
  readonly code: string;

  constructor(message: string, code: string) {
    super(message);
    this.code = code;
  }
}

/** Encodes one daemon message as a bounded LF-terminated JSON line. */
export function encodeDaemonMessage(message: DaemonMessage): string {
  const line = `${JSON.stringify(message)}\n`;
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
  if (message.type === "hello") return { v: 1, type: "hello" };
  if (message.type === "ping") return { v: 1, type: "ping" };
  if (message.type === "submit") {
    const { id, text, force } = message;
    if (
      typeof id !== "string" ||
      id.length === 0 ||
      id.length > MAX_SUBMISSION_ID_LENGTH ||
      !submissionIdPattern.test(id)
    ) {
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
      ? { v: 1, type: "submit", id, text, force: true }
      : { v: 1, type: "submit", id, text };
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
