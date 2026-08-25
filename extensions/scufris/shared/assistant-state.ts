export const SPEECH_STATE_EVENT = "scufris:speech-state";
export const ATTENTION_STATE_EVENT = "scufris:attention-state";

/**
 * States the Scufris daemon reports over the desktop control protocol.
 * Listening and transcribing stay companion-local; the daemon never sees audio.
 */
export type AssistantState =
  | "idle"
  | "working"
  | "speaking"
  | "attention"
  | "error";

export interface SpeechStateSignal {
  playing: boolean;
}

export interface AttentionStateSignal {
  state: "attention" | "error" | "clear";
  detail: string;
}

export interface AssistantStateReport {
  state: AssistantState;
  detail: string;
}

const maxDetail = 200;

function boundedDetail(value: string): string {
  const collapsed = value.replace(/\s+/g, " ").trim();
  return collapsed.length > maxDetail
    ? `${collapsed.slice(0, maxDetail - 1)}…`
    : collapsed;
}

/**
 * Resolves one assistant state from the independent signals the daemon
 * observes. An active run always wins, because that is what the user just
 * asked for; an unattended job only surfaces once nothing else is happening.
 */
export class AssistantStateTracker {
  private running = false;
  private speaking = false;
  private unattended?: { state: "attention" | "error"; detail: string };

  report(): AssistantStateReport {
    if (this.running) return { state: "working", detail: "" };
    if (this.speaking) return { state: "speaking", detail: "" };
    if (this.unattended) return { ...this.unattended };
    return { state: "idle", detail: "" };
  }

  setRunning(running: boolean): void {
    this.running = running;
    if (running) this.unattended = undefined;
  }

  setSpeaking(speaking: boolean): void {
    this.speaking = speaking;
  }

  setUnattended(signal: AttentionStateSignal): void {
    this.unattended =
      signal.state === "clear"
        ? undefined
        : { state: signal.state, detail: boundedDetail(signal.detail) };
  }

  reset(): void {
    this.running = false;
    this.speaking = false;
    this.unattended = undefined;
  }
}
