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
const hiddenCustomTypes = new Set([
  "scufris-job-event",
  "scufris-widget-event",
]);

type CalmState = { enabled: boolean };
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

export default function calm(pi: ExtensionAPI): void {
  installCalmPatches();
  if (process.env.SCUFRIS_CALM === "1") calmState().enabled = true;

  pi.on("session_start", (_event, context) => {
    applyCalmPresentation(context);
  });

  pi.registerCommand("calm", {
    description: "Toggle Scufris Calm transcript presentation.",
    handler: async (_args, context) => {
      const state = calmState();
      state.enabled = !state.enabled;
      applyCalmPresentation(context);
      context.ui.notify(`Calm mode ${state.enabled ? "on" : "off"}.`, "info");
    },
  });
}
