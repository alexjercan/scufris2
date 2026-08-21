import { randomBytes } from "node:crypto";
import { fileURLToPath } from "node:url";
import { StringEnum, Type } from "@earendil-works/pi-ai";
import {
  defineTool,
  type ExtensionAPI,
  type ExtensionContext,
} from "@earendil-works/pi-coding-agent";
import { runPrivateHelper, toolResult } from "./shared/runtime.ts";

const jobHelperPath = fileURLToPath(
  new URL("../../scripts/scufris-job", import.meta.url),
);

interface OwnedJob {
  job_id: string;
  harness: "pi" | "claude";
  feature: string;
  state: string;
  summary: string;
  offset: number;
  tail: string;
  inode: number | null;
  window_alive: boolean;
  protocolErrors: Set<string>;
  exitReported: boolean;
}

interface SpawnResult {
  job_id: string;
  state: string;
  harness: "pi" | "claude";
  feature: string;
  model: string;
  thinking: string;
  message: string;
}

interface PollResult {
  jobs: Array<{
    job_id: string;
    events: string[];
    errors: string[];
    offset: number;
    tail: string;
    inode: number | null;
    window_alive: boolean;
  }>;
}

interface InspectResult {
  job_id: string;
  harness: string;
  feature: string;
  state: string;
  summary: string;
  window_alive: boolean;
  events: string[];
  report?: string;
}

async function runHelper<T>(
  command: string,
  request: unknown,
  signal?: AbortSignal,
  timeoutMs = 30_000,
): Promise<T> {
  const envelope = await runPrivateHelper<T>(
    jobHelperPath,
    command,
    request,
    signal,
    timeoutMs,
  );
  if (!envelope.ok || envelope.result === undefined) {
    throw new Error(envelope.error ?? "Scufris helper failed");
  }
  return envelope.result;
}

function generatedJobId(): string {
  return randomBytes(6).toString("hex");
}

function parseEvent(line: string): { state: string; summary: string } {
  const separator = line.indexOf(": ");
  return {
    state: line.slice(0, separator),
    summary: line.slice(separator + 2),
  };
}

export default function scufris(pi: ExtensionAPI): void {
  const jobs = new Map<string, OwnedJob>();
  let timer: ReturnType<typeof setInterval> | undefined;
  let pollRunning = false;
  let pollError: string | undefined;
  let shuttingDown = false;
  let context: ExtensionContext | undefined;

  const sendJobEvent = (job: OwnedJob, event: string, triggerTurn: boolean) => {
    pi.sendMessage(
      {
        customType: "scufris-job-event",
        content: `Scufris job ${job.job_id}: ${event}`,
        display: true,
        details: { job_id: job.job_id, event },
      },
      { deliverAs: "followUp", triggerTurn },
    );
  };

  const poll = async () => {
    if (pollRunning || shuttingDown || !context) return;
    const active = [...jobs.values()].filter(
      (job) =>
        job.state !== "stopped" &&
        job.state !== "done" &&
        job.state !== "failed",
    );
    if (active.length === 0) {
      context.ui.setStatus("scufris", undefined);
      return;
    }
    pollRunning = true;
    try {
      const result = await runHelper<PollResult>("poll", {
        jobs: active.map((job) => ({
          job_id: job.job_id,
          offset: job.offset,
          tail: job.tail,
          inode: job.inode,
        })),
      });
      if (shuttingDown) return;
      const routine: string[] = [];
      for (const update of result.jobs) {
        const job = jobs.get(update.job_id);
        if (!job) continue;
        job.offset = update.offset;
        job.tail = update.tail;
        job.inode = update.inode;
        job.window_alive = update.window_alive;

        for (const error of update.errors) {
          const errorIdentity = `${update.offset}:${error}`;
          if (job.protocolErrors.has(errorIdentity)) continue;
          job.protocolErrors.add(errorIdentity);
          context.ui.notify(`Job ${job.job_id}: ${error}`, "error");
          sendJobEvent(job, `protocol-error: ${error}`, true);
        }
        for (const line of update.events) {
          const event = parseEvent(line);
          job.state = event.state;
          job.summary = event.summary;
          if (event.state === "working" || event.state === "review-ready") {
            routine.push(`${job.job_id}: ${line}`);
          } else {
            sendJobEvent(job, line, true);
          }
        }
        if (
          !job.window_alive &&
          !job.exitReported &&
          job.state !== "done" &&
          job.state !== "failed" &&
          job.state !== "stopped"
        ) {
          job.exitReported = true;
          job.state = "failed";
          job.summary = "worker exited without terminal status";
          context.ui.notify(`Job ${job.job_id} exited`, "error");
          sendJobEvent(
            job,
            "failed: worker exited without terminal status",
            true,
          );
        }
      }
      if (routine.length > 0) context.ui.notify(routine.join("\n"), "info");
      const running = active.filter((job) => job.window_alive).length;
      context.ui.setStatus(
        "scufris",
        running > 0
          ? `${running} delegated job${running === 1 ? "" : "s"}`
          : undefined,
      );
      pollError = undefined;
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      if (!shuttingDown && message !== pollError) {
        context.ui.notify(message, "error");
      }
      pollError = message;
    } finally {
      pollRunning = false;
    }
  };

  const spawnTool = defineTool({
    name: "scufris_agent_spawn",
    label: "Spawn delegated agent",
    description:
      "Start one independent Pi or Claude coding worker in an isolated worktree.",
    parameters: Type.Object(
      {
        harness: StringEnum(["pi", "claude"] as const),
        instructions: Type.String({ minLength: 1, maxLength: 262_144 }),
        model: Type.Optional(Type.String({ minLength: 1, maxLength: 200 })),
        thinking: Type.Optional(
          StringEnum([
            "off",
            "minimal",
            "low",
            "medium",
            "high",
            "xhigh",
            "max",
          ] as const),
        ),
      },
      { additionalProperties: false },
    ),
    async execute(_id, params, signal, _update, ctx) {
      try {
        const result = await runHelper<SpawnResult>(
          "spawn",
          {
            job_id: generatedJobId(),
            harness: params.harness,
            instructions: params.instructions,
            project_root: ctx.cwd,
            ...(params.model === undefined ? {} : { model: params.model }),
            ...(params.thinking === undefined
              ? {}
              : { thinking: params.thinking }),
          },
          signal,
          120_000,
        );
        if (shuttingDown) {
          await runHelper("stop", { job_id: result.job_id }, undefined, 15_000);
          return toolResult({ error: "Pi session is shutting down" }, true);
        }
        jobs.set(result.job_id, {
          job_id: result.job_id,
          harness: result.harness,
          feature: result.feature,
          state: result.state,
          summary: "worker starting",
          offset: 0,
          tail: "",
          inode: null,
          window_alive: true,
          protocolErrors: new Set(),
          exitReported: false,
        });
        void poll();
        return toolResult(result);
      } catch (error) {
        return toolResult(
          { error: error instanceof Error ? error.message : String(error) },
          true,
        );
      }
    },
  });

  const listTool = defineTool({
    name: "scufris_agent_list",
    label: "List delegated agents",
    description: "List jobs owned by this Pi session.",
    parameters: Type.Object({}, { additionalProperties: false }),
    async execute() {
      return toolResult({
        jobs: [...jobs.values()].map((job) => ({
          job_id: job.job_id,
          harness: job.harness,
          state: job.state,
          summary: job.summary,
          feature: job.feature,
          window_alive: job.window_alive,
        })),
      });
    },
  });

  const inspectTool = defineTool({
    name: "scufris_agent_inspect",
    label: "Inspect delegated agent",
    description:
      "Inspect one owned job and optionally include its bounded report.",
    parameters: Type.Object(
      {
        job_id: Type.String({ pattern: "^[a-z0-9]{12}$" }),
        include_report: Type.Optional(Type.Boolean({ default: false })),
      },
      { additionalProperties: false },
    ),
    async execute(_id, params, signal) {
      if (!jobs.has(params.job_id)) {
        return toolResult(
          { error: "job is not owned by this Pi session" },
          true,
        );
      }
      try {
        return toolResult(
          await runHelper<InspectResult>(
            "inspect",
            {
              job_id: params.job_id,
              include_report: params.include_report ?? false,
            },
            signal,
          ),
        );
      } catch (error) {
        return toolResult(
          { error: error instanceof Error ? error.message : String(error) },
          true,
        );
      }
    },
  });

  const sendTool = defineTool({
    name: "scufris_agent_send",
    label: "Steer delegated agent",
    description: "Submit one literal steering message to an owned worker.",
    parameters: Type.Object(
      {
        job_id: Type.String({ pattern: "^[a-z0-9]{12}$" }),
        message: Type.String({
          minLength: 1,
          maxLength: 16_384,
          pattern: "^[^\\r\\n]+$",
        }),
      },
      { additionalProperties: false },
    ),
    async execute(_id, params, signal) {
      if (!jobs.has(params.job_id)) {
        return toolResult(
          { error: "job is not owned by this Pi session" },
          true,
        );
      }
      try {
        return toolResult(await runHelper("send", params, signal, 20_000));
      } catch (error) {
        return toolResult(
          { error: error instanceof Error ? error.message : String(error) },
          true,
        );
      }
    },
  });

  const stopTool = defineTool({
    name: "scufris_agent_stop",
    label: "Stop delegated agent",
    description:
      "Stop one owned worker without deleting its worktree or evidence.",
    parameters: Type.Object(
      { job_id: Type.String({ pattern: "^[a-z0-9]{12}$" }) },
      { additionalProperties: false },
    ),
    async execute(_id, params, signal) {
      const job = jobs.get(params.job_id);
      if (!job) {
        return toolResult(
          { error: "job is not owned by this Pi session" },
          true,
        );
      }
      try {
        const result = await runHelper<{ job_id: string; state: string }>(
          "stop",
          params,
          signal,
          15_000,
        );
        job.state = "stopped";
        job.summary = "stopped by Scufris";
        job.window_alive = false;
        return toolResult(result);
      } catch (error) {
        return toolResult(
          { error: error instanceof Error ? error.message : String(error) },
          true,
        );
      }
    },
  });

  pi.registerTool(spawnTool);
  pi.registerTool(listTool);
  pi.registerTool(inspectTool);
  pi.registerTool(sendTool);
  pi.registerTool(stopTool);

  pi.on("session_start", async (_event, ctx) => {
    context = ctx;
    shuttingDown = false;
    try {
      const result = await runHelper<{ job_ids: string[] }>("orphans", {});
      if (result.job_ids.length > 0) {
        const content = `Possible Scufris orphan jobs: ${result.job_ids.join(", ")}. They were not adopted.`;
        ctx.ui.notify(content, "warning");
        pi.sendMessage(
          {
            customType: "scufris-job-event",
            content,
            display: true,
            details: { event: "orphans", job_ids: result.job_ids },
          },
          { deliverAs: "followUp", triggerTurn: true },
        );
      }
    } catch (error) {
      ctx.ui.notify(
        `Scufris orphan scan failed: ${error instanceof Error ? error.message : String(error)}`,
        "error",
      );
    }
    if (!shuttingDown) timer = setInterval(() => void poll(), 1_000);
  });

  pi.on("session_shutdown", async () => {
    shuttingDown = true;
    if (timer) clearInterval(timer);
    timer = undefined;
    while (pollRunning) {
      await new Promise((resolve) => setTimeout(resolve, 25));
    }
    const owned = [...jobs.values()].filter((job) => job.window_alive);
    await Promise.allSettled(
      owned.map((job) =>
        runHelper("stop", { job_id: job.job_id }, undefined, 15_000),
      ),
    );
    context?.ui.setStatus("scufris", undefined);
    jobs.clear();
    context = undefined;
  });
}
