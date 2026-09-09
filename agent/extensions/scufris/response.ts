import { randomBytes } from "node:crypto";
import type { AssistantMessage } from "@earendil-works/pi-ai";
import { Type } from "@earendil-works/pi-ai";
import { defineTool, type ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { Text } from "@earendil-works/pi-tui";
import {
  JOB_CITATION_EVENT,
  OFFER_TAKE_EVENT,
  type JobCitationSignal,
  type OfferTakeSignal,
  type ReceiptBadge,
} from "./shared/citations.ts";
import { AGENT_RESPONSE_EVENT, type AtomicResponse } from "./service/client.ts";
import {
  MAX_CITATIONS,
  MAX_OFFERS,
  type Citation,
} from "./service/protocol.ts";

export const FINAL_TOOL = "scufris_final_response";
export const RESPONSE_ENTRY = "scufris-response-v5";
export const maxDetailBytes = 32 * 1024;
export const maxResponseBytes = 8 * 1024;
export const finalResponsePolicy =
  "Use scufris_final_response for every final answer. Put mandatory short literal plain prose in text: do not put Markdown headings, lists, emphasis, code fences, inline code, or Markdown links there. Put any formatted explanation in optional Markdown details. Put optional stored attachment IDs in attachments and optional best-effort presentation calls in widgets. Call scufris_final_response as the only tool in the final tool batch. Do not write assistant text before or after it.";
export const offerPolicy =
  "Put an optional next step in offers only when you are reporting on a job this turn and there is one obvious thing the user would ask for next about that job: land it, stop it, open the review. Name the job in job_id, give the button two or three words in label, and write what you would do in prompt as a plain instruction. Never offer a measured fact, a question, or anything the user did not ask about. An offer naming a job this answer carries no measured badges for is dropped.";

/** How many spent-but-not-taken offers stay pressable at once.
 *
 * An offer whose words are gone is a button that does nothing, so the store
 * is what keeps a button honest. It is bounded because a long session would
 * otherwise hold every offer it ever wrote.
 */
export const maxLiveOffers = 64;

/** One thing the model offers to do next about one job. */
export interface OfferDraft {
  job_id: string;
  label: string;
  prompt: string;
}

export interface ResponseEntry extends AtomicResponse {
  version: 5;
  /**
   * The words behind each offer identifier in this message.
   *
   * They never cross the socket: a surface sends the identifier back and this
   * extension knows what it stored against it, so no button can put words
   * into the conversation. They are written here because the identifier
   * outlives the process, and an offer nobody can honour is worse than one
   * that was never drawn.
   */
  offer_prompts?: Record<string, string>;
}

/** The offer prompts a restarted session has to load to honour its buttons. */
export function restoredOfferPrompts(
  entries: Iterable<{ type: string; customType?: string; data?: unknown }>,
): Map<string, string> {
  const prompts = new Map<string, string>();
  for (const entry of entries) {
    if (entry.type !== "custom" || entry.customType !== RESPONSE_ENTRY)
      continue;
    const data = entry.data as Partial<ResponseEntry> | undefined;
    if (data?.version !== 5) continue;
    for (const [id, prompt] of Object.entries(data.offer_prompts ?? {}))
      if (typeof prompt === "string") prompts.set(id, prompt);
  }
  return prompts;
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

function emit(
  pi: ExtensionAPI,
  response: AtomicResponse,
  offerPrompts?: Record<string, string>,
): ResponseEntry {
  const entry: ResponseEntry = {
    version: 5,
    ...response,
    ...(offerPrompts && Object.keys(offerPrompts).length > 0
      ? { offer_prompts: offerPrompts }
      : {}),
  };
  pi.appendEntry(RESPONSE_ENTRY, entry);
  pi.events.emit(AGENT_RESPONSE_EVENT, response);
  return entry;
}

export default function response(pi: ExtensionAPI): void {
  if (process.env.SCUFRIS_ROLE !== "orchestrator") return;
  const prepared = new Map<
    string,
    AtomicResponse & { offers?: OfferDraft[] }
  >();
  // Measured badges waiting for the answer that reports them. Two job events
  // batched into one turn leave two entries here and produce two strips.
  const pending = new Map<string, ReceiptBadge[]>();
  const offerPrompts = new Map<string, string>();

  const rememberOffer = (id: string, prompt: string) => {
    offerPrompts.set(id, prompt);
    while (offerPrompts.size > maxLiveOffers) {
      const oldest = offerPrompts.keys().next();
      if (oldest.done) break;
      offerPrompts.delete(oldest.value);
    }
  };

  /** Binds this turn's badges and offers to the jobs they are about.
   *
   * The identifier does the binding, so no prose is parsed and none is
   * broken. An offer naming a job this message carries no badges for has
   * nowhere to be drawn, and is dropped rather than given a strip of its own.
   */
  const cite = (
    offers: OfferDraft[] = [],
  ): { receipts: Citation[]; prompts: Record<string, string> } => {
    const receipts: Citation[] = [];
    for (const [job_id, badges] of pending) {
      if (receipts.length >= MAX_CITATIONS) break;
      receipts.push({ job_id, badges, offers: [] });
    }
    pending.clear();
    const prompts: Record<string, string> = {};
    for (const offer of offers) {
      const citation = receipts.find((held) => held.job_id === offer.job_id);
      if (!citation || citation.offers.length >= MAX_OFFERS) continue;
      const id = `offer-${randomBytes(6).toString("hex")}`;
      prompts[id] = `for job ${offer.job_id}: ${offer.prompt}`;
      rememberOffer(id, prompts[id]);
      citation.offers.push({ id, label: offer.label });
    }
    return { receipts, prompts };
  };

  const answer = (
    text: AtomicResponse,
    offers?: OfferDraft[],
  ): ResponseEntry => {
    const { receipts, prompts } = cite(offers);
    return emit(
      pi,
      { ...text, ...(receipts.length > 0 ? { receipts } : {}) },
      prompts,
    );
  };

  pi.events.on(JOB_CITATION_EVENT, (value: unknown) => {
    const signal = value as Partial<JobCitationSignal> | undefined;
    if (typeof signal?.job_id !== "string" || !Array.isArray(signal.badges))
      return;
    pending.set(signal.job_id, signal.badges);
  });

  // The offer runs the words this extension stored, not words a surface sent.
  // It arrives as a follow-up rather than as a user message, so nothing in
  // the conversation looks like Alex typed it. The spent badge is the only
  // thing that says what the answer below is answering.
  pi.events.on(OFFER_TAKE_EVENT, (value: unknown) => {
    const signal = value as Partial<OfferTakeSignal> | undefined;
    if (typeof signal?.id !== "string") return;
    const prompt = offerPrompts.get(signal.id);
    if (!prompt) return;
    offerPrompts.delete(signal.id);
    void pi.sendMessage(
      { customType: "scufris-offer", content: prompt, display: true },
      { deliverAs: "followUp", triggerTurn: true },
    );
  });

  pi.on("session_start", (_event, context) => {
    pending.clear();
    offerPrompts.clear();
    for (const [id, prompt] of restoredOfferPrompts(
      context.sessionManager.getBranch(),
    ))
      rememberOffer(id, prompt);
  });

  pi.on("before_agent_start", (event) => ({
    systemPrompt: `${event.systemPrompt}\n\n${finalResponsePolicy}\n\n${offerPolicy}`,
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
      const input = call.arguments as Partial<AtomicResponse> & {
        offers?: OfferDraft[];
      };
      const offered = input.offers;
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
        ...(Array.isArray(offered) ? { offers: offered } : {}),
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
    // Badges are measured, so an answer that arrives as bare assistant text
    // still carries the strip for the job it is about.
    answer({ text });
    return { message: { ...message, content: [{ type: "text", text }] } };
  });

  pi.registerTool(
    defineTool({
      name: FINAL_TOOL,
      label: "Final response",
      description: "Emit one atomic user-visible response and end the turn.",
      promptSnippet:
        "Emit plain text, optional Markdown details, and optional widget calls",
      promptGuidelines: [finalResponsePolicy, offerPolicy],
      executionMode: "sequential",
      renderShell: "self",
      parameters: Type.Object(
        {
          text: Type.String({
            minLength: 1,
            maxLength: maxResponseBytes,
            description: "Short literal plain prose. Do not use Markdown.",
          }),
          details: Type.Optional(
            Type.String({
              minLength: 1,
              maxLength: maxDetailBytes,
              description:
                "Optional Markdown explanation and structured detail.",
            }),
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
          offers: Type.Optional(
            Type.Array(
              Type.Object(
                {
                  job_id: Type.String({ pattern: "^[a-f0-9]{12}$" }),
                  label: Type.String({ minLength: 1, maxLength: 48 }),
                  prompt: Type.String({ minLength: 1, maxLength: 400 }),
                },
                { additionalProperties: false },
              ),
              { maxItems: 4 },
            ),
          ),
        },
        { additionalProperties: false },
      ),
      async execute(toolCallId, params) {
        const { offers, ...response } = prepared.get(toolCallId) ?? {
          text: plainProse(params.text) ?? params.text,
          ...(params.details ? { details: params.details } : {}),
          ...(params.widgets ? { widgets: params.widgets } : {}),
          ...(params.attachments ? { attachments: params.attachments } : {}),
          ...(params.offers ? { offers: params.offers } : {}),
        };
        prepared.delete(toolCallId);
        const entry = answer(response, offers);
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
