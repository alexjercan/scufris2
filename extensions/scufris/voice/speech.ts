import type { AssistantMessage } from "@earendil-works/pi-ai";
import type {
  ExtensionAPI,
  ExtensionContext,
} from "@earendil-works/pi-coding-agent";
import { SPOKEN_EVENT, type SpokenSignal } from "../shared/spoken.ts";
import { plainProseParagraph } from "./response.ts";

const speechStateType = "scufris-speech-state-v1";

export type SpeechMode = "off" | "on" | "once";

interface SpeechStateEntry {
  version: 1;
  mode: SpeechMode;
}

interface BranchMessageEntry {
  type: "message";
  id: string;
  message: AssistantMessage | { role: string };
}

export interface SafeAssistantParagraph {
  entryId: string;
  paragraph: string;
}

export function extractSpokenParagraph(
  message: AssistantMessage,
): string | undefined {
  if (
    message.stopReason !== "stop" ||
    message.content.some((content) => content.type === "toolCall")
  ) {
    return undefined;
  }
  const text = message.content
    .filter((content) => content.type === "text")
    .map((content) => content.text)
    .join("");
  return plainProseParagraph(text);
}

function isAssistantMessage(value: unknown): value is AssistantMessage {
  return (
    typeof value === "object" &&
    value !== null &&
    (value as { role?: unknown }).role === "assistant" &&
    Array.isArray((value as { content?: unknown }).content) &&
    typeof (value as { stopReason?: unknown }).stopReason === "string"
  );
}

export function lastSafeAssistantParagraph(
  entries: readonly unknown[],
): SafeAssistantParagraph | undefined {
  for (let index = entries.length - 1; index >= 0; index -= 1) {
    const candidate = entries[index];
    if (typeof candidate !== "object" || candidate === null) continue;
    const custom = candidate as {
      type?: unknown;
      id?: unknown;
      customType?: unknown;
      data?: { version?: unknown; spoken?: unknown };
    };
    if (
      custom.type === "custom" &&
      custom.customType === "scufris-response-v1" &&
      custom.data?.version === 1 &&
      typeof custom.data.spoken === "string" &&
      typeof custom.id === "string"
    ) {
      const paragraph = plainProseParagraph(custom.data.spoken);
      return paragraph ? { entryId: custom.id, paragraph } : undefined;
    }
    if (custom.type !== "message") continue;
    const entry = candidate as BranchMessageEntry;
    if ((entry.message as { role?: unknown }).role !== "assistant") continue;
    if (!isAssistantMessage(entry.message)) return undefined;
    const paragraph = extractSpokenParagraph(entry.message);
    return paragraph ? { entryId: entry.id, paragraph } : undefined;
  }
  return undefined;
}

function restoredMode(
  context: ExtensionContext,
  fallback: SpeechMode,
): SpeechMode {
  let mode = fallback;
  for (const entry of context.sessionManager.getBranch()) {
    if (entry.type !== "custom" || entry.customType !== speechStateType) {
      continue;
    }
    const data = entry.data as Partial<SpeechStateEntry> | undefined;
    if (
      data?.version === 1 &&
      (data.mode === "off" || data.mode === "on" || data.mode === "once")
    ) {
      mode = data.mode;
    }
  }
  return mode;
}

/**
 * Decides what Scufris says aloud, and hands it to whoever owns the speaker.
 *
 * Synthesis and playback are not here. The frontend owns the speaker, so it
 * runs Piper and may refuse what it is handed; a headless agent has no speaker
 * to run one for. What stays here is the decision, because the prose rules that
 * make a paragraph safe to say are the agent's and always were.
 *
 * The mode is off, on, or once, and it is remembered in the session so a
 * restart resumes with it.
 */
export default function speech(pi: ExtensionAPI): void {
  if (process.env.SCUFRIS_ROLE !== "orchestrator") return;

  const environmentMode: SpeechMode =
    process.env.SCUFRIS_SPEECH === "1" ? "on" : "off";
  let mode: SpeechMode = environmentMode;
  let awaitingSettlement = false;
  let speakAtSettlement = false;
  let runEntryBoundary = 0;
  let missingResponseWarned = false;

  const persistMode = () => {
    pi.appendEntry(speechStateType, {
      version: 1,
      mode,
    } satisfies SpeechStateEntry);
  };

  const say = (paragraph: string) => {
    pi.events.emit(SPOKEN_EVENT, { speak: paragraph } satisfies SpokenSignal);
  };

  const warn = (context: ExtensionContext, message: string) => {
    if (context.hasUI) context.ui.notify(message, "warning");
  };

  pi.registerCommand("speech", {
    description: "Control Scufris spoken responses: on, off, once, or replay.",
    getArgumentCompletions: (prefix) => {
      const values = ["on", "off", "once", "replay"];
      const matches = values.filter((value) => value.startsWith(prefix.trim()));
      return matches.length
        ? matches.map((value) => ({ value, label: value }))
        : null;
    },
    handler: async (args, context) => {
      const command = args.trim().toLowerCase();
      if (command === "on") {
        mode = "on";
        persistMode();
        context.ui.notify("Speech mode on.", "info");
        return;
      }
      if (command === "off") {
        mode = "off";
        speakAtSettlement = false;
        persistMode();
        context.ui.notify("Speech mode off.", "info");
        return;
      }
      if (command === "once") {
        mode = "once";
        persistMode();
        context.ui.notify("Speech armed for one response.", "info");
        return;
      }
      if (command === "replay") {
        const response = lastSafeAssistantParagraph(
          context.sessionManager.getBranch(),
        );
        if (!response) {
          if (!missingResponseWarned) {
            warn(context, "No safe response to speak.");
            missingResponseWarned = true;
          }
          return;
        }
        say(response.paragraph);
        return;
      }
      context.ui.notify("Use /speech on, off, once, or replay.", "warning");
    },
  });

  pi.on("session_start", (_event, context) => {
    awaitingSettlement = false;
    speakAtSettlement = false;
    runEntryBoundary = 0;
    missingResponseWarned = false;
    mode = restoredMode(context, environmentMode);
  });

  pi.on("session_tree", (_event, context) => {
    awaitingSettlement = false;
    speakAtSettlement = false;
    runEntryBoundary = 0;
    mode = restoredMode(context, environmentMode);
  });

  pi.on("agent_start", (_event, context) => {
    if (awaitingSettlement) return;
    awaitingSettlement = true;
    speakAtSettlement = mode !== "off";
    runEntryBoundary = context.sessionManager.getBranch().length;
    if (mode === "once") {
      mode = "off";
      persistMode();
    }
  });

  pi.on("agent_settled", (_event, context) => {
    const shouldSpeak = speakAtSettlement;
    const boundary = runEntryBoundary;
    awaitingSettlement = false;
    speakAtSettlement = false;
    runEntryBoundary = 0;
    if (!shouldSpeak) return;

    const response = lastSafeAssistantParagraph(
      context.sessionManager.getBranch().slice(boundary),
    );
    if (!response) {
      if (!missingResponseWarned) {
        warn(context, "No safe response to speak.");
        missingResponseWarned = true;
      }
      return;
    }
    missingResponseWarned = false;
    say(response.paragraph);
  });

  pi.on("session_shutdown", () => {
    awaitingSettlement = false;
    speakAtSettlement = false;
    runEntryBoundary = 0;
    missingResponseWarned = false;
  });
}
