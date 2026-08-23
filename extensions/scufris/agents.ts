import { randomBytes } from "node:crypto";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { StringEnum, Type } from "@earendil-works/pi-ai";
import {
  defineTool,
  type ExtensionAPI,
  type ExtensionContext,
} from "@earendil-works/pi-coding-agent";
import { runPrivateHelper, toolResult } from "./shared/runtime.ts";
import {
  initialWalkthroughState,
  parseWalkthrough,
  saveWalkthroughState,
  startWalkthroughServer,
  type ReviewComment,
  type WalkthroughState,
} from "./walkthrough.ts";

const jobsHelperPath = fileURLToPath(
  new URL("../../tools/jobs/scufris-jobs", import.meta.url),
);

const CONTEXT_ID = /^[a-f0-9]{24}$/;
const JOB_ID = /^[a-f0-9]{12}$/;
const MILESTONE = /^[a-z][a-z0-9]*(?:-[a-z0-9]+)*$/;
const TERMINAL_STATES = new Set(["done", "failed", "stopped", "landed"]);
export const QUICK_REVIEW_TOOL = "scufris_job_quick_review";
export const PLANNOTATOR_REVIEW_TOOL = "scufris_job_plannotator_review";

export type WorkerEventType =
  | "working"
  | "needs-decision"
  | "blocked"
  | "ready"
  | "done"
  | "failed";

export interface WorkerEvent {
  type: WorkerEventType;
  value: string;
}

export function parseWorkerEvent(line: string): WorkerEvent | undefined {
  const separator = line.indexOf(": ");
  if (separator <= 0) return undefined;
  const type = line.slice(0, separator) as WorkerEventType;
  const value = line.slice(separator + 2);
  if (
    ![
      "working",
      "needs-decision",
      "blocked",
      "ready",
      "done",
      "failed",
    ].includes(type) ||
    !value ||
    /[\x00-\x1f\x7f]/.test(value) ||
    (type === "ready" && !MILESTONE.test(value))
  ) {
    return undefined;
  }
  return { type, value };
}

export function workerEventWakes(type: WorkerEventType): boolean {
  return type !== "working";
}

interface ContextResult {
  project: string;
  project_root: string;
  configured: boolean;
  fingerprint: string;
  markdown: string;
  diagnostic: string | null;
}

interface ResolvedContext extends ContextResult {
  context_id: string;
  consumed: boolean;
}

interface SpawnResult {
  job_id: string;
  state: string;
  project: string | null;
  workspace: "temporary" | "project" | "sprout" | "review";
  feature: string | null;
  harness: "pi" | "claude";
  model: string;
  thinking: string;
  tmux_session: string;
  message: string;
}

interface OwnedJob extends SpawnResult {
  context_id?: string;
  context_fingerprint?: string;
  summary: string;
  offset: number;
  tail: string;
  inode: number | null;
  window_alive: boolean;
  exit_reported: boolean;
  quick_review?: { close(): Promise<void>; revision: string };
}

interface QuickReviewBuild {
  job_id: string;
  cwd: string;
  default_branch: string;
  base_revision: string;
  revision: string;
  artifact: string;
  state: string;
  model: string;
  thinking: string;
}

interface QuickReviewSnapshot {
  base_revision: string;
  revision: string;
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

async function runHelper<T>(
  command: string,
  request: unknown,
  signal?: AbortSignal,
  timeoutMs = 30_000,
): Promise<T> {
  const envelope = await runPrivateHelper<T>(
    jobsHelperPath,
    command,
    request,
    signal,
    timeoutMs,
  );
  if (!envelope.ok || envelope.result === undefined) {
    throw new Error(envelope.error ?? "Scufris jobs helper failed");
  }
  return envelope.result;
}

function contextId(): string {
  return randomBytes(12).toString("hex");
}

function jobId(): string {
  return randomBytes(6).toString("hex");
}

function activeJobPrompt(jobs: Iterable<OwnedJob>): string {
  const active = [...jobs].filter((job) => !TERMINAL_STATES.has(job.state));
  const index = active.length
    ? active
        .slice(-32)
        .map(
          (job) =>
            `- ${job.job_id}: ${job.project ?? "general"}, ${job.state}, ${job.summary.slice(0, 160)}${job.context_id ? `, context ${job.context_id}` : ""}`,
        )
        .join("\n")
    : "- none";
  return `Scufris resolves project workflow preferences per job.

Before planning every new project job, call scufris_project_context with an
opaque project ID from scufris_projects. Follow the returned project guidance
unless the user's explicit request overrides it or it is impossible. Use no
project context for general work. A project context creates exactly one job.
Treat ready events as completed milestone hints: inspect the job and decide
what follows; never route from the milestone slug alone.

Active Scufris jobs:
${index}`;
}

export default function scufrisJobs(pi: ExtensionAPI): void {
  if (process.env.SCUFRIS_ROLE !== "orchestrator") return;

  const contexts = new Map<string, ResolvedContext>();
  const jobs = new Map<string, OwnedJob>();
  let timer: ReturnType<typeof setInterval> | undefined;
  let polling = false;
  let shuttingDown = false;
  let extensionContext: ExtensionContext | undefined;
  let pollError: string | undefined;

  const sendEvent = (job: OwnedJob, line: string, triggerTurn: boolean) => {
    pi.sendMessage(
      {
        customType: "scufris-job-event",
        content: `Scufris job ${job.job_id} (${job.project ?? "general"}): ${line}. Inspect the pinned job context, prompt, report, and state before deciding what follows.`,
        display: true,
        details: {
          job_id: job.job_id,
          project: job.project,
          context_id: job.context_id,
          event: line,
        },
      },
      { deliverAs: "followUp", triggerTurn },
    );
  };

  const poll = async () => {
    if (polling || shuttingDown || !extensionContext) return;
    const active = [...jobs.values()].filter(
      (job) => !TERMINAL_STATES.has(job.state),
    );
    if (active.length === 0) {
      if (extensionContext.hasUI)
        extensionContext.ui.setStatus("scufris", undefined);
      return;
    }
    polling = true;
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
      const progress: string[] = [];
      for (const update of result.jobs) {
        const job = jobs.get(update.job_id);
        if (!job) continue;
        job.offset = update.offset;
        job.tail = update.tail;
        job.inode = update.inode;
        job.window_alive = update.window_alive;
        for (const error of update.errors) {
          if (extensionContext.hasUI)
            extensionContext.ui.notify(`Job ${job.job_id}: ${error}`, "error");
          sendEvent(job, `failed: ${error}`, true);
        }
        for (const line of update.events) {
          const event = parseWorkerEvent(line);
          if (!event) continue;
          job.state = event.type;
          job.summary = event.value;
          if (workerEventWakes(event.type)) sendEvent(job, line, true);
          else progress.push(`${job.job_id}: ${event.value}`);
        }
        if (
          !job.window_alive &&
          !job.exit_reported &&
          !TERMINAL_STATES.has(job.state)
        ) {
          job.exit_reported = true;
          job.state = "failed";
          job.summary = "worker exited without a terminal event";
          sendEvent(job, `failed: ${job.summary}`, true);
        }
      }
      if (progress.length > 0 && extensionContext.hasUI)
        extensionContext.ui.notify(progress.join("\n"), "info");
      const running = active.filter((job) => job.window_alive).length;
      if (extensionContext.hasUI)
        extensionContext.ui.setStatus(
          "scufris",
          running > 0
            ? `${running} delegated job${running === 1 ? "" : "s"}`
            : undefined,
        );
      pollError = undefined;
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      if (!shuttingDown && message !== pollError && extensionContext.hasUI)
        extensionContext.ui.notify(message, "error");
      pollError = message;
    } finally {
      polling = false;
    }
  };

  pi.registerTool(
    defineTool({
      name: "scufris_projects",
      label: "List Scufris projects",
      description:
        "List opaque project IDs available for project-specific Scufris jobs.",
      promptSnippet: "List opaque projects available to Scufris",
      parameters: Type.Object({}, { additionalProperties: false }),
      async execute() {
        try {
          return toolResult(await runHelper("projects", {}));
        } catch (error) {
          throw new Error(
            error instanceof Error ? error.message : String(error),
          );
        }
      },
    }),
  );

  pi.registerTool(
    defineTool({
      name: "scufris_project_context",
      label: "Load project workflow context",
      description:
        "Load one project's advisory .scufris.toml workflow preferences before planning one new job. Returns a single-use context ID.",
      promptSnippet: "Load project workflow preferences for one new job",
      promptGuidelines: [
        "Call scufris_project_context before planning every new project job, even when another job already uses that project.",
      ],
      parameters: Type.Object(
        {
          project: Type.String({
            pattern: "^[A-Za-z0-9_.-]+(?:/[A-Za-z0-9_.-]+)*$",
          }),
        },
        { additionalProperties: false },
      ),
      async execute(_id, params, signal) {
        const result = await runHelper<ContextResult>(
          "context",
          params,
          signal,
        );
        const resolved: ResolvedContext = {
          ...result,
          context_id: contextId(),
          consumed: false,
        };
        contexts.set(resolved.context_id, resolved);
        return toolResult({
          context_id: resolved.context_id,
          project: resolved.project,
          configured: resolved.configured,
          fingerprint: resolved.fingerprint,
          diagnostic: resolved.diagnostic,
          project_context: resolved.markdown,
          instruction:
            "Compose this job now. Pass this context ID exactly once to scufris_job_spawn.",
        });
      },
    }),
  );

  pi.registerTool(
    defineTool({
      name: "scufris_job_spawn",
      label: "Spawn Scufris job",
      description:
        "Start one independent Pi or Claude worker. Use a fresh project context ID for project work or omit it for general work.",
      promptSnippet: "Start an independent project or general worker",
      promptGuidelines: [
        "Use scufris_job_spawn for work expected to take minutes. Select workspace sprout only when project guidance or the user asks for isolated repository work.",
      ],
      executionMode: "sequential",
      parameters: Type.Object(
        {
          instructions: Type.String({ minLength: 1 }),
          context_id: Type.Optional(Type.String({ pattern: "^[a-f0-9]{24}$" })),
          harness: Type.Optional(StringEnum(["pi", "claude"] as const)),
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
          workspace: Type.Optional(
            StringEnum(["temporary", "project", "sprout"] as const),
          ),
          feature: Type.Optional(
            Type.String({
              pattern: "^[a-z0-9]+(?:-[a-z0-9]+)*$",
              maxLength: 48,
            }),
          ),
          review_of: Type.Optional(
            Type.String({
              description:
                "Owned project job to inspect read-only in this new review job.",
              pattern: "^[a-f0-9]{12}$",
            }),
          ),
        },
        { additionalProperties: false },
      ),
      async execute(_id, params, signal, _update, ctx) {
        let resolved: ResolvedContext | undefined;
        if (params.context_id !== undefined) {
          if (!CONTEXT_ID.test(params.context_id))
            throw new Error("invalid project context ID");
          resolved = contexts.get(params.context_id);
          if (!resolved || resolved.consumed)
            throw new Error(
              "project context is unavailable or already consumed",
            );
        }
        if (params.context_id === undefined && params.workspace === "project")
          throw new Error("project workspace requires a project context");
        if (params.context_id === undefined && params.workspace === "sprout")
          throw new Error("sprout workspace requires a project context");
        if (params.review_of !== undefined) {
          const source = jobs.get(params.review_of);
          if (!source)
            throw new Error(
              "review source is not owned by this Scufris session",
            );
          if (!resolved || source.project !== resolved.project)
            throw new Error(
              "review source and fresh project context must select the same project",
            );
        }
        const generatedJobId = jobId();
        const result = await runHelper<SpawnResult>(
          "spawn",
          {
            job_id: generatedJobId,
            instructions: params.instructions,
            owner_session: ctx.sessionManager.getSessionId(),
            ...(resolved
              ? {
                  project: resolved.project,
                  project_root: resolved.project_root,
                  context_markdown: resolved.markdown,
                  context_fingerprint: resolved.fingerprint,
                }
              : {}),
            ...(params.harness === undefined
              ? {}
              : { harness: params.harness }),
            ...(params.model === undefined ? {} : { model: params.model }),
            ...(params.thinking === undefined
              ? {}
              : { thinking: params.thinking }),
            ...(params.workspace === undefined
              ? {}
              : { workspace: params.workspace }),
            ...(params.feature === undefined
              ? {}
              : { feature: params.feature }),
            ...(params.review_of === undefined
              ? {}
              : { review_of: params.review_of }),
          },
          signal,
          120_000,
        );
        if (resolved) {
          resolved.consumed = true;
          contexts.delete(resolved.context_id);
        }
        if (shuttingDown) {
          await runHelper(
            "stop",
            { job_id: generatedJobId },
            undefined,
            15_000,
          );
          throw new Error("Pi session is shutting down");
        }
        const job: OwnedJob = {
          ...result,
          ...(resolved
            ? {
                context_id: resolved.context_id,
                context_fingerprint: resolved.fingerprint,
              }
            : {}),
          summary: "worker starting",
          offset: 0,
          tail: "",
          inode: null,
          window_alive: true,
          exit_reported: false,
        };
        jobs.set(job.job_id, job);
        void poll();
        return toolResult({
          ...result,
          ...(resolved ? { context_id: resolved.context_id } : {}),
        });
      },
    }),
  );

  pi.registerTool(
    defineTool({
      name: PLANNOTATOR_REVIEW_TOOL,
      label: "Open Plannotator review",
      description:
        "Open an explicit Plannotator since-base code review for one owned Sprout job. The review result wakes foreground Scufris; it does not land automatically.",
      executionMode: "sequential",
      parameters: Type.Object(
        { job_id: Type.String({ pattern: "^[a-f0-9]{12}$" }) },
        { additionalProperties: false },
      ),
      async execute(_id, params, signal) {
        const job = jobs.get(params.job_id);
        if (!job) throw new Error("job is not owned by this Scufris session");
        const target = await runHelper<{
          cwd: string;
          default_branch: string;
        }>("review-target", params, signal);
        const requestId = `plannotator-review-${params.job_id}-${randomBytes(6).toString("hex")}`;
        pi.events.emit("plannotator:request", {
          requestId,
          action: "code-review",
          payload: {
            cwd: target.cwd,
            defaultBranch: target.default_branch,
            diffType: "since-base",
          },
          respond: (response: unknown) => {
            if (shuttingDown || jobs.get(job.job_id) !== job) return;
            let encoded: string;
            try {
              encoded = JSON.stringify(response);
            } catch {
              encoded = '{"status":"error","error":"unserializable response"}';
            }
            if (Buffer.byteLength(encoded, "utf8") > 16 * 1024)
              encoded =
                '{"status":"error","error":"review response exceeded the mediation limit"}';
            pi.sendMessage(
              {
                customType: "scufris-job-event",
                content: `Plannotator review completed for Scufris job ${job.job_id}. Inspect this structured result and decide what follows from the project preferences: ${encoded}`,
                display: true,
                details: {
                  job_id: job.job_id,
                  context_id: job.context_id,
                  plannotator_review: response,
                },
              },
              { deliverAs: "followUp", triggerTurn: true },
            );
          },
        });
        return toolResult({
          job_id: job.job_id,
          request_id: requestId,
          state: "plannotator-review-opened",
        });
      },
    }),
  );

  pi.registerTool(
    defineTool({
      name: QUICK_REVIEW_TOOL,
      label: "Open Quick Review",
      description:
        "Generate and open the custom Scufris Quick Review walkthrough for one owned Sprout job. This is separate from Plannotator and never lands automatically.",
      executionMode: "sequential",
      parameters: Type.Object(
        {
          job_id: Type.String({ pattern: "^[a-f0-9]{12}$" }),
          model: Type.Optional(
            Type.String({
              description:
                "Pi model used to generate and explain the walkthrough.",
              minLength: 1,
              maxLength: 200,
            }),
          ),
          thinking: Type.Optional(
            StringEnum([
              "off",
              "minimal",
              "low",
              "medium",
              "high",
              "xhigh",
            ] as const),
          ),
        },
        { additionalProperties: false },
      ),
      async execute(_id, params, signal, _update, ctx) {
        const job = jobs.get(params.job_id);
        if (!job) throw new Error("job is not owned by this Scufris session");
        if (job.quick_review)
          throw new Error("Quick Review is already open for this job");

        const built = await runHelper<QuickReviewBuild>(
          "quick-review-build",
          {
            job_id: job.job_id,
            model: params.model ?? "openai-codex/gpt-5.6-sol",
            thinking: params.thinking ?? "medium",
          },
          signal,
          920_000,
        );
        const document = parseWalkthrough(readFileSync(built.artifact, "utf8"));
        if (
          document.baseRevision !== built.base_revision ||
          document.revision !== built.revision
        )
          throw new Error("Quick Review artifact revisions do not match");
        const state: WalkthroughState = initialWalkthroughState(document);
        saveWalkthroughState(built.state, state);
        const contexts = new Map<string, string>();
        for (const section of document.sections) {
          const result = await runHelper<{ content: string }>(
            "quick-review-context",
            {
              job_id: job.job_id,
              base_revision: built.base_revision,
              revision: built.revision,
              file: section.file,
            },
            signal,
          );
          contexts.set(section.id, result.content);
        }

        const verify = async () => {
          const snapshot = await runHelper<QuickReviewSnapshot>(
            "quick-review-snapshot",
            { job_id: job.job_id },
          );
          if (
            snapshot.base_revision !== built.base_revision ||
            snapshot.revision !== built.revision
          )
            throw new Error("Quick Review revision changed");
        };
        const finish = async (
          milestone: string,
          detail: Record<string, unknown>,
          invalidate: boolean,
        ) => {
          const active = job.quick_review;
          job.quick_review = undefined;
          if (active) void active.close().catch(() => undefined);
          if (invalidate)
            await runHelper("invalidate-quick-review", {
              job_id: job.job_id,
            });
          job.state = "ready";
          job.summary = milestone;
          pi.sendMessage(
            {
              customType: "scufris-job-event",
              content: `Scufris job ${job.job_id} (${job.project ?? "general"}): ready: ${milestone}. Inspect the Quick Review result and decide what follows from the project preferences.`,
              display: true,
              details: {
                job_id: job.job_id,
                context_id: job.context_id,
                quick_review: detail,
              },
            },
            { deliverAs: "followUp", triggerTurn: true },
          );
        };
        const server = await startWalkthroughServer(document, state, {
          verify,
          persist: (next) => saveWalkthroughState(built.state, next),
          context: (section) =>
            contexts.get(section.id) ?? "Context unavailable.",
          explain: async (section, question) => {
            const answer = await runHelper<{ answer: string }>(
              "quick-review-question",
              {
                job_id: job.job_id,
                revision: built.revision,
                section,
                question,
                model: built.model,
                thinking: built.thinking,
              },
              undefined,
              200_000,
            );
            return answer.answer;
          },
          requestChanges: async (feedback) => {
            await verify();
            await runHelper("send", {
              job_id: job.job_id,
              message: feedback,
            });
            await finish("quick-review-feedback-submitted", { feedback }, true);
          },
          fullDiff: async () => {
            await verify();
            pi.events.emit("plannotator:request", {
              requestId: `quick-review-full-diff-${job.job_id}-${randomBytes(6).toString("hex")}`,
              action: "code-review",
              payload: {
                cwd: built.cwd,
                defaultBranch: built.default_branch,
                diffType: "since-base",
              },
              respond: () => undefined,
            });
          },
          approved: async (comments: ReviewComment[]) => {
            await verify();
            await finish(
              "quick-review-approved",
              { comments, revision: built.revision },
              false,
            );
          },
        });
        job.quick_review = { close: server.close, revision: built.revision };
        server.open();
        if (ctx.hasUI)
          ctx.ui.notify(
            `Quick Review for job ${job.job_id}: ${server.url}`,
            "info",
          );
        return toolResult({
          job_id: job.job_id,
          state: "quick-review-opened",
          revision: built.revision,
          sections: document.sections.length,
        });
      },
    }),
  );

  pi.registerTool(
    defineTool({
      name: "scufris_job_land",
      label: "Land Scufris job",
      description:
        "Explicitly land one owned Sprout job after the selected workflow has supplied user approval. This tool never runs automatically.",
      executionMode: "sequential",
      parameters: Type.Object(
        {
          job_id: Type.String({ pattern: "^[a-f0-9]{12}$" }),
          subject: Type.String({ minLength: 1, pattern: "^[^\\r\\n]+$" }),
          remove_workspace: Type.Optional(Type.Boolean({ default: true })),
        },
        { additionalProperties: false },
      ),
      async execute(_id, params, signal) {
        const job = jobs.get(params.job_id);
        if (!job) throw new Error("job is not owned by this Scufris session");
        const result = await runHelper(
          "land",
          {
            job_id: params.job_id,
            subject: params.subject,
            remove_workspace: params.remove_workspace ?? true,
          },
          signal,
          120_000,
        );
        job.state = "landed";
        job.summary = "explicitly landed by foreground Scufris";
        job.window_alive = false;
        return toolResult(result);
      },
    }),
  );

  pi.registerTool(
    defineTool({
      name: "scufris_job_list",
      label: "List Scufris jobs",
      description: "List jobs owned by this Scufris session.",
      parameters: Type.Object({}, { additionalProperties: false }),
      async execute() {
        return toolResult({
          jobs: [...jobs.values()].map((job) => ({
            job_id: job.job_id,
            project: job.project,
            context_id: job.context_id,
            state: job.state,
            summary: job.summary,
            workspace: job.workspace,
            harness: job.harness,
            model: job.model,
            thinking: job.thinking,
            window_alive: job.window_alive,
          })),
        });
      },
    }),
  );

  pi.registerTool(
    defineTool({
      name: "scufris_job_inspect",
      label: "Inspect Scufris job",
      description:
        "Inspect one owned job. Include its report, project context, or worker prompt when needed for orchestration.",
      parameters: Type.Object(
        {
          job_id: Type.String({ pattern: "^[a-f0-9]{12}$" }),
          include_report: Type.Optional(Type.Boolean({ default: false })),
          include_context: Type.Optional(Type.Boolean({ default: false })),
          include_prompt: Type.Optional(Type.Boolean({ default: false })),
        },
        { additionalProperties: false },
      ),
      async execute(_id, params, signal) {
        if (!JOB_ID.test(params.job_id) || !jobs.has(params.job_id))
          throw new Error("job is not owned by this Scufris session");
        return toolResult(
          await runHelper(
            "inspect",
            {
              job_id: params.job_id,
              include_report: params.include_report ?? false,
              include_context: params.include_context ?? false,
              include_prompt: params.include_prompt ?? false,
            },
            signal,
          ),
        );
      },
    }),
  );

  pi.registerTool(
    defineTool({
      name: "scufris_job_send",
      label: "Steer Scufris job",
      description: "Send one literal line to an owned waiting worker.",
      executionMode: "sequential",
      parameters: Type.Object(
        {
          job_id: Type.String({ pattern: "^[a-f0-9]{12}$" }),
          message: Type.String({
            minLength: 1,
            pattern: "^[^\\r\\n]+$",
          }),
        },
        { additionalProperties: false },
      ),
      async execute(_id, params, signal) {
        const job = jobs.get(params.job_id);
        if (!job) throw new Error("job is not owned by this Scufris session");
        const result = await runHelper("send", params, signal, 20_000);
        job.state = "working";
        job.summary = "foreground guidance submitted";
        return toolResult(result);
      },
    }),
  );

  pi.registerTool(
    defineTool({
      name: "scufris_job_stop",
      label: "Stop Scufris job",
      description:
        "Stop one owned worker and optionally remove its Sprout workspace.",
      executionMode: "sequential",
      parameters: Type.Object(
        {
          job_id: Type.String({ pattern: "^[a-f0-9]{12}$" }),
          remove_workspace: Type.Optional(Type.Boolean({ default: false })),
        },
        { additionalProperties: false },
      ),
      async execute(_id, params, signal) {
        const job = jobs.get(params.job_id);
        if (!job) throw new Error("job is not owned by this Scufris session");
        const quickReview = job.quick_review;
        job.quick_review = undefined;
        if (quickReview) await quickReview.close();
        const result = await runHelper(
          "stop",
          {
            job_id: params.job_id,
            remove_workspace: params.remove_workspace ?? false,
          },
          signal,
          30_000,
        );
        job.state = "stopped";
        job.summary = "stopped by foreground Scufris";
        job.window_alive = false;
        return toolResult(result);
      },
    }),
  );

  pi.on("before_agent_start", (event) => ({
    systemPrompt: `${event.systemPrompt}\n\n${activeJobPrompt(jobs.values())}`,
  }));

  pi.on("session_start", async (_event, ctx) => {
    extensionContext = ctx;
    shuttingDown = false;
    try {
      const result = await runHelper<{ job_ids: string[] }>("orphans", {
        owner_session: ctx.sessionManager.getSessionId(),
      });
      if (result.job_ids.length > 0 && ctx.hasUI)
        ctx.ui.notify(
          `Unowned Scufris jobs remain: ${result.job_ids.join(", ")}`,
          "warning",
        );
    } catch (error) {
      if (ctx.hasUI)
        ctx.ui.notify(
          error instanceof Error ? error.message : String(error),
          "error",
        );
    }
    timer = setInterval(() => void poll(), 1_000);
  });

  pi.on("session_shutdown", async () => {
    shuttingDown = true;
    if (timer) clearInterval(timer);
    timer = undefined;
    while (polling) await new Promise((resolve) => setTimeout(resolve, 25));
    await Promise.allSettled(
      [...jobs.values()].map(async (job) => {
        const quickReview = job.quick_review;
        job.quick_review = undefined;
        if (quickReview) await quickReview.close();
        if (job.window_alive)
          await runHelper("stop", { job_id: job.job_id }, undefined, 15_000);
      }),
    );
    if (extensionContext?.hasUI)
      extensionContext.ui.setStatus("scufris", undefined);
    contexts.clear();
    jobs.clear();
    extensionContext = undefined;
  });
}
