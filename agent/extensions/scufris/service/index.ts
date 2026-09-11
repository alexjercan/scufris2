import { join } from "node:path";
import type {
  ExtensionAPI,
  ExtensionContext,
} from "@earendil-works/pi-coding-agent";
import { OFFER_TAKE_EVENT, type OfferTakeSignal } from "../shared/citations.ts";
import {
  JOB_COMMAND_EVENT,
  JOB_ROWS_EVENT,
  type JobCommandSignal,
} from "../shared/job-rows.ts";
import {
  AGENT_RESPONSE_EVENT,
  AgentClient,
  type AgentClientOptions,
  type AtomicResponse,
} from "./client.ts";
import { registerAttachmentTool } from "./attachments.ts";
import {
  AGENT_FILE_NAME,
  SOCKET_DIRECTORY_NAME,
  type JobRow,
} from "./protocol.ts";

export const PROACTIVE_DETAILS_VERSION = 1;
export const PROACTIVE_DETAILS_KEY = "__scufrisServiceProactive";

interface ProactiveMessageDetails {
  version: typeof PROACTIVE_DETAILS_VERSION;
  proactiveId: string;
}

/** Keep host correlation on the exact custom message Pi queues. */
export function proactiveMessageDetails(
  proactiveId: string,
  wakeDetails?: unknown,
): Record<string, unknown> {
  const details =
    typeof wakeDetails === "object" &&
    wakeDetails !== null &&
    !Array.isArray(wakeDetails)
      ? (wakeDetails as Record<string, unknown>)
      : {};
  return {
    ...details,
    [PROACTIVE_DETAILS_KEY]: {
      version: PROACTIVE_DETAILS_VERSION,
      proactiveId,
    } satisfies ProactiveMessageDetails,
  };
}

/** Read only correlation envelopes created by this extension. */
export function proactiveIdFromMessage(message: unknown): string | undefined {
  if (typeof message !== "object" || message === null) return undefined;
  const custom = message as Record<string, unknown>;
  if (custom.role !== "custom") return undefined;
  const details = custom.details;
  if (typeof details !== "object" || details === null) return undefined;
  const envelope = (details as Record<string, unknown>)[PROACTIVE_DETAILS_KEY];
  if (typeof envelope !== "object" || envelope === null) return undefined;
  const correlation = envelope as Partial<ProactiveMessageDetails>;
  return correlation.version === PROACTIVE_DETAILS_VERSION &&
    typeof correlation.proactiveId === "string"
    ? correlation.proactiveId
    : undefined;
}

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

type ServiceAgentClient = Pick<
  AgentClient,
  | "start"
  | "stop"
  | "jobs"
  | "response"
  | "proactiveStarted"
  | "proactiveSettled"
>;

export interface ServiceBindingOptions {
  role?: string;
  socketPath?: string;
  createClient?: (options: AgentClientOptions) => ServiceAgentClient;
}

/** Bind the Pi lifecycle to one service client. Options exist for a bounded
 * wiring test; normal extension loading uses only the Pi argument. */
export function bindService(
  pi: ExtensionAPI,
  options: ServiceBindingOptions = {},
): void {
  if ((options.role ?? process.env.SCUFRIS_ROLE) !== "orchestrator") return;
  registerAttachmentTool(pi);
  const socketPath = options.socketPath ?? resolveSocketPath();
  const createClient =
    options.createClient ?? ((clientOptions) => new AgentClient(clientOptions));
  // The host keeps nothing for an agent that went away, and rows published
  // while the socket was down were dropped. Holding the last list is what
  // lets a reconnect say again that a job is still blocked.
  let rows: JobRow[] = [];
  let context: ExtensionContext | undefined;
  let client: ServiceAgentClient | undefined;
  // Set only when Pi delivers the correlated custom message. Queue receipt is
  // not turn receipt: workflow and offer follow-ups may be ahead of it.
  let turnProactiveId: string | undefined;
  let turnResponseSent = false;
  // IDs accepted into Pi's follow-up queue, retained through socket reconnects
  // so a host redelivery cannot queue a second copy of the same model turn.
  let queuedProactiveIds: string[] = [];

  const forgetProactive = (proactiveId: string): void => {
    queuedProactiveIds = queuedProactiveIds.filter((id) => id !== proactiveId);
  };

  const notify = (message: string, level: "info" | "error") => {
    if (context?.hasUI) context.ui.notify(`Scufris service: ${message}`, level);
    else if (level === "error") console.error(`scufris service: ${message}`);
  };

  const publishRows = () => client?.jobs(rows);

  pi.events.on(JOB_ROWS_EVENT, (value: unknown) => {
    if (!Array.isArray(value)) return;
    rows = value as JobRow[];
    publishRows();
  });

  pi.events.on(AGENT_RESPONSE_EVENT, (value: unknown) => {
    const response = value as AtomicResponse | undefined;
    if (typeof response?.text !== "string") return;
    const proactiveId = turnResponseSent ? undefined : turnProactiveId;
    if (proactiveId !== undefined) turnResponseSent = true;
    client?.response(response, proactiveId);
  });

  pi.on("message_end", (event) => {
    const proactiveId = proactiveIdFromMessage(event.message);
    if (proactiveId === undefined) return;
    turnProactiveId = proactiveId;
    turnResponseSent = false;
    client?.proactiveStarted(proactiveId);
  });

  pi.on("agent_settled", () => {
    const proactiveId = turnProactiveId ?? queuedProactiveIds[0];
    if (proactiveId === undefined) return;
    if (turnProactiveId === undefined)
      notify(
        `proactive event ${proactiveId} settled before Pi delivered its queued message`,
        "error",
      );
    else if (!turnResponseSent)
      notify(
        `proactive event ${proactiveId} settled without an atomic response`,
        "error",
      );
    // agent_settled means no queued continuation remains. Reporting the exact
    // queued ID here lets the host release a wake that was rejected or aborted
    // before message_end, rather than reserving every surface forever.
    client?.proactiveSettled(proactiveId);
    forgetProactive(proactiveId);
    turnProactiveId = undefined;
    turnResponseSent = false;
  });

  pi.on("session_start", (_event, ctx) => {
    turnProactiveId = undefined;
    turnResponseSent = false;
    queuedProactiveIds = [];
    context = ctx;
    if (!socketPath) {
      notify("XDG_RUNTIME_DIR is required to reach the agent channel", "error");
      return;
    }
    const nextClient = createClient({
      socketPath,
      busy: () => context?.isIdle() === false,
      connected: publishRows,
      abort: () => context?.abort(),
      // Both verbs are relayed, never acted on here. This module owns the
      // socket; orchestration owns the jobs and the response tool owns the
      // words behind an offer.
      jobCommand: (id, action) =>
        pi.events.emit(JOB_COMMAND_EVENT, {
          id,
          action,
        } satisfies JobCommandSignal),
      offerTake: (id) =>
        pi.events.emit(OFFER_TAKE_EVENT, { id } satisfies OfferTakeSignal),
      sendUserMessage: (message, busy) => {
        if (busy) pi.sendUserMessage(message, { deliverAs: "steer" });
        else pi.sendUserMessage(message);
      },
      // The same wake a briefing already performs, under the caller's custom
      // type. It is a follow-up that triggers a turn, never a user message, so
      // nothing here looks like the owner typed it.
      wake: ({ proactiveId, customType, content, details }) => {
        if (
          proactiveId !== undefined &&
          (turnProactiveId === proactiveId ||
            queuedProactiveIds.includes(proactiveId))
        )
          return;
        if (proactiveId !== undefined) queuedProactiveIds.push(proactiveId);
        try {
          pi.sendMessage(
            {
              customType,
              content,
              details:
                proactiveId === undefined
                  ? details
                  : proactiveMessageDetails(proactiveId, details),
              display: true,
            },
            { deliverAs: "followUp", triggerTurn: true },
          );
        } catch (error) {
          if (proactiveId !== undefined) {
            forgetProactive(proactiveId);
            client?.proactiveSettled(proactiveId);
          }
          notify(
            `proactive message could not be queued: ${String(error)}`,
            "error",
          );
        }
      },
      log: notify,
    });
    client = nextClient;
    nextClient.start();
  });

  pi.on("session_shutdown", () => {
    client?.stop();
    client = undefined;
    context = undefined;
    turnProactiveId = undefined;
    turnResponseSent = false;
    queuedProactiveIds = [];
    rows = [];
  });
}

export default bindService;
