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
import { OWNER_EVENT, type OwnerSignal } from "../shared/owner.ts";
import {
  AGENT_RESPONSE_EVENT,
  AgentClient,
  type AgentClientOptions,
  type AtomicResponse,
} from "./client.ts";
import { registerAttachmentTool } from "./attachments.ts";
import type { LeaseGrant, LeaseSource } from "./lease.ts";
import {
  AGENT_FILE_NAME,
  REFUSAL,
  SOCKET_DIRECTORY_NAME,
  type AgentHolder,
  type AgentSession,
  type ConversationEntry,
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
> &
  Partial<Pick<AgentClient, "turn" | "activity" | "session">>;

/** One instrumentation line. Fields are small and JSON-safe. */
export type ServiceTrace = (
  event: string,
  fields?: Record<string, unknown>,
) => void;

/**
 * Who owns the jobs this process delegates from now on.
 *
 * The foreground token moves with the agent, so a job started in a terminal
 * that later gives the agent back is still the foreground's job. Orchestration
 * listens; this module only says when it changed.
 */
export { OWNER_EVENT, type OwnerSignal };

/** The custom type of the one message a joining agent is caught up with. */
export const CATCH_UP_TYPE = "scufris_catch_up";

/** How long a leased start waits for the agent channel to answer its hello. */
export const CHANNEL_TIMEOUT_MS = 15_000;

/** Resolves true when `done` settles first, false when `ms` pass first. */
function within(ms: number, done: Promise<void>): Promise<boolean> {
  return new Promise((resolve) => {
    const timer = setTimeout(() => resolve(false), ms);
    timer.unref?.();
    void done.then(() => {
      clearTimeout(timer);
      resolve(true);
    });
  });
}

/** The words of a catch-up page, as one block of text. */
export function catchUpContent(entries: ConversationEntry[]): string {
  const lines = entries.map((entry) => {
    const who = entry.role === "user" ? `user (${entry.surface})` : "you";
    return `${who}: ${entry.text}`;
  });
  return [
    "You are joining a conversation that continued without you. This is what",
    "was said, oldest first. Nobody is waiting for an answer to it.",
    "",
    ...lines,
  ].join("\n");
}

export interface ServiceBindingOptions {
  role?: string;
  socketPath?: string;
  createClient?: (options: AgentClientOptions) => ServiceAgentClient;
  /** How long a leased start waits for the channel; tests shorten it. */
  channelTimeoutMs?: number;
  /**
   * With a lease, this process joins the agent channel in the managed child's
   * place: typed turns are recorded, activity is reported, and the lease is
   * given back on shutdown. Without one, this is the managed child.
   */
  lease?: LeaseSource;
  /** Told when the lease ended without this process asking. */
  onLost?: (reason: string) => void;
  trace?: ServiceTrace;
}

/** What the terminal extension drives the agent channel with. */
export interface ServiceBinding {
  /** Take the lease and join the channel. Rejects with the refusal. */
  attach(options?: { abortWorking?: boolean }): Promise<LeaseGrant>;
  /** Give the agent back and leave the channel. */
  release(): Promise<void>;
  /** The grant in hand, when this process is the agent. */
  grant(): LeaseGrant | undefined;
  /** Whether the agent channel handshake has completed. */
  connected(): boolean;
}

/** Bind the Pi lifecycle to one service client. Options exist for a bounded
 * wiring test; normal extension loading uses only the Pi argument. */
export function bindService(
  pi: ExtensionAPI,
  options: ServiceBindingOptions = {},
): ServiceBinding | undefined {
  if ((options.role ?? process.env.SCUFRIS_ROLE) !== "orchestrator")
    return undefined;
  registerAttachmentTool(pi);
  const socketPath = options.socketPath ?? resolveSocketPath();
  const createClient =
    options.createClient ?? ((clientOptions) => new AgentClient(clientOptions));
  const channelTimeoutMs = options.channelTimeoutMs ?? CHANNEL_TIMEOUT_MS;
  const trace: ServiceTrace = options.trace ?? (() => {});
  const lease = options.lease;
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
  // The lease in hand, and the channel state that follows from it.
  let grant: LeaseGrant | undefined;
  let channelUp = false;
  let handedOff = false;
  // The turn typed here that is running now, and the one the host accepted.
  // Only an accepted turn can be named in an answer: the identifier the host
  // recorded is the only one it can match.
  let turns = 0;
  let openTurnId: string | undefined;
  let acceptedTurnId: string | undefined;

  const forgetProactive = (proactiveId: string): void => {
    queuedProactiveIds = queuedProactiveIds.filter((id) => id !== proactiveId);
  };

  const notify = (message: string, level: "info" | "error") => {
    if (context?.hasUI) context.ui.notify(`Scufris service: ${message}`, level);
    else if (level === "error") console.error(`scufris service: ${message}`);
  };

  const publishRows = () => client?.jobs(rows);

  /** The Pi session this process is writing, as the host records it.
   *
   * Read defensively: a wiring test's context has no session manager, and a
   * missing declaration costs catch-up, never a failed start.
   */
  const currentSession = (): AgentSession | undefined => {
    const sessions = context?.sessionManager as
      | ExtensionContext["sessionManager"]
      | undefined;
    if (!sessions) return undefined;
    const file = sessions.getSessionFile();
    const cwd = sessions.getCwd();
    if (typeof file !== "string" || typeof cwd !== "string") return undefined;
    const parent = sessions.getHeader?.()?.parentSession;
    return {
      id: sessions.getSessionId(),
      file,
      cwd,
      ...(typeof parent === "string" ? { parent } : {}),
    };
  };

  const announceOwner = (owner: string, handoff = false): void => {
    pi.events.emit(OWNER_EVENT, {
      owner,
      ...(handoff ? { handoff: true } : {}),
    } satisfies OwnerSignal);
  };

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
    client?.response(response, proactiveId, acceptedTurnId);
  });

  if (lease) {
    // What the managed child's RPC stdout tells the service, a terminal has to
    // say over the agent channel itself.
    pi.on("agent_start", () => {
      trace("agent_start");
      client?.activity?.(true);
    });
    pi.on("agent_settled", () => {
      trace("agent_settled");
      client?.activity?.(false);
    });
    // Every interactive input is the person's words, and Pi has already
    // decided what counts: measured in Phase 0 gate G4, built-in commands,
    // extension commands, and `!` shell lines never reach this event at all,
    // while prose and `/skill:` lines reach it raw. So nothing is filtered
    // here; filtering would only drop the skill lines the design records.
    pi.on("input", (event) => {
      const images = event.images?.length ?? 0;
      trace("input", {
        source: event.source,
        bytes: Buffer.byteLength(event.text, "utf8"),
        images,
        ...(event.streamingBehavior === undefined
          ? {}
          : { streaming: event.streamingBehavior }),
      });
      if (event.source !== "interactive" || !grant) return;
      turns += 1;
      openTurnId = `terminal-${process.pid}-${turns}`;
      client?.turn?.(openTurnId, event.text, images);
    });
    pi.on("session_before_compact", (event) => {
      trace("session_before_compact", {
        reason: event.reason,
        willRetry: event.willRetry,
      });
    });
    pi.on("session_compact", (event) => {
      trace("session_compact", {
        reason: event.reason,
        willRetry: event.willRetry,
        fromExtension: event.fromExtension,
      });
    });
    // A tree move stays on the same file, so the lineage is unchanged. It is
    // reported because the branch the fork-back copies did change.
    pi.on("session_tree", () => {
      const session = currentSession();
      trace("session_tree", { ...session });
      if (session) client?.session?.(session);
    });
  }

  pi.on("message_end", (event) => {
    const proactiveId = proactiveIdFromMessage(event.message);
    if (proactiveId === undefined) return;
    turnProactiveId = proactiveId;
    turnResponseSent = false;
    client?.proactiveStarted(proactiveId);
  });

  pi.on("agent_settled", () => {
    // The turn is over, so its identifier stops being the one an answer names.
    openTurnId = undefined;
    acceptedTurnId = undefined;
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

  /** Joins the agent channel, and says when the handshake has landed. */
  const startClient = (): Promise<void> => {
    if (!socketPath) {
      notify("XDG_RUNTIME_DIR is required to reach the agent channel", "error");
      return Promise.resolve();
    }
    let up: (() => void) | undefined;
    const channel = new Promise<void>((resolve) => {
      up = resolve;
    });
    const nextClient = createClient({
      socketPath,
      busy: () => context?.isIdle() === false,
      hello: () => {
        const session = currentSession();
        return {
          ...(grant === undefined ? {} : { lease: grant.generation }),
          ...(session === undefined ? {} : { session }),
        };
      },
      connected: () => {
        channelUp = true;
        up?.();
        up = undefined;
        publishRows();
      },
      turnAck: (id, sequence) => {
        trace("turn_ack", { id, sequence });
        // Only the turn that is running can be the one an answer names.
        if (id === openTurnId) acceptedTurnId = id;
      },
      handoff: (generation, next) => {
        trace("handoff", { generation, next });
        handedOff = true;
        grant = undefined;
        channelUp = false;
        client?.stop();
        client = undefined;
        announceOwner(independentOwner(), true);
        notify(handoffNotice(next), "info");
      },
      catchUp: (since, entries) => {
        trace("catch_up", { since, entries: entries.length });
        if (entries.length === 0) return;
        try {
          pi.sendMessage(
            {
              customType: CATCH_UP_TYPE,
              content: catchUpContent(entries),
              display: false,
            },
            { deliverAs: "nextTurn", triggerTurn: false },
          );
        } catch (error) {
          notify(
            `the catch-up could not be injected: ${String(error)}`,
            "error",
          );
        }
      },
      refused: (id, code, detail) => {
        trace("refused", { ...(id === undefined ? {} : { id }), code });
        if (
          code !== REFUSAL.NOT_LEASE_HOLDER &&
          code !== REFUSAL.LEASE_REQUIRED
        )
          return;
        // The host already gave the agent back. Saying so is what turns this
        // process into an ordinary Pi instead of a writer nobody reads.
        grant = undefined;
        channelUp = false;
        client?.stop();
        client = undefined;
        announceOwner(independentOwner());
        notify(`the agent was taken back: ${detail || code}`, "error");
        options.onLost?.(code);
      },
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
    return channel;
  };

  /** The owner token this process falls back to with no lease. */
  const independentOwner = (): string =>
    currentSession()?.id ?? `pid-${process.pid}`;

  const attach: ServiceBinding["attach"] = async (attachOptions = {}) => {
    if (!lease) throw new Error("this process was not built to take the lease");
    const session = currentSession();
    const taken = await lease.acquire(
      {
        pid: process.pid,
        cwd: session?.cwd ?? process.cwd(),
        ...(session === undefined ? {} : { session_file: session.file }),
      },
      attachOptions.abortWorking === true,
    );
    grant = taken;
    handedOff = false;
    trace("lease_acquired", { ...taken });
    announceOwner(taken.owner);
    client?.stop();
    client = undefined;
    channelUp = false;
    // A typed turn can follow the grant at once, and the client drops whatever
    // is sent before its hello is answered. Measured in the proof of concept:
    // a prompt arrived six milliseconds after the grant, and the question
    // never reached the conversation while its answer did. So an attach is not
    // over until the channel is up, or has plainly not come.
    const ready = await within(channelTimeoutMs, startClient());
    trace(ready ? "channel_ready" : "channel_timeout");
    if (!ready)
      notify(
        "the agent channel did not come up in time; typed turns are not recorded until it does",
        "error",
      );
    return taken;
  };

  const release: ServiceBinding["release"] = async () => {
    client?.stop();
    client = undefined;
    channelUp = false;
    if (!lease || !grant) return;
    grant = undefined;
    announceOwner(independentOwner());
    try {
      await lease.release();
      trace("lease_released");
    } catch (error) {
      trace("lease_release_failed", { error: String(error) });
    }
  };

  pi.on("session_start", async (event, ctx) => {
    turnProactiveId = undefined;
    turnResponseSent = false;
    queuedProactiveIds = [];
    openTurnId = undefined;
    acceptedTurnId = undefined;
    context = ctx;
    const session = currentSession();
    trace("session_start", { reason: event.reason, ...session });
    if (!socketPath) {
      notify("XDG_RUNTIME_DIR is required to reach the agent channel", "error");
      return;
    }
    // A session switch starts again without a shutdown in between, and the old
    // client would otherwise hold the channel against the new one.
    client?.stop();
    client = undefined;
    channelUp = false;
    // With a lease, joining is a policy decision the terminal extension makes:
    // it owns when to take the agent and when to stay an ordinary Pi.
    if (lease) return;
    await startClient();
  });

  pi.on("session_shutdown", async () => {
    trace("session_shutdown", { handoff: handedOff });
    client?.stop();
    client = undefined;
    channelUp = false;
    context = undefined;
    turnProactiveId = undefined;
    turnResponseSent = false;
    queuedProactiveIds = [];
    openTurnId = undefined;
    acceptedTurnId = undefined;
    rows = [];
    // A handoff already ended the lease on the host side; releasing again
    // would only refuse.
    if (!lease || !grant || handedOff) {
      grant = undefined;
      return;
    }
    grant = undefined;
    try {
      await lease.release();
      trace("lease_released");
    } catch (error) {
      trace("lease_release_failed", { error: String(error) });
    }
  });

  if (lease)
    lease.onLost((reason) => {
      trace("lease_lost", { reason });
      grant = undefined;
      channelUp = false;
      client?.stop();
      client = undefined;
      announceOwner(independentOwner());
      notify(`the lease ended (${reason}); this Pi is on its own`, "error");
      options.onLost?.(reason);
    });

  return {
    attach,
    release,
    grant: () => grant,
    connected: () => channelUp,
  };
}

/** What the terminal says when the agent moves on. */
function handoffNotice(next: AgentHolder): string {
  return next === "managed"
    ? "the agent went back to the background service; this Pi is on its own"
    : "the agent moved to another terminal; this Pi is on its own";
}

export default bindService;
