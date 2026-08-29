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
const hiddenCustomTypes = new Set(["scufris-job-event"]);

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
