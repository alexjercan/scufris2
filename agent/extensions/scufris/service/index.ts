import { join } from "node:path";
import type {
  ExtensionAPI,
  ExtensionContext,
} from "@earendil-works/pi-coding-agent";
import {
  ATTENTION_NOTICE_EVENT,
  type AttentionNoticeSignal,
} from "../shared/attention-notice.ts";
import {
  AGENT_RESPONSE_EVENT,
  AgentClient,
  type AtomicResponse,
} from "./client.ts";
import { AGENT_FILE_NAME, SOCKET_DIRECTORY_NAME } from "./protocol.ts";

export function resolveSocketPath(
  environment: NodeJS.ProcessEnv = process.env,
): string | undefined {
  if (environment.SCUFRIS_AGENT_SOCKET) return environment.SCUFRIS_AGENT_SOCKET;
  if (environment.SCUFRIS_RUNTIME_DIR)
    return join(environment.SCUFRIS_RUNTIME_DIR, AGENT_FILE_NAME);
  if (environment.XDG_RUNTIME_DIR)
    return join(
      environment.XDG_RUNTIME_DIR,
      SOCKET_DIRECTORY_NAME,
      AGENT_FILE_NAME,
    );
  return undefined;
}

export default function service(pi: ExtensionAPI): void {
  if (process.env.SCUFRIS_ROLE !== "orchestrator") return;
  const socketPath = resolveSocketPath();
  const notices = new Map<string, AttentionNoticeSignal>();
  let context: ExtensionContext | undefined;
  let client: AgentClient | undefined;

  const notify = (message: string, level: "info" | "error") => {
    if (context?.hasUI) context.ui.notify(`Scufris service: ${message}`, level);
    else if (level === "error") console.error(`scufris service: ${message}`);
  };

  const publishState = () => {
    const failed = [...notices.values()].find(
      (notice) => notice.state === "error",
    );
    const blocked = [...notices.values()].find(
      (notice) => notice.state === "attention",
    );
    const selected = failed ?? blocked;
    client?.state(
      failed ? "failed" : blocked ? "blocked" : "clear",
      selected?.detail ?? "",
    );
  };

  pi.events.on(ATTENTION_NOTICE_EVENT, (value: unknown) => {
    const signal = value as Partial<AttentionNoticeSignal> | undefined;
    if (
      typeof signal?.id !== "string" ||
      (signal.state !== "attention" &&
        signal.state !== "error" &&
        signal.state !== "clear")
    )
      return;
    if (signal.state === "clear") notices.delete(signal.id);
    else
      notices.set(signal.id, {
        id: signal.id,
        state: signal.state,
        detail: typeof signal.detail === "string" ? signal.detail : "",
      });
    publishState();
  });

  pi.events.on(AGENT_RESPONSE_EVENT, (value: unknown) => {
    const response = value as AtomicResponse | undefined;
    if (typeof response?.text === "string") client?.response(response);
  });

  pi.on("session_start", (_event, ctx) => {
    context = ctx;
    if (!socketPath) {
      notify("XDG_RUNTIME_DIR is required to reach the agent channel", "error");
      return;
    }
    client = new AgentClient({
      socketPath,
      busy: () => context?.isIdle() === false,
      abort: () => context?.abort(),
      sendUserMessage: (message, busy) => {
        if (busy) pi.sendUserMessage(message, { deliverAs: "steer" });
        else pi.sendUserMessage(message);
      },
      log: notify,
    });
    client.start();
  });

  pi.on("session_shutdown", () => {
    client?.stop();
    client = undefined;
    context = undefined;
    notices.clear();
  });
}
