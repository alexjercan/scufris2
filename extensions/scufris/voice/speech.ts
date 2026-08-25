import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import type { AssistantMessage } from "@earendil-works/pi-ai";
import type {
  ExtensionAPI,
  ExtensionContext,
} from "@earendil-works/pi-coding-agent";
import {
  SPEECH_STATE_EVENT,
  type SpeechStateSignal,
} from "../shared/assistant-state.ts";
import { plainProseParagraph } from "./response.ts";

const speechHelperPath = fileURLToPath(
  new URL("../../../tools/voice/scufris-speak", import.meta.url),
);
const speechStateType = "scufris-speech-state-v1";
const maxSpeechBytes = 1_000;
const maxHelperErrorBytes = 2_048;
const defaultPlaybackTimeoutMs = 65_000;
const stopGraceMs = 750;

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

export interface SpeechPlayback {
  play(text: string): Promise<void>;
  cancel(): Promise<void>;
}

interface ActivePlayback {
  child: ReturnType<typeof spawn>;
  cancelled: boolean;
  timedOut: boolean;
  outputExceeded: boolean;
  errorBytes: number;
  timeout: ReturnType<typeof setTimeout>;
  escalation?: ReturnType<typeof setTimeout>;
  done: Promise<void>;
  resolve: () => void;
  reject: (error: Error) => void;
  settled: boolean;
}

export class SpeechPlaybackError extends Error {}

function boundedUtf8(value: string, maximum: number): boolean {
  return Buffer.byteLength(value, "utf8") <= maximum;
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

function helperExitError(code: number | null): SpeechPlaybackError {
  switch (code) {
    case 2:
      return new SpeechPlaybackError("Speech input was rejected.");
    case 3:
    case 4:
      return new SpeechPlaybackError("Speech is unavailable.");
    case 5:
      return new SpeechPlaybackError("Speech synthesis failed.");
    case 6:
      return new SpeechPlaybackError("Speech playback failed.");
    case 7:
      return new SpeechPlaybackError("Speech playback timed out.");
    default:
      return new SpeechPlaybackError("Speech playback failed.");
  }
}

export class OwnedSpeechPlayback implements SpeechPlayback {
  private current?: ActivePlayback;
  private generation = 0;
  private readonly helperPath: string;
  private readonly timeoutMs: number;

  constructor(
    helperPath = speechHelperPath,
    timeoutMs = defaultPlaybackTimeoutMs,
  ) {
    this.helperPath = helperPath;
    this.timeoutMs = timeoutMs;
  }

  private settle(active: ActivePlayback, error?: Error): void {
    if (active.settled) return;
    active.settled = true;
    clearTimeout(active.timeout);
    if (active.escalation) clearTimeout(active.escalation);
    if (this.current === active) this.current = undefined;
    if (active.cancelled) active.resolve();
    else if (error) active.reject(error);
    else active.resolve();
  }

  private async stop(
    active: ActivePlayback,
    cancelled: boolean,
  ): Promise<void> {
    if (cancelled) active.cancelled = true;
    if (!active.settled) {
      active.child.kill("SIGTERM");
      active.escalation ??= setTimeout(() => {
        if (!active.settled) active.child.kill("SIGKILL");
      }, stopGraceMs);
      active.escalation.unref();
    }
    await active.done.catch(() => undefined);
  }

  async play(text: string): Promise<void> {
    if (!text || !boundedUtf8(text, maxSpeechBytes)) {
      throw new SpeechPlaybackError("Speech input was rejected.");
    }

    const generation = ++this.generation;
    const previous = this.current;
    if (previous) await this.stop(previous, true);
    if (generation !== this.generation) return;

    const child = spawn(this.helperPath, [], {
      env: process.env,
      shell: false,
      stdio: ["pipe", "ignore", "pipe"],
    });
    let resolve = () => {};
    let reject = (_error: Error) => {};
    const done = new Promise<void>((doneResolve, doneReject) => {
      resolve = doneResolve;
      reject = doneReject;
    });
    const active: ActivePlayback = {
      child,
      cancelled: false,
      timedOut: false,
      outputExceeded: false,
      errorBytes: 0,
      timeout: setTimeout(() => {
        active.timedOut = true;
        void this.stop(active, false);
      }, this.timeoutMs),
      done,
      resolve,
      reject,
      settled: false,
    };
    active.timeout.unref();
    this.current = active;

    child.stderr?.on("data", (chunk: Buffer) => {
      active.errorBytes += chunk.length;
      if (active.errorBytes > maxHelperErrorBytes && !active.outputExceeded) {
        active.outputExceeded = true;
        void this.stop(active, false);
      }
    });
    child.stdin?.on("error", () => undefined);
    child.once("error", () =>
      this.settle(active, new SpeechPlaybackError("Speech is unavailable.")),
    );
    child.once("close", (code) => {
      if (active.timedOut) {
        this.settle(
          active,
          new SpeechPlaybackError("Speech playback timed out."),
        );
      } else if (active.outputExceeded) {
        this.settle(active, new SpeechPlaybackError("Speech playback failed."));
      } else if (code === 0 || active.cancelled) {
        this.settle(active);
      } else {
        this.settle(active, helperExitError(code));
      }
    });
    child.stdin?.end(text, "utf8");
    return done;
  }

  async cancel(): Promise<void> {
    this.generation += 1;
    const active = this.current;
    if (active) await this.stop(active, true);
  }
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

export default function speech(
  pi: ExtensionAPI,
  options: { playback?: SpeechPlayback } = {},
): void {
  if (process.env.SCUFRIS_ROLE !== "orchestrator") return;

  const playback = options.playback ?? new OwnedSpeechPlayback();
  const environmentMode: SpeechMode =
    process.env.SCUFRIS_SPEECH === "1" ? "on" : "off";
  let mode: SpeechMode = environmentMode;
  let tuiActive = false;
  let awaitingSettlement = false;
  let speakAtSettlement = false;
  let runEntryBoundary = 0;
  let playbackGeneration = 0;
  let sessionGeneration = 0;
  let missingResponseWarned = false;

  const persistMode = () => {
    pi.appendEntry(speechStateType, {
      version: 1,
      mode,
    } satisfies SpeechStateEntry);
  };

  const announceSpeech = (playing: boolean) => {
    pi.events.emit(SPEECH_STATE_EVENT, { playing } satisfies SpeechStateSignal);
  };

  const cancelPlayback = async () => {
    playbackGeneration += 1;
    announceSpeech(false);
    await playback.cancel();
  };

  const notifyPlaybackError = (
    error: unknown,
    context: ExtensionContext,
    generation: number,
    session: number,
  ) => {
    if (generation !== playbackGeneration || session !== sessionGeneration) {
      return;
    }
    context.ui.notify(
      error instanceof SpeechPlaybackError
        ? error.message
        : "Speech playback failed.",
      "error",
    );
  };

  const play = (paragraph: string, context: ExtensionContext) => {
    const generation = ++playbackGeneration;
    const session = sessionGeneration;
    const finish = () => {
      if (generation === playbackGeneration && session === sessionGeneration) {
        announceSpeech(false);
      }
    };
    announceSpeech(true);
    void playback.play(paragraph).then(
      () => {
        finish();
        if (
          generation === playbackGeneration &&
          session === sessionGeneration
        ) {
          missingResponseWarned = false;
        }
      },
      (error) => {
        finish();
        notifyPlaybackError(error, context, generation, session);
      },
    );
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
      if (context.mode !== "tui") return;
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
        await cancelPlayback();
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
            context.ui.notify("No safe response to speak.", "warning");
            missingResponseWarned = true;
          }
          return;
        }
        play(response.paragraph, context);
        return;
      }
      context.ui.notify("Use /speech on, off, once, or replay.", "warning");
    },
  });

  pi.on("session_start", async (_event, context) => {
    sessionGeneration += 1;
    awaitingSettlement = false;
    speakAtSettlement = false;
    runEntryBoundary = 0;
    missingResponseWarned = false;
    await cancelPlayback();
    tuiActive = context.mode === "tui";
    mode = tuiActive ? restoredMode(context, environmentMode) : "off";
  });

  pi.on("session_tree", async (_event, context) => {
    awaitingSettlement = false;
    speakAtSettlement = false;
    runEntryBoundary = 0;
    await cancelPlayback();
    if (context.mode === "tui") mode = restoredMode(context, environmentMode);
  });

  pi.on("input", async (_event, context) => {
    if (context.mode === "tui") await cancelPlayback();
    return { action: "continue" };
  });

  pi.on("agent_start", (_event, context) => {
    if (!tuiActive || context.mode !== "tui" || awaitingSettlement) return;
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
    if (!shouldSpeak || !tuiActive || context.mode !== "tui") return;

    const response = lastSafeAssistantParagraph(
      context.sessionManager.getBranch().slice(boundary),
    );
    if (!response) {
      if (!missingResponseWarned) {
        context.ui.notify("No safe response to speak.", "warning");
        missingResponseWarned = true;
      }
      return;
    }
    play(response.paragraph, context);
  });

  pi.on("session_shutdown", async () => {
    sessionGeneration += 1;
    tuiActive = false;
    awaitingSettlement = false;
    speakAtSettlement = false;
    runEntryBoundary = 0;
    missingResponseWarned = false;
    await cancelPlayback();
  });
}
