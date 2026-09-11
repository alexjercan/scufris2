import type { AssistantMessage } from "@earendil-works/pi-ai";
import {
  AssistantMessageComponent,
  CustomMessageComponent,
  ToolExecutionComponent,
  type ExtensionAPI,
  type ExtensionContext,
} from "@earendil-works/pi-coding-agent";
const calmStateKey = Symbol.for("scufris:calm-state:v1");
const calmPatchKey = Symbol.for("scufris:calm-patches:v1");
const calmStateType = "scufris-calm-state-v1";
const hiddenCustomTypes = new Set(["scufris-job-event", "scufris-briefing"]);

/** The envelope `service/protocol.ts` wraps a message from another surface in.
 *
 * The shape is exact because the sender writes it in one place: the widgets
 * the agent may draw with, the attachments it may ask for, and the person's
 * words, each JSON encoded so no payload can carry a `<`.
 */
const SURFACE_MESSAGE =
  /^<scufris_surface_message>\n<widgets>\n[\s\S]*?\n<\/widgets>\n<attachments>\n([\s\S]*?)\n<\/attachments>\n<user_message>\n([\s\S]*?)\n<\/user_message>\n<\/scufris_surface_message>$/;

type CalmState = { enabled: boolean };

interface CalmStateEntry {
  version: 1;
  enabled: boolean;
}

type CalmGlobals = typeof globalThis & {
  [calmStateKey]?: CalmState;
  [calmPatchKey]?: true;
};

type AssistantMessageState = {
  lastMessage?: AssistantMessage;
};

type CustomMessageState = {
  message?: { customType?: string };
};

/** What a message from a phone or a panel reads as on a screen.
 *
 * Four thousand characters of widget schema around one sentence is what the
 * model is given, and it is the right thing to give it. A person reading the
 * same conversation in a terminal is given the sentence, and the attachments
 * by name because those are content somebody sent. Display only: Pi keeps the
 * envelope in the session and in the model's context, so `/calm off` and the
 * conversation the service replays are both unchanged.
 *
 * Anything that is not the envelope, and any envelope that does not parse, is
 * returned as it came. A block that is hard to read is better than a message
 * this guessed at.
 */
export function calmUserMessage(markdown: string): string {
  const match = SURFACE_MESSAGE.exec(markdown);
  if (!match) return markdown;
  const [, attachmentsJson, textJson] = match;
  if (attachmentsJson === undefined || textJson === undefined) return markdown;
  let text: unknown;
  let attachments: unknown;
  try {
    attachments = JSON.parse(attachmentsJson);
    text = JSON.parse(textJson);
  } catch {
    return markdown;
  }
  if (typeof text !== "string") return markdown;
  const names = (Array.isArray(attachments) ? attachments : [])
    .map((item) =>
      typeof item === "object" && item !== null
        ? (item as { name?: unknown }).name
        : undefined,
    )
    .filter((name): name is string => typeof name === "string");
  const words = text.trim();
  if (names.length === 0) return words;
  const attached = `_attached: ${names.join(", ")}_`;
  return words ? `${words}\n\n${attached}` : attached;
}

function calmState(): CalmState {
  const globals = globalThis as CalmGlobals;
  globals[calmStateKey] ??= { enabled: true };
  return globals[calmStateKey];
}

function calmAssistantMessage(message: AssistantMessage): AssistantMessage {
  const hasToolCall = message.content.some(
    (content) => content.type === "toolCall",
  );
  const content = message.content.filter(
    (item) =>
      item.type !== "thinking" &&
      item.type !== "toolCall" &&
      !(hasToolCall && item.type === "text"),
  );
  return content.length === message.content.length
    ? message
    : { ...message, content };
}

function installCalmPatches(): void {
  const globals = globalThis as CalmGlobals;
  if (globals[calmPatchKey]) return;

  const assistantUpdate = AssistantMessageComponent.prototype.updateContent;
  const toolRender = ToolExecutionComponent.prototype.render;
  const customMessageRender = CustomMessageComponent.prototype.render;
  if (
    typeof assistantUpdate !== "function" ||
    typeof toolRender !== "function" ||
    typeof customMessageRender !== "function"
  ) {
    throw new Error("Scufris Calm requires Pi transcript renderer APIs");
  }

  AssistantMessageComponent.prototype.updateContent = function (
    message,
    isStreaming,
  ): void {
    const presented = calmState().enabled
      ? calmAssistantMessage(message)
      : message;
    assistantUpdate.call(this, presented, isStreaming);
    if (presented !== message) {
      (this as unknown as AssistantMessageState).lastMessage = message;
    }
  };

  ToolExecutionComponent.prototype.render = function (width): string[] {
    return calmState().enabled ? [] : toolRender.call(this, width);
  };

  CustomMessageComponent.prototype.render = function (width): string[] {
    const customType = (this as unknown as CustomMessageState).message
      ?.customType;
    return calmState().enabled &&
      customType &&
      hiddenCustomTypes.has(customType)
      ? []
      : customMessageRender.call(this, width);
  };

  globals[calmPatchKey] = true;
}

function applyCalmPresentation(context: ExtensionContext): void {
  context.ui.setHiddenThinkingLabel(calmState().enabled ? "" : undefined);
}

function restoredCalmState(
  context: ExtensionContext,
  fallback: boolean,
): boolean {
  let enabled = fallback;
  for (const entry of context.sessionManager.getBranch()) {
    if (entry.type !== "custom" || entry.customType !== calmStateType) continue;
    const data = entry.data as Partial<CalmStateEntry> | undefined;
    if (data?.version === 1 && typeof data.enabled === "boolean")
      enabled = data.enabled;
  }
  return enabled;
}

export function resolveCalmCommand(
  args: string,
  current: boolean,
): { enabled: boolean; changed: boolean; notice: string; warning: boolean } {
  const command = args.trim().toLowerCase();
  if (command === "") {
    return {
      enabled: current,
      changed: false,
      notice: `Calm mode ${current ? "on" : "off"}.`,
      warning: false,
    };
  }
  if (command === "on" || command === "off") {
    const enabled = command === "on";
    return {
      enabled,
      changed: enabled !== current,
      notice: `Calm mode ${command}.`,
      warning: false,
    };
  }
  return {
    enabled: current,
    changed: false,
    notice: "Use /calm on or off.",
    warning: true,
  };
}

export default function calm(pi: ExtensionAPI): void {
  installCalmPatches();
  const defaultEnabled = true;

  const restore = (context: ExtensionContext) => {
    calmState().enabled = restoredCalmState(context, defaultEnabled);
    applyCalmPresentation(context);
  };

  pi.on("session_start", (_event, context) => restore(context));
  pi.on("session_tree", (_event, context) => restore(context));

  // Pi runs this when it draws a message and when the width changes, so a
  // message already on the screen keeps the presentation it was drawn with
  // until something redraws it. That is why the state is read here rather
  // than captured: a terminal started after `/calm off` renders raw from its
  // first message, which is the setting the person left.
  pi.registerMarkdownTransformer((markdown, context) =>
    calmState().enabled && context.messageType === "user"
      ? calmUserMessage(markdown)
      : markdown,
  );

  pi.registerCommand("calm", {
    description: "Control Scufris Calm transcript presentation: on or off.",
    getArgumentCompletions: (prefix) => {
      const values = ["on", "off"];
      const matches = values.filter((value) =>
        value.startsWith(prefix.trim().toLowerCase()),
      );
      return matches.length
        ? matches.map((value) => ({ value, label: value }))
        : null;
    },
    handler: async (args, context) => {
      const state = calmState();
      const result = resolveCalmCommand(args, state.enabled);
      if (result.changed) {
        state.enabled = result.enabled;
        pi.appendEntry(calmStateType, {
          version: 1,
          enabled: state.enabled,
        } satisfies CalmStateEntry);
        applyCalmPresentation(context);
      }
      context.ui.notify(result.notice, result.warning ? "warning" : "info");
    },
  });
}
