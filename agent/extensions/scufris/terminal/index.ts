/** An interactive Pi in a terminal as the one Scufris agent.
 *
 * The service owns a `pi --mode rpc` child and that child is normally the
 * agent. This module is how a terminal takes that place for a while: it holds
 * the lease, joins the agent channel in the child's stead, and gives the agent
 * back when the session ends. While it holds the lease, what is typed here is
 * the canonical conversation, the phone and the desktop see it, and wakes,
 * briefings, and delegated jobs reach this terminal instead of the child.
 *
 * Three states, and nothing between them:
 *
 *   Independent  an ordinary Pi. No agent channel, no HUD.
 *   Leased       the sole agent.
 *   Lost         Independent, plus a bounded loop trying to be Leased again.
 *
 * The composition that loads this is `.pi/extensions/scufris-terminal`, which
 * is the gate. Nothing here runs unless that gate opens.
 */
import type {
  ExtensionAPI,
  ExtensionCommandContext,
  ExtensionContext,
} from "@earendil-works/pi-coding-agent";
import { appendFileSync } from "node:fs";
import {
  bindService,
  type ServiceBinding,
  type ServiceBindingOptions,
  type ServiceTrace,
} from "../service/index.ts";
import {
  LeaseRefused,
  type LeaseGrant,
  type LeaseSource,
} from "../service/lease.ts";

export const TERMINAL_VARIABLE = "SCUFRIS_TERMINAL";
export const TERMINAL_LOG_VARIABLE = "SCUFRIS_TERMINAL_LOG";

/** The reacquire loop, which never becomes a busy wait. */
export const RETRY_MIN_MS = 1_000;
export const RETRY_MAX_MS = 30_000;

export type TerminalState = "independent" | "leased" | "lost";

/** Whether this process should take the lease at all. */
export function terminalEnabled(
  environment: NodeJS.ProcessEnv = process.env,
): boolean {
  if (environment[TERMINAL_VARIABLE] !== "1") return false;
  // A worker Pi is delegated work, never the conversation.
  const role = environment.SCUFRIS_ROLE;
  return role === undefined || role === "orchestrator";
}

/** One JSON line per event, appended to the named file. */
export function fileTrace(path: string | undefined): ServiceTrace {
  if (!path) return () => {};
  return (event, fields) => {
    try {
      appendFileSync(
        path,
        `${JSON.stringify({ at: new Date().toISOString(), event, ...fields })}\n`,
      );
    } catch {
      // A trace that cannot be written is not worth a broken turn.
    }
  };
}

/** Creates the fork file a terminal continues the lineage on. */
export type ForkSession = (
  lineage: string,
  cwd: string,
  sessionDir: string,
) => Promise<string | undefined>;

export interface TerminalOptions
  extends Pick<
    ServiceBindingOptions,
    "role" | "socketPath" | "createClient" | "channelTimeoutMs" | "trace"
  > {
  lease: LeaseSource;
  /** Take the lease when the session starts. */
  attachOnStart?: boolean;
  /** Continue the lineage by forking it when this Pi is not already on it. */
  forkOnStart?: boolean;
  forkSession?: ForkSession;
  retryMinMs?: number;
  retryMaxMs?: number;
}

/** What the commands and the tests drive. */
export interface TerminalControl {
  state(): TerminalState;
  attach(context?: ExtensionCommandContext): Promise<LeaseGrant | undefined>;
  release(): Promise<void>;
  hold(): void;
  /** The line `/scufris status` prints. */
  status(): Promise<string>;
}

/** Pi creates a fork file the same way its own `--fork` does. */
const forkWithPi: ForkSession = async (lineage, cwd, sessionDir) => {
  const { SessionManager } = await import("@earendil-works/pi-coding-agent");
  return SessionManager.forkFrom(lineage, cwd, sessionDir).getSessionFile();
};

export function bindTerminal(
  pi: ExtensionAPI,
  options: TerminalOptions,
): TerminalControl | undefined {
  const trace: ServiceTrace = options.trace ?? (() => {});
  const retryMin = options.retryMinMs ?? RETRY_MIN_MS;
  const retryMax = options.retryMaxMs ?? RETRY_MAX_MS;
  const forkSession = options.forkSession ?? forkWithPi;
  const attachOnStart = options.attachOnStart !== false;
  const forkOnStart = options.forkOnStart !== false;

  let state: TerminalState = "independent";
  let context: ExtensionContext | undefined;
  let retryTimer: ReturnType<typeof setTimeout> | undefined;
  let retryDelay = retryMin;
  /** True while this module is performing its own session switch. */
  let switching = false;
  /** True once the person said to stop trying. */
  let holding = false;

  const notify = (message: string, level: "info" | "error") => {
    if (context?.hasUI) context.ui.notify(`Scufris: ${message}`, level);
    else if (level === "error") console.error(`scufris: ${message}`);
  };

  const binding: ServiceBinding | undefined = bindService(pi, {
    ...(options.role === undefined ? {} : { role: options.role }),
    ...(options.socketPath === undefined
      ? {}
      : { socketPath: options.socketPath }),
    ...(options.createClient === undefined
      ? {}
      : { createClient: options.createClient }),
    ...(options.channelTimeoutMs === undefined
      ? {}
      : { channelTimeoutMs: options.channelTimeoutMs }),
    lease: options.lease,
    trace,
    onLost: (reason) => lost(reason),
  });
  if (!binding) return undefined;

  const stopRetry = (): void => {
    if (retryTimer === undefined) return;
    clearTimeout(retryTimer);
    retryTimer = undefined;
  };

  const scheduleRetry = (): void => {
    if (holding || retryTimer !== undefined) return;
    const delay = retryDelay;
    retryDelay = Math.min(retryDelay * 2, retryMax);
    trace("reacquire_scheduled", { delay });
    const timer = setTimeout(() => {
      retryTimer = undefined;
      void attach().then((grant) => {
        if (!grant && state === "lost") scheduleRetry();
      });
    }, delay);
    timer.unref?.();
    retryTimer = timer;
  };

  const lost = (reason: string): void => {
    if (state === "independent") return;
    state = "lost";
    trace("terminal_lost", { reason });
    scheduleRetry();
  };

  const attach: TerminalControl["attach"] = async (commandContext) => {
    if (!binding) return undefined;
    let grant: LeaseGrant;
    try {
      grant = await binding.attach();
    } catch (error) {
      const refusal =
        error instanceof LeaseRefused
          ? `${error.code}: ${error.message}`
          : String(error);
      trace("attach_refused", { error: refusal });
      // A refusal is an answer, not a fault: this Pi goes on being an
      // ordinary Pi, with nothing of the conversation in it.
      if (state !== "lost") state = "independent";
      notify(`the agent was not handed over (${refusal})`, "error");
      return undefined;
    }
    state = "leased";
    holding = false;
    retryDelay = retryMin;
    stopRetry();
    notify(`holding the agent, generation ${grant.generation}`, "info");
    if (commandContext) await continueLineage(grant, commandContext);
    return grant;
  };

  /** Whether this session already carries the conversation the host recorded. */
  const onLineage = (lineage: string | undefined): boolean => {
    if (lineage === undefined) return true;
    const sessions = context?.sessionManager;
    if (!sessions) return true;
    if (sessions.getSessionFile() === lineage) return true;
    return sessions.getHeader?.()?.parentSession === lineage;
  };

  /** Forks the lineage into this directory and switches to the copy.
   *
   * Only from a command context, because only a command may switch sessions.
   * Phase 0 gate G3 measured the trap this guards: a switch starts a session,
   * which would ask for another switch. Asking only when this session is not
   * already on the lineage is what ends that.
   */
  const continueLineage = async (
    grant: LeaseGrant,
    commandContext: ExtensionCommandContext,
  ): Promise<void> => {
    const lineage = grant.lineageFile;
    if (lineage === undefined || onLineage(lineage)) return;
    const sessions = commandContext.sessionManager;
    let forked: string | undefined;
    try {
      forked = await forkSession(
        lineage,
        sessions.getCwd(),
        sessions.getSessionDir(),
      );
    } catch (error) {
      trace("fork_failed", { error: String(error) });
    }
    if (!forked) {
      notify(
        "the conversation could not be forked here; catching up on words only",
        "error",
      );
      return;
    }
    trace("fork_created", { lineage, file: forked });
    switching = true;
    try {
      await commandContext.switchSession(forked);
    } finally {
      switching = false;
    }
  };

  const release: TerminalControl["release"] = async () => {
    holding = true;
    stopRetry();
    retryDelay = retryMin;
    const wasLeased = state === "leased";
    state = "independent";
    await binding.release();
    trace("terminal_released", { wasLeased });
    notify(
      wasLeased
        ? "the agent went back to the background service"
        : "no longer trying to take the agent",
      "info",
    );
  };

  const hold: TerminalControl["hold"] = () => {
    holding = true;
    stopRetry();
    retryDelay = retryMin;
    if (state === "lost") state = "independent";
    trace("terminal_hold");
    notify("no longer trying to take the agent", "info");
  };

  const status: TerminalControl["status"] = async () => {
    const grant = binding.grant();
    const lines = [
      `state: ${state}`,
      `channel: ${binding.connected() ? "up" : "down"}`,
    ];
    if (grant) {
      lines.push(`generation: ${grant.generation}`);
      lines.push(`owner: ${grant.owner}`);
      lines.push(`lineage: ${grant.lineageFile ?? "none"}`);
    }
    if (options.lease.state) {
      try {
        const service = await options.lease.state();
        lines.push(`service: ${service.state} (${service.holder})`);
        if (service.lineageFile)
          lines.push(`service lineage: ${service.lineageFile}`);
      } catch (error) {
        lines.push(`service: unreachable (${String(error)})`);
      }
    }
    return lines.join("\n");
  };

  pi.registerCommand("scufris", {
    description:
      "Scufris terminal: status, attach (take the agent), release, hold",
    handler: async (args: string, commandContext: ExtensionCommandContext) => {
      context = commandContext;
      const action = args.trim().split(/\s+/)[0] ?? "";
      if (action === "" || action === "status") {
        const text = await status();
        if (commandContext.hasUI) commandContext.ui.notify(text, "info");
        else console.log(text);
        return;
      }
      if (action === "attach") {
        await attach(commandContext);
        return;
      }
      if (action === "release") {
        await release();
        return;
      }
      if (action === "hold") {
        hold();
        return;
      }
      notify(`unknown command: ${action}`, "error");
    },
  });

  pi.on("session_start", async (_event, ctx) => {
    context = ctx;
    if (!attachOnStart) return;
    const grant = await attach();
    if (!grant || !forkOnStart || onLineage(grant.lineageFile)) return;
    // Switching needs a command context, and a command is how one is had.
    // The guard above is what keeps this from asking again after the switch.
    trace("attach_dispatch", { lineage: grant.lineageFile });
    pi.sendUserMessage("/scufris attach", { expandPromptTemplates: true });
  });

  // The lineage has to stay one chain: the fork-back on release copies this
  // session, and a `/resume` into an unrelated one would hand that context to
  // the managed child. `/tree` stays allowed; it is the same file.
  pi.on("session_before_switch", (event) => {
    if (switching || state !== "leased") return {};
    trace("switch_cancelled", { reason: event.reason });
    notify(
      `this Pi holds the agent, so ${event.reason === "new" ? "/new" : "/resume"} is off. Use /scufris release first.`,
      "error",
    );
    return { cancel: true };
  });
  pi.on("session_before_fork", () => {
    if (switching || state !== "leased") return {};
    trace("fork_cancelled");
    notify(
      "this Pi holds the agent, so /fork is off. Use /scufris release first.",
      "error",
    );
    return { cancel: true };
  });

  return { state: () => state, attach, release, hold, status };
}

export default bindTerminal;
