/** Protocol v5 agent channel. */

export const SERVICE_VERSION = 5;
export const MAX_MESSAGE_BYTES = 64 * 1024;
export const SOCKET_DIRECTORY_NAME = "scufris";
export const AGENT_FILE_NAME = "agent.sock";
export const CONTENT_FILE_NAME = "content.sock";
export const MAX_IDENTIFIER_LENGTH = 64;
export const MAX_TEXT_BYTES = 8 * 1024;
export const MAX_DETAILS_BYTES = 32 * 1024;
export const MAX_WIDGETS = 32;
export const MAX_WIDGET_ARGUMENTS_BYTES = 16 * 1024;
export const MAX_ATTACHMENTS = 8;
export const MAX_ATTACHMENT_NAME_BYTES = 255;
export const MAX_MEDIA_TYPE_BYTES = 127;
export const MAX_ATTACHMENT_BYTES = 16 * 1024 * 1024;

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

export type AgentRequest =
  | { v: 5; type: "agent.hello" }
  | {
      v: 5;
      type: "agent.response";
      text: string;
      details?: string;
      widgets?: WidgetCall[];
      attachments?: string[];
    }
  | {
      v: 5;
      type: "agent.state";
      state: "failed" | "blocked" | "clear";
      detail: string;
    };

export type AgentResponse =
  | { v: 5; type: "agent.ready" }
  | {
      v: 5;
      type: "agent.message";
      id: string;
      text: string;
      widgets: WidgetDefinition[];
      attachments: AttachmentDescriptor[];
    }
  | { v: 5; type: "agent.abort"; id: string }
  | { v: 5; type: "agent.rejected"; code: string; detail: string };

export class ProtocolError extends Error {
  readonly code: string;

  constructor(message: string, code: string) {
    super(message);
    this.code = code;
  }
}

const identifier = /^[A-Za-z0-9._-]{1,64}$/;
function bounded(value: unknown, maximum: number, field: string): string {
  if (
    typeof value !== "string" ||
    value.length === 0 ||
    Buffer.byteLength(value, "utf8") > maximum
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

export function encodeAgentRequest(message: AgentRequest): string {
  if (message.type === "agent.response") {
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
  if (message.type === "agent.ready") return { v: 5, type: "agent.ready" };
  if (message.type === "agent.abort")
    return { v: 5, type: "agent.abort", id: id(message.id, "id") };
  if (message.type === "agent.rejected")
    return {
      v: 5,
      type: "agent.rejected",
      code: id(message.code, "code"),
      detail: typeof message.detail === "string" ? message.detail : "",
    };
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
      v: 5,
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
