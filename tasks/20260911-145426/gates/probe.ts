/** Phase 0 gate probe: one Pi extension that reports what Pi actually does.
 *
 * The terminal handoff design rests on six Pi behaviours that are not in the
 * 0.85 documentation. This extension is how they are measured rather than
 * assumed: it appends one JSON line per observed event to `SCUFRIS_GATE_LOG`
 * and exposes the two commands the gates need.
 *
 * It is measurement scaffolding, not shipped code. Nothing in
 * `agent/extensions/` imports it.
 */
import { appendFileSync } from "node:fs";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";

/** The notice a cancelled session switch shows, so a gate can see it drawn. */
export const GATE_NOTICE = "gate probe cancelled the session switch";

/** A dispatch that switches sessions starts a session, which dispatches
 * again. Whether one guard stops that loop is part of what G3 measures. */
let dispatched = false;

function trace(event: string, fields: Record<string, unknown> = {}): void {
  const path = process.env.SCUFRIS_GATE_LOG;
  if (!path) return;
  try {
    appendFileSync(
      path,
      `${JSON.stringify({ at: new Date().toISOString(), event, ...fields })}\n`,
    );
  } catch {
    // A trace that cannot be written is not worth a failed run.
  }
}

function sessionFields(context: unknown): Record<string, unknown> {
  const sessions = (context as { sessionManager?: unknown } | undefined)
    ?.sessionManager as
    | {
        getSessionId?: () => string;
        getSessionFile?: () => string;
        getCwd?: () => string;
      }
    | undefined;
  if (!sessions) return {};
  return {
    session: sessions.getSessionId?.(),
    file: sessions.getSessionFile?.(),
    cwd: sessions.getCwd?.(),
  };
}

export default function gateProbe(pi: ExtensionAPI): void {
  pi.registerCommand("gatecmd", {
    description: "Gate probe: record that an extension command ran",
    handler: async (args, ctx) => {
      trace("command_gatecmd", { args });
      if (ctx.hasUI) ctx.ui.notify("gatecmd ran", "info");
    },
  });

  pi.registerCommand("gateattach", {
    description: "Gate probe: switch to the session named by the environment",
    handler: async (_args, ctx) => {
      const target = process.env.SCUFRIS_GATE_SWITCH_TARGET;
      trace("gateattach_start", { target, ...sessionFields(ctx) });
      if (!target) return;
      try {
        const result = await ctx.switchSession(target, {
          withSession: async (next) => {
            trace("gateattach_withsession", sessionFields(next));
          },
        });
        trace("gateattach_done", { cancelled: result?.cancelled ?? false });
      } catch (error) {
        trace("gateattach_failed", { error: String(error) });
      }
    },
  });

  pi.on("input", (event) => {
    trace("input", {
      text: event.text,
      source: event.source,
      images: event.images?.length ?? 0,
      streaming: event.streamingBehavior ?? null,
    });
  });

  pi.on("session_start", (event, ctx) => {
    trace("session_start", { reason: event.reason, ...sessionFields(ctx) });
    const catchUp = process.env.SCUFRIS_GATE_CATCH_UP;
    if (catchUp) {
      const deliverAs = (process.env.SCUFRIS_GATE_CATCH_UP_MODE ??
        "nextTurn") as "steer" | "followUp" | "nextTurn";
      try {
        pi.sendMessage(
          {
            customType: "scufris-gate-catch-up",
            content: catchUp,
            display: false,
          },
          { deliverAs },
        );
        trace("catch_up_injected", { deliverAs });
      } catch (error) {
        trace("catch_up_failed", { error: String(error) });
      }
    }
    const dispatch = process.env.SCUFRIS_GATE_DISPATCH;
    const guarded = process.env.SCUFRIS_GATE_DISPATCH_ONCE === "1";
    if (dispatch && !(guarded && dispatched)) {
      dispatched = true;
      try {
        pi.sendUserMessage(dispatch, { expandPromptTemplates: true });
        trace("dispatched", { dispatch, guarded });
      } catch (error) {
        trace("dispatch_failed", { error: String(error) });
      }
    }
  });

  pi.on("session_before_switch", (event, ctx) => {
    trace("session_before_switch", {
      reason: event.reason,
      target: event.targetSessionFile,
    });
    if (process.env.SCUFRIS_GATE_CANCEL_SWITCH !== "1") return undefined;
    if (ctx.hasUI) ctx.ui.notify(GATE_NOTICE, "info");
    trace("switch_cancelled", { reason: event.reason });
    return { cancel: true };
  });

  pi.on("session_before_fork", (event, ctx) => {
    trace("session_before_fork", { entry: event.entryId });
    if (process.env.SCUFRIS_GATE_CANCEL_SWITCH !== "1") return undefined;
    if (ctx.hasUI) ctx.ui.notify(GATE_NOTICE, "info");
    trace("fork_cancelled", {});
    return { cancel: true };
  });

  pi.on("session_before_compact", (event) => {
    trace("session_before_compact", { reason: event.reason });
    return undefined;
  });

  pi.on("agent_start", () => trace("agent_start"));
  pi.on("agent_settled", () => trace("agent_settled"));
  pi.on("session_shutdown", () => trace("session_shutdown"));
}
