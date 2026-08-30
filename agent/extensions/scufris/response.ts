import type { AssistantMessage } from "@earendil-works/pi-ai";
import { Type } from "@earendil-works/pi-ai";
import { defineTool, type ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { Text } from "@earendil-works/pi-tui";
import { AGENT_RESPONSE_EVENT, type AtomicResponse } from "./service/client.ts";

export const FINAL_TOOL = "scufris_final_response";
export const RESPONSE_ENTRY = "scufris-response-v5";
export const maxDetailBytes = 32 * 1024;
export const maxResponseBytes = 8 * 1024;
export const finalResponsePolicy =
  "Use scufris_final_response for every final answer. Put mandatory short plain prose in text, optional Markdown in details, optional stored attachment IDs in attachments, and optional best-effort presentation calls in widgets. Call it as the only tool in the final tool batch. Do not write assistant text before or after it.";

export interface ResponseEntry extends AtomicResponse {
  version: 5;
}

export function plainProse(value: string): string | undefined {
  const prose = value.replace(/\s+/g, " ").trim();
  if (
    !prose ||
    Buffer.byteLength(prose, "utf8") > maxResponseBytes ||
    /[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]/.test(prose) ||
    /\n/.test(prose)
  )
    return undefined;
  return prose;
}

function assistantText(message: AssistantMessage): string {
  return message.content
    .filter((item) => item.type === "text")
    .map((item) => item.text)
    .join("")
    .trim();
}

function emit(pi: ExtensionAPI, response: AtomicResponse): ResponseEntry {
  const entry: ResponseEntry = { version: 5, ...response };
  pi.appendEntry(RESPONSE_ENTRY, entry);
  pi.events.emit(AGENT_RESPONSE_EVENT, response);
  return entry;
}

export default function response(pi: ExtensionAPI): void {
  if (process.env.SCUFRIS_ROLE !== "orchestrator") return;
  const prepared = new Map<string, AtomicResponse>();

  pi.on("before_agent_start", (event) => ({
    systemPrompt: `${event.systemPrompt}\n\n${finalResponsePolicy}`,
  }));

  pi.registerMarkdownTransformer((markdown, context) => {
    if (
      context.messageType === "assistant-thinking" ||
      (context.messageType === "assistant" && context.isStreaming)
    )
      return "";
    return markdown;
  });

  pi.registerEntryRenderer<ResponseEntry>(
    RESPONSE_ENTRY,
    (entry, options, theme) => {
      if (entry.data?.version !== 5) return undefined;
      let rendered = entry.data.text;
      if (options.expanded && entry.data.details)
        rendered += `\n\n${entry.data.details}`;
      return new Text(theme.fg("text", rendered), 0, 0);
    },
  );

  pi.on("message_end", (event) => {
    if (event.message.role !== "assistant") return;
    const message = event.message;
    const calls = message.content.filter(
      (
        item,
      ): item is Extract<
        AssistantMessage["content"][number],
        { type: "toolCall" }
      > => item.type === "toolCall",
    );
    const finals = calls.filter((call) => call.name === FINAL_TOOL);
    if (finals.length === 1 && calls.length === 1) {
      const call = finals[0]!;
      const input = call.arguments as Partial<AtomicResponse>;
      const text =
        typeof input.text === "string" ? plainProse(input.text) : undefined;
      if (!text) return;
      prepared.set(call.id, {
        text,
        ...(typeof input.details === "string"
          ? { details: input.details }
          : {}),
        ...(Array.isArray(input.widgets) ? { widgets: input.widgets } : {}),
        ...(Array.isArray(input.attachments)
          ? { attachments: input.attachments }
          : {}),
      });
      return {
        message: {
          ...message,
          content: message.content.map((item) =>
            item === call ? { ...call, arguments: { text } } : item,
          ),
        },
      };
    }
    if (calls.length > 0) {
      return {
        message: {
          ...message,
          content: message.content.filter((item) => item.type !== "text"),
        },
      };
    }
    if (message.stopReason !== "stop") return;
    const text = plainProse(assistantText(message));
    if (!text) return;
    emit(pi, { text });
    return { message: { ...message, content: [{ type: "text", text }] } };
  });

  pi.registerTool(
    defineTool({
      name: FINAL_TOOL,
      label: "Final response",
      description: "Emit one atomic user-visible response and end the turn.",
      promptSnippet:
        "Emit plain text, optional Markdown details, and optional widget calls",
      promptGuidelines: [finalResponsePolicy],
      executionMode: "sequential",
      renderShell: "self",
      parameters: Type.Object(
        {
          text: Type.String({ minLength: 1, maxLength: maxResponseBytes }),
          details: Type.Optional(
            Type.String({ minLength: 1, maxLength: maxDetailBytes }),
          ),
          attachments: Type.Optional(
            Type.Array(Type.String({ minLength: 1, maxLength: 64 }), {
              maxItems: 8,
              uniqueItems: true,
            }),
          ),
          widgets: Type.Optional(
            Type.Array(
              Type.Object(
                {
                  id: Type.String({ minLength: 1, maxLength: 64 }),
                  name: Type.String({ minLength: 1, maxLength: 64 }),
                  arguments: Type.Unknown(),
                },
                { additionalProperties: false },
              ),
              { maxItems: 32 },
            ),
          ),
        },
        { additionalProperties: false },
      ),
      async execute(toolCallId, params) {
        const response = prepared.get(toolCallId) ?? {
          text: plainProse(params.text) ?? params.text,
          ...(params.details ? { details: params.details } : {}),
          ...(params.widgets ? { widgets: params.widgets } : {}),
          ...(params.attachments ? { attachments: params.attachments } : {}),
        };
        prepared.delete(toolCallId);
        const entry = emit(pi, response);
        return {
          content: [{ type: "text", text: "Final response recorded." }],
          details: entry,
          terminate: true,
        };
      },
      renderCall: () => new Text("", 0, 0),
      renderResult: () => new Text("", 0, 0),
    }),
  );
}
