import { randomBytes } from "node:crypto";
import {
  existsSync,
  lstatSync,
  mkdirSync,
  openSync,
  readFileSync,
  readdirSync,
  statSync,
  unlinkSync,
  writeFileSync,
  closeSync,
} from "node:fs";
import { join } from "node:path";
import type { AssistantMessage } from "@earendil-works/pi-ai";
import { Type } from "@earendil-works/pi-ai";
import {
  defineTool,
  type ExtensionAPI,
  type ExtensionContext,
} from "@earendil-works/pi-coding-agent";
import { Text } from "@earendil-works/pi-tui";
import {
  appendScufrisIdentityPrompt,
  scufrisIdentityPrompt,
} from "../workflow/identity.ts";

export const spokenResponseInstruction =
  "The spoken field must be one short natural prose paragraph in complete sentences without Markdown, paths, hashes, URLs, or code.";
export const FINAL_TOOL = "scufris_final_response";
export const RESPONSE_ENTRY = "scufris-response-v1";
export const REVIEW_ENTRY = "scufris-detail-review-v1";
export const maxDetailBytes = 256 * 1024;
export const maxReviewBytes = 256 * 1024;
export const maxArtifacts = 128;
export const finalResponsePolicy = `${spokenResponseInstruction} Use scufris_final_response for every final answer. Put optional Markdown detail in detail. Call it as the only tool in the final tool batch. Do not write assistant text before or after the call.`;
const fallbackSentence =
  "I could not safely present that response, so I saved it for review.";
const idPattern = /^[a-f0-9]{24}$/;

export function plainProseParagraph(value: string): string | undefined {
  const normalized = value.replace(/\r\n?/g, "\n").trimStart();
  if (!normalized) return undefined;
  const paragraph = normalized.split(/\n[ \t]*\n/, 1)[0] ?? "";
  if (!paragraph || /[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]/.test(paragraph))
    return undefined;
  if (
    paragraph
      .split("\n")
      .some((line) =>
        /^\s*(?:[-+*#>]|\d+[.)]|[\u2022\u2023\u25e6\u2043\u2219])\s/.test(line),
      )
  )
    return undefined;
  const prose = paragraph.replace(/[ \t]*\n[ \t]*/g, " ").trim();
  if (
    !prose ||
    Buffer.byteLength(prose, "utf8") > 1_000 ||
    !/[\p{L}\p{N}]/u.test(prose) ||
    !/[.!?]["')\]]?$/.test(prose) ||
    /[\\/#`*_~|{}<>\[\]]/.test(prose) ||
    /(?:^|\s)(?:https?:|ftp:|www\.)/iu.test(prose) ||
    /\b[A-Za-z0-9-]+\.[A-Za-z][A-Za-z0-9-]{0,15}\b/u.test(prose) ||
    /(?:=>|:=|==|&&|\|\||::)/.test(prose)
  )
    return undefined;
  return prose;
}

export interface ResponseEntry {
  version: 1;
  spoken: string;
  artifact_id?: string;
}

interface ReviewEntry {
  version: 1;
  artifact_id: string;
  outcome: "approved" | "closed" | "feedback";
  annotations: number;
}

interface ArtifactMetadata {
  version: 1;
  artifact_id: string;
  session_id: string;
  created_at: string;
  review?: unknown;
}

interface ArtifactStoreOptions {
  sessionFile: string;
  sessionId: string;
}

function ownUid(): number {
  if (typeof process.getuid !== "function")
    throw new Error("artifact ownership checks are unavailable");
  return process.getuid();
}

function safeRegular(path: string, mode: number): void {
  const value = lstatSync(path);
  if (!value.isFile() || value.isSymbolicLink() || value.uid !== ownUid()) {
    throw new Error("artifact file ownership is invalid");
  }
  if ((value.mode & 0o777) !== mode)
    throw new Error("artifact file mode is invalid");
}

export class ArtifactStore {
  readonly root: string;
  private readonly sessionId: string;

  constructor(options: ArtifactStoreOptions) {
    this.root = `${options.sessionFile}.scufris`;
    this.sessionId = options.sessionId;
    if (!existsSync(this.root)) mkdirSync(this.root, { mode: 0o700 });
    const value = lstatSync(this.root);
    if (
      !value.isDirectory() ||
      value.isSymbolicLink() ||
      value.uid !== ownUid()
    ) {
      throw new Error("artifact sidecar ownership is invalid");
    }
    if ((value.mode & 0o777) !== 0o700) {
      throw new Error("artifact sidecar mode is invalid");
    }
  }

  private path(id: string, suffix: string): string {
    if (!idPattern.test(id)) throw new Error("invalid artifact ID");
    return join(this.root, `${id}${suffix}`);
  }

  create(markdown: string): string {
    if (
      !markdown.trim() ||
      Buffer.byteLength(markdown, "utf8") > maxDetailBytes
    ) {
      throw new Error("detail is empty or exceeds the artifact limit");
    }
    const count = statArtifactIds(this.root).length;
    if (count >= maxArtifacts) throw new Error("artifact count limit reached");
    for (let attempt = 0; attempt < 8; attempt += 1) {
      const id = randomBytes(12).toString("hex");
      const markdownPath = this.path(id, ".md");
      const metadataPath = this.path(id, ".json");
      let descriptor: number | undefined;
      let createdMarkdown = false;
      try {
        descriptor = openSync(markdownPath, "wx", 0o600);
        createdMarkdown = true;
        writeFileSync(descriptor, markdown, "utf8");
        closeSync(descriptor);
        descriptor = undefined;
        writeExclusive(metadataPath, {
          version: 1,
          artifact_id: id,
          session_id: this.sessionId,
          created_at: new Date().toISOString(),
        } satisfies ArtifactMetadata);
        return id;
      } catch (error) {
        if (descriptor !== undefined) closeSync(descriptor);
        if (createdMarkdown) {
          try {
            unlinkSync(markdownPath);
          } catch {
            // Keep the original persistence failure.
          }
        }
        if ((error as NodeJS.ErrnoException).code !== "EEXIST") throw error;
      }
    }
    throw new Error("could not allocate an artifact ID");
  }

  read(id: string): {
    path: string;
    markdown: string;
    metadata: ArtifactMetadata;
  } {
    const markdownPath = this.path(id, ".md");
    const metadataPath = this.path(id, ".json");
    safeRegular(markdownPath, 0o600);
    safeRegular(metadataPath, 0o600);
    if (statSync(metadataPath).size > maxReviewBytes) {
      throw new Error("artifact metadata exceeds the review limit");
    }
    const metadata = JSON.parse(
      readFileSync(metadataPath, "utf8"),
    ) as ArtifactMetadata;
    if (
      metadata.version !== 1 ||
      metadata.artifact_id !== id ||
      metadata.session_id !== this.sessionId
    ) {
      throw new Error("artifact is not owned by the active session");
    }
    const markdown = readFileSync(markdownPath, "utf8");
    if (Buffer.byteLength(markdown, "utf8") > maxDetailBytes) {
      throw new Error("artifact exceeds the detail limit");
    }
    return { path: markdownPath, markdown, metadata };
  }

  recordReview(id: string, review: unknown): void {
    const artifact = this.read(id);
    const path = this.path(id, ".json");
    safeRegular(path, 0o600);
    const serialized = `${JSON.stringify({ ...artifact.metadata, review }, null, 2)}\n`;
    if (Buffer.byteLength(serialized, "utf8") > maxReviewBytes) {
      throw new Error("detail feedback exceeds the review limit");
    }
    writeFileSync(path, serialized, { mode: 0o600 });
    safeRegular(path, 0o600);
  }
}

function writeExclusive(path: string, value: unknown): void {
  const descriptor = openSync(path, "wx", 0o600);
  try {
    writeFileSync(descriptor, `${JSON.stringify(value, null, 2)}\n`, "utf8");
  } finally {
    closeSync(descriptor);
  }
}

function statArtifactIds(root: string): string[] {
  return readdirSync(root)
    .filter((name) => /^[a-f0-9]{24}\.md$/.test(name))
    .map((name) => name.slice(0, -3));
}

export function splitDirectResponse(text: string): {
  spoken: string;
  detail?: string;
} {
  const normalized = text.replace(/\r\n?/g, "\n").trimStart();
  const paragraph = normalized.split(/\n[ \t]*\n/, 1)[0] ?? "";
  const spoken = plainProseParagraph(paragraph);
  if (!spoken) return { spoken: fallbackSentence, detail: normalized || text };
  const detail = normalized.slice(paragraph.length).replace(/^\s+/, "");
  return detail ? { spoken, detail } : { spoken };
}

export function responseText(entry: ResponseEntry): string {
  return entry.artifact_id
    ? `${entry.spoken}\n\n/detail ${entry.artifact_id}`
    : entry.spoken;
}

export function assembleScufrisPrompt(base: string): string {
  return `${appendScufrisIdentityPrompt(base)}\n\n${finalResponsePolicy}`;
}

function assistantText(message: AssistantMessage): string {
  return message.content
    .filter((item) => item.type === "text")
    .map((item) => item.text)
    .join("");
}

function activeStore(context: ExtensionContext): ArtifactStore {
  const sessionFile = context.sessionManager.getSessionFile();
  if (!sessionFile) throw new Error("details require a durable Pi session");
  const file = statSync(sessionFile);
  if (!file.isFile() || file.uid !== ownUid()) {
    throw new Error("session storage ownership is invalid");
  }
  return new ArtifactStore({
    sessionFile,
    sessionId: context.sessionManager.getSessionId(),
  });
}

function prepareResponse(
  context: ExtensionContext,
  spoken: string,
  detail?: string,
): ResponseEntry {
  let artifact_id: string | undefined;
  if (detail) {
    try {
      artifact_id = activeStore(context).create(detail);
    } catch (error) {
      context.ui.notify(
        `Scufris detail was not saved: ${error instanceof Error ? error.message : String(error)}`,
        "error",
      );
    }
  }
  const entry: ResponseEntry = {
    version: 1,
    spoken,
    ...(artifact_id ? { artifact_id } : {}),
  };
  return entry;
}

function appendResponse(
  pi: ExtensionAPI,
  context: ExtensionContext,
  spoken: string,
  detail?: string,
): ResponseEntry {
  const entry = prepareResponse(context, spoken, detail);
  pi.appendEntry(RESPONSE_ENTRY, entry);
  return entry;
}

export function promptInspectionMarkdown(
  effectivePrompt: string,
  options: unknown,
  tools: ReturnType<ExtensionAPI["getAllTools"]>,
): string {
  const provenance = [
    "1. Pi base prompt and custom prompt inputs",
    "2. Context files in Pi order",
    "3. Active tool descriptions and guidelines",
    "4. Loaded skills",
    "5. Embedded canonical Scufris identity policy",
    "6. Scufris final-response policy",
  ];
  return `# Scufris effective system prompt\n\n## Ordered provenance\n\n${provenance.map((item) => `- ${item}`).join("\n")}\n\n## Pi inputs\n\n\`\`\`json\n${JSON.stringify({ systemPromptOptions: options, tools }, null, 2)}\n\`\`\`\n\n## Embedded canonical identity policy\n\n${scufrisIdentityPrompt}\n## Final-response policy\n\n${finalResponsePolicy}\n\n## Exact assembled prompt\n\n\`\`\`text\n${effectivePrompt}\n\`\`\`\n`;
}

export default function response(pi: ExtensionAPI): void {
  if (process.env.SCUFRIS_ROLE !== "orchestrator") return;
  const prepared = new Map<string, ResponseEntry>();
  const appendedResponses = new WeakSet<ResponseEntry>();
  let lastEffectivePrompt: string | undefined;

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
    (entry, _options, theme) => {
      if (entry.data?.version !== 1) return undefined;
      return new Text(theme.fg("text", responseText(entry.data)), 0, 0);
    },
  );
  pi.registerEntryRenderer<ReviewEntry>(
    REVIEW_ENTRY,
    (entry, _options, theme) => {
      const data = entry.data;
      if (data?.version !== 1) return undefined;
      const label = data.outcome;
      return new Text(
        theme.fg(
          data.outcome === "feedback" ? "warning" : "muted",
          `Detail review: ${label}.`,
        ),
        0,
        0,
      );
    },
  );

  pi.on("before_agent_start", (event) => {
    lastEffectivePrompt = `${event.systemPrompt}\n\n${finalResponsePolicy}`;
    return { systemPrompt: lastEffectivePrompt };
  });

  pi.on("message_end", (event, context) => {
    if (event.message.role !== "assistant") return;
    const message = event.message;
    const toolCalls = message.content.filter(
      (
        item,
      ): item is Extract<
        AssistantMessage["content"][number],
        { type: "toolCall" }
      > => item.type === "toolCall",
    );
    const finalCalls = toolCalls.filter(
      (
        item,
      ): item is Extract<
        AssistantMessage["content"][number],
        { type: "toolCall" }
      > => item.type === "toolCall" && item.name === FINAL_TOOL,
    );
    if (finalCalls.length === 1 && toolCalls.length === 1) {
      const call = finalCalls[0]!;
      const input = call.arguments as { spoken?: unknown; detail?: unknown };
      const inputSpoken =
        typeof input.spoken === "string" ? input.spoken : undefined;
      const validated = inputSpoken
        ? plainProseParagraph(inputSpoken)
        : undefined;
      const valid = validated && validated === inputSpoken?.trim();
      const spoken = valid ? validated : fallbackSentence;
      const detail = valid
        ? typeof input.detail === "string" && input.detail
          ? input.detail
          : undefined
        : `# Rejected final response\n\n${typeof input.spoken === "string" ? input.spoken : "Missing spoken response."}\n\n${typeof input.detail === "string" ? input.detail : ""}`;
      const entry = prepareResponse(context, spoken, detail);
      prepared.set(call.id, entry);
      return {
        message: {
          ...message,
          content: message.content.map((item) =>
            item === call
              ? {
                  ...call,
                  arguments: { spoken },
                }
              : item,
          ),
        },
      };
    }
    if (finalCalls.length > 0) {
      const detail = `# Rejected final tool batch\n\n\`\`\`json\n${JSON.stringify(
        finalCalls.map((call) => call.arguments),
        null,
        2,
      )}\n\`\`\``;
      const entry = prepareResponse(context, fallbackSentence, detail);
      for (const call of finalCalls) prepared.set(call.id, entry);
      return {
        message: {
          ...message,
          content: message.content
            .filter((item) => item.type !== "text")
            .map((item) =>
              item.type === "toolCall" && item.name === FINAL_TOOL
                ? {
                    ...item,
                    arguments: { spoken: fallbackSentence },
                  }
                : item,
            ),
        },
      };
    }
    if (toolCalls.length > 0) {
      return {
        message: {
          ...message,
          content: message.content.filter((item) => item.type !== "text"),
        },
      };
    }
    if (message.stopReason !== "stop") return;
    const split = splitDirectResponse(assistantText(message));
    const entry = prepareResponse(context, split.spoken, split.detail);
    return {
      message: {
        ...message,
        content: [{ type: "text", text: responseText(entry) }],
      },
    };
  });

  pi.registerTool(
    defineTool({
      name: FINAL_TOOL,
      label: "Final response",
      description:
        "Emit the only user-visible final response and terminate the turn.",
      promptSnippet:
        "Emit the final spoken paragraph and optional private Markdown detail",
      promptGuidelines: [finalResponsePolicy],
      executionMode: "sequential",
      renderShell: "self",
      parameters: Type.Object(
        {
          spoken: Type.String({ minLength: 1, maxLength: 1000 }),
          detail: Type.Optional(
            Type.String({ minLength: 1, maxLength: maxDetailBytes }),
          ),
        },
        { additionalProperties: false },
      ),
      async execute(toolCallId, params, _signal, _update, context) {
        let entry = prepared.get(toolCallId);
        prepared.delete(toolCallId);
        if (!entry) {
          const spoken = plainProseParagraph(params.spoken);
          if (!spoken || spoken !== params.spoken.trim())
            throw new Error("spoken must be complete safe plain prose");
          entry = appendResponse(pi, context, spoken, params.detail);
        } else if (!appendedResponses.has(entry)) {
          pi.appendEntry(RESPONSE_ENTRY, entry);
          appendedResponses.add(entry);
        }
        return {
          content: [{ type: "text", text: "Final response recorded." }],
          details: entry,
          terminate: true,
        };
      },
      renderCall() {
        return new Text("", 0, 0);
      },
      renderResult() {
        return new Text("", 0, 0);
      },
    }),
  );

  pi.registerCommand("detail", {
    description: "Open a private Scufris detail artifact in Plannotator.",
    handler: async (args, context) => {
      const id = args.trim();
      if (!idPattern.test(id)) {
        context.ui.notify(
          "Use /detail with an artifact ID from this session.",
          "warning",
        );
        return;
      }
      let artifact: ReturnType<ArtifactStore["read"]>;
      try {
        artifact = activeStore(context).read(id);
      } catch (error) {
        context.ui.notify(
          error instanceof Error ? error.message : String(error),
          "error",
        );
        return;
      }
      const requestId = `scufris-detail-${randomBytes(8).toString("hex")}`;
      pi.events.emit("plannotator:request", {
        requestId,
        action: "annotate",
        payload: {
          filePath: artifact.path,
          markdown: artifact.markdown,
          gate: true,
        },
        respond: (response: unknown) => {
          const value = response as {
            status?: string;
            result?: { feedback?: unknown; approved?: unknown; exit?: unknown };
          };
          if (value?.status !== "handled" || !value.result) {
            context.ui.notify("Detail review was unavailable.", "error");
            return;
          }
          const feedback =
            typeof value.result.feedback === "string"
              ? value.result.feedback.trim()
              : "";
          const annotations = 0;
          const outcome: ReviewEntry["outcome"] = feedback
            ? "feedback"
            : value.result.approved === true
              ? "approved"
              : "closed";
          try {
            activeStore(context).recordReview(id, value.result);
          } catch (error) {
            context.ui.notify(
              `Detail feedback was not saved: ${error instanceof Error ? error.message : String(error)}`,
              "error",
            );
            return;
          }
          pi.appendEntry(REVIEW_ENTRY, {
            version: 1,
            artifact_id: id,
            outcome,
            annotations,
          } satisfies ReviewEntry);
          if (feedback) {
            const content = `The user reviewed detail artifact ${id}. Address this private structured feedback: ${JSON.stringify(value.result)}`;
            if (Buffer.byteLength(content, "utf8") > 16 * 1024) {
              context.ui.notify(
                "Detail feedback was saved but exceeds the mediation limit.",
                "error",
              );
              return;
            }
            pi.sendMessage(
              {
                customType: "scufris-detail-feedback",
                content,
                display: false,
                details: { artifact_id: id },
              },
              { deliverAs: "followUp", triggerTurn: true },
            );
          }
        },
      });
    },
  });

  pi.registerCommand("scufris-prompt", {
    description:
      "Inspect the private effective Scufris prompt without a provider request.",
    handler: async (_args, context) => {
      try {
        const detail = promptInspectionMarkdown(
          lastEffectivePrompt ??
            assembleScufrisPrompt(context.getSystemPrompt()),
          context.getSystemPromptOptions(),
          pi
            .getAllTools()
            .filter((tool) => pi.getActiveTools().includes(tool.name)),
        );
        appendResponse(
          pi,
          context,
          "The private prompt inspection is ready.",
          detail,
        );
      } catch (error) {
        context.ui.notify(
          error instanceof Error ? error.message : String(error),
          "error",
        );
      }
    },
  });
}
