import { randomBytes } from "node:crypto";
import { watch, type FSWatcher } from "node:fs";
import { StringEnum, Type } from "@earendil-works/pi-ai";
import {
  defineTool,
  isToolCallEventType,
  type ExtensionAPI,
  type ExtensionContext,
} from "@earendil-works/pi-coding-agent";
import {
  ACKNOWLEDGMENT_STATE_EVENT,
  type AcknowledgmentState,
} from "../shared/acknowledgment.ts";
import {
  ATTENTION_NOTICE_EVENT,
  type AttentionNoticeSignal,
} from "../shared/attention-notice.ts";
import { runPrivateHelper, toolPath, toolResult } from "../shared/runtime.ts";
import {
  startQuickReviewAgent,
  type QuickReviewAgent,
  type QuickReviewCompletion,
} from "./quick-review-agent.ts";

const jobsHelperPath = toolPath("jobs/scufris-jobs", import.meta.url);

const CONTEXT_ID = /^[a-f0-9]{24}$/;
const JOB_ID = /^[a-f0-9]{12}$/;
// Every worker event except `working` ends its execution, so `blocked` releases
// the pane exactly like `done` and waits for foreground guidance.
export const TERMINAL_OWNERSHIP_STATES: ReadonlySet<string> = new Set([
  "blocked",
  "done",
  "failed",
  "suspended",
  "stopped",
  "landed",
]);
export const QUICK_REVIEW_TOOL = "scufris_job_quick_review";
export const PLANNOTATOR_REVIEW_TOOL = "scufris_job_plannotator_review";
export const FINAL_RESPONSE_TOOL = "scufris_final_response";
export const ACKNOWLEDGED_ACTION_TOOLS: ReadonlySet<string> = new Set([
  "scufris_job_spawn",
  "scufris_job_send",
  "scufris_job_stop",
  "scufris_job_land",
  QUICK_REVIEW_TOOL,
  PLANNOTATOR_REVIEW_TOOL,
]);
export const literalDelegationPolicy = `Scufris delegates literally. A project context is a menu of agent types, not a workflow to run.

Before planning every new project job, call scufris_project_context with an
opaque project ID from scufris_projects. Its Agents section is the menu: start
only the agents this request names, in the order it names them, and start no
agent because the project declares it. "Implement X" is the work agent alone.
"Implement X and review it" is the work agent and then the review agent. An
agent the request names by a name you have never seen is still delegated to
with that entry's keywords. Infer the Conventions section for tracking,
workspace, base branch, and harness; an explicit instruction in the request
wins over any of them. Use no project context for general work. A project
context creates exactly one job.

A later round of an agent you already own for this work is scufris_job_send,
not a second spawn. The steered job resumes its own session and keeps what it
already accepted; a fresh job keeps nothing and re-derives its findings.

One request is one job. A done event is a finished assignment: inspect the job
and its report, then tell the user what happened. A done event never selects
review, steering, landing, or shutdown by itself. Never queue follow-on work of
your own.`;

export const foregroundActionPolicy = `Call each meaningful workflow action as the only tool in its tool batch. After a successful spawn, steering, stop, landing, or review-opening action, call scufris_final_response next as the only permitted follow-up. Give one short natural acknowledgment of what happened, then end. After spawn or steering, do not sleep, wait, poll, inspect, or do any other work before that final response. If an action tool fails, do not claim success; use scufris_final_response for one concise explanation and the next safe step. Do not use a canned acknowledgment.`;

export type WorkerEventType = "working" | "blocked" | "done" | "failed";
export type WakeMode = "minimal" | "all";

const wakeStateType = "scufris-wake-state-v1";

interface WakeStateEntry {
  version: 1;
  mode: WakeMode;
}

export interface WorkerEvent {
  type: WorkerEventType;
  value: string;
  id?: string;
  generation?: number;
}

export interface WorkerEventTarget {
  job_id: string;
  project: string | null;
  context_id?: string;
}

export function parseWorkerEvent(line: string): WorkerEvent | undefined {
  const separator = line.indexOf(": ");
  if (separator <= 0) return undefined;
  const type = line.slice(0, separator) as WorkerEventType;
  const value = line.slice(separator + 2);
  if (
    !["working", "blocked", "done", "failed"].includes(type) ||
    !value ||
    /[\x00-\x1f\x7f]/.test(value)
  ) {
    return undefined;
  }
  return { type, value };
}

export function workerEventWakes(
  type: WorkerEventType,
  mode: WakeMode = "minimal",
): boolean {
  return type !== "working" || mode === "all";
}

export function resolveWakeCommand(
  args: string,
  current: WakeMode,
): { mode: WakeMode; changed: boolean; notice: string; warning: boolean } {
  const command = args.trim().toLowerCase();
  if (command === "") {
    return {
      mode: current,
      changed: false,
      notice: `Wake mode ${current}.`,
      warning: false,
    };
  }
  if (command === "minimal" || command === "all") {
    return {
      mode: command,
      changed: command !== current,
      notice: `Wake mode ${command}.`,
      warning: false,
    };
  }
  return {
    mode: current,
    changed: false,
    notice: "Use /wake minimal or all.",
    warning: true,
  };
}

export function wakeModeFromEntries(
  entries: Iterable<{
    type: string;
    customType?: string;
    data?: unknown;
  }>,
  fallback: WakeMode = "minimal",
): WakeMode {
  let mode = fallback;
  for (const entry of entries) {
    if (entry.type !== "custom" || entry.customType !== wakeStateType) continue;
    const data = entry.data as Partial<WakeStateEntry> | undefined;
    if (data?.version === 1 && (data.mode === "minimal" || data.mode === "all"))
      mode = data.mode;
  }
  return mode;
}

function restoredWakeMode(context: ExtensionContext): WakeMode {
  return wakeModeFromEntries(context.sessionManager.getBranch());
}

export function deliveredWorkerEventIds(
  entries: Iterable<{
    type?: string;
    customType?: string;
    details?: unknown;
    message?: unknown;
  }>,
): Set<string> {
  const result = new Set<string>();
  for (const entry of entries) {
    const message = entry.message as
      | { customType?: string; details?: { event_id?: unknown } }
      | undefined;
    const topLevelDetails = entry.details as { event_id?: unknown } | undefined;
    if (
      entry.type === "custom_message" &&
      entry.customType === "scufris-job-event" &&
      typeof topLevelDetails?.event_id === "string"
    )
      result.add(topLevelDetails.event_id);
    else if (
      entry.type === "message" &&
      message?.customType === "scufris-job-event" &&
      typeof message.details?.event_id === "string"
    )
      result.add(message.details.event_id);
  }
  return result;
}

/** Maps one worker event to the durable tray notice owned by that job. */
export function workerAttentionSignal(
  job: WorkerEventTarget,
  event: WorkerEvent,
): AttentionNoticeSignal {
  if (event.type === "blocked") {
    return {
      id: job.job_id,
      state: "attention",
      detail: `Job ${job.job_id} is blocked: ${event.value}`,
    };
  }
  if (event.type === "failed") {
    return {
      id: job.job_id,
      state: "error",
      detail: `Job ${job.job_id} failed: ${event.value}`,
    };
  }
  return { id: job.job_id, state: "clear", detail: "" };
}

export function deliverWorkerEvent(
  pi: Pick<ExtensionAPI, "events" | "sendMessage">,
  context: Pick<ExtensionContext, "hasUI" | "ui">,
  job: WorkerEventTarget,
  event: WorkerEvent,
  mode: WakeMode,
): void {
  pi.events.emit(ATTENTION_NOTICE_EVENT, workerAttentionSignal(job, event));
  if (!workerEventWakes(event.type, mode)) {
    if (context.hasUI)
      context.ui.notify(`${job.job_id}: ${event.value}`, "info");
    return;
  }
  const line = `${event.type}: ${event.value}`;
  pi.sendMessage(
    {
      customType: "scufris-job-event",
      content: `Scufris job ${job.job_id} (${job.project ?? "general"}): ${line}. Inspect the pinned job context, prompt, report, and state, then tell the user what happened. Start another agent only if the original request named it. After reacting, call scufris_final_response with one short useful acknowledgment before ending this wake turn.`,
      display: true,
      details: {
        job_id: job.job_id,
        project: job.project,
        context_id: job.context_id,
        event: line,
        event_id: event.id,
        generation: event.generation,
      },
    },
    { deliverAs: "followUp", triggerTurn: true },
  );
}

export function deliverRuntimeFailure(
  pi: Pick<ExtensionAPI, "events" | "sendMessage">,
  context: Pick<ExtensionContext, "hasUI" | "ui">,
  job: WorkerEventTarget,
  error: string,
  mode: WakeMode,
): void {
  if (context.hasUI) context.ui.notify(`Job ${job.job_id}: ${error}`, "error");
  deliverWorkerEvent(pi, context, job, { type: "failed", value: error }, mode);
}

export function toolBatchAllowsAction(
  toolName: string,
  batch: readonly string[],
): boolean {
  return (
    (!ACKNOWLEDGED_ACTION_TOOLS.has(toolName) &&
      toolName !== FINAL_RESPONSE_TOOL) ||
    (batch.length === 1 && batch[0] === toolName)
  );
}

export class ForegroundAcknowledgmentGate {
  private pendingAction: string | undefined;
  private readonly onChange: (action?: string) => void;

  constructor(onChange: (action?: string) => void = () => {}) {
    this.onChange = onChange;
  }

  markSuccessfulAction(toolName: string): void {
    if (!ACKNOWLEDGED_ACTION_TOOLS.has(toolName)) return;
    this.pendingAction = toolName;
    this.onChange(toolName);
  }

  blockReason(toolName: string): string | undefined {
    if (!this.pendingAction || toolName === FINAL_RESPONSE_TOOL) return;
    return `After ${this.pendingAction}, the only permitted follow-up is scufris_final_response. Do not wait, poll, inspect, or do more work.`;
  }

  completeFinalResponse(isError: boolean): void {
    if (!isError) this.reset();
  }

  isPending(): boolean {
    return this.pendingAction !== undefined;
  }

  reset(): void {
    this.pendingAction = undefined;
    this.onChange();
  }
}

function currentToolBatch(context: ExtensionContext): string[] {
  const branch = context.sessionManager.getBranch();
  for (let index = branch.length - 1; index >= 0; index -= 1) {
    const entry = branch[index] as {
      type?: string;
      message?: {
        role?: string;
        content?: Array<{ type?: string; name?: string }>;
      };
    };
    if (entry.type !== "message" || entry.message?.role !== "assistant")
      continue;
    return (entry.message.content ?? [])
      .filter(
        (item): item is { type: "toolCall"; name: string } =>
          item.type === "toolCall" && typeof item.name === "string",
      )
      .map((item) => item.name);
  }
  return [];
}

export function registerForegroundAcknowledgmentLifecycle(
  pi: ExtensionAPI,
): ForegroundAcknowledgmentGate {
  const gate = new ForegroundAcknowledgmentGate((action) => {
    pi.events.emit(ACKNOWLEDGMENT_STATE_EVENT, {
      pending: action !== undefined,
      action,
    } satisfies AcknowledgmentState);
  });

  pi.on("tool_call", (event, context) => {
    const batch = currentToolBatch(context);
    if (!toolBatchAllowsAction(event.toolName, batch)) {
      return {
        block: true,
        reason:
          "Meaningful workflow actions and scufris_final_response must each be the only tool in their tool batch.",
      };
    }
    const reason = gate.blockReason(event.toolName);
    if (reason) return { block: true, reason };
  });

  pi.on("tool_result", (event) => {
    if (event.toolName === FINAL_RESPONSE_TOOL) {
      gate.completeFinalResponse(event.isError);
    }
  });

  pi.on("agent_settled", () => {
    if (gate.isPending()) gate.reset();
  });

  return gate;
}

export function foregroundCommandWaits(command: string): boolean {
  return command.split(/[\n;&|]+/).some((part) => {
    const words = part
      .trim()
      .replace(/^[({]+\s*/, "")
      .split(/\s+/);
    while (/^[A-Za-z_][A-Za-z0-9_]*=/.test(words[0] ?? "")) words.shift();
    while (["command", "builtin", "env"].includes(words[0] ?? "")) {
      words.shift();
      while (/^(?:-[^\s]+|[A-Za-z_][A-Za-z0-9_]*=)/.test(words[0] ?? ""))
        words.shift();
    }
    const executable = (words[0] ?? "").replace(/^['"]|['"]$/g, "");
    return /(?:^|\/)sleep$/.test(executable) || executable === "wait";
  });
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
  root_job: string;
  parent_job: string | null;
  workflow_id: string;
  generation: number;
  workspace: "temporary" | "project" | "sprout" | "review";
  feature: string | null;
  harness: "pi" | "claude";
  model: string;
  thinking: string;
  tmux_session: string | null;
  status_file: string;
  message: string;
  review_isolation: {
    enforcement: "model-tool-allowlist";
    filesystem: "not-os-sandboxed";
    tools: string[];
    trusted_boundary: string[];
  } | null;
}

interface OwnedJob extends SpawnResult {
  trusted_capability: string;
  context_id?: string;
  context_fingerprint?: string;
  summary: string;
  window_alive: boolean;
  quick_review?: QuickReviewAgent;
  status_watcher?: FSWatcher;
}

interface QuickReviewTarget {
  job_id: string;
  cwd: string;
  default_branch: string;
  base_revision: string;
  revision: string;
  state_dir: string;
}

interface EventResult {
  jobs: Array<{
    job_id: string;
    generation: number;
    more: boolean;
    window_alive: boolean;
    events: Array<{
      id: string;
      start: number;
      end: number;
      generation: number;
      type: WorkerEventType | "invalid";
      value: string;
      line: string;
    }>;
  }>;
}

interface CleanupResult {
  removed_jobs: string[];
}

export function applySteerResult(
  job: {
    state: string;
    summary: string;
    generation: number;
    status_file: string;
    window_alive: boolean;
  },
  result: { generation: number; status_file: string; restarted: boolean },
  watchJob: () => void,
): void {
  job.state = "working";
  job.summary = "foreground guidance submitted";
  job.generation = result.generation;
  job.status_file = result.status_file;
  job.window_alive = true;
  if (result.restarted) watchJob();
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
  const owned = [...jobs];
  const index = owned.length
    ? owned
        .slice(-32)
        .map(
          (job) =>
            `- ${job.job_id}: ${job.project ?? "general"}, ${job.state}, ${job.summary.slice(0, 160)}${job.context_id ? `, context ${job.context_id}` : ""}`,
        )
        .join("\n")
    : "- none";
  return `${literalDelegationPolicy}

${foregroundActionPolicy}
Filesystem notifications will deliver later worker events.

Owned Scufris logical jobs:
${index}`;
}

function quickReviewFeedback(event: QuickReviewCompletion): string {
  return `Quick Review requested changes: ${JSON.stringify({ explanation: event.overallComment, comments: event.comments })}`;
}

function quickReviewSummary(event: QuickReviewCompletion): string {
  const encoded = JSON.stringify({
    outcome: event.outcome,
    revision: event.revision,
    base_revision: event.baseRevision,
    overall_comment: event.overallComment,
    comments: event.comments,
    artifact: event.artifact,
    state: event.state,
  });
  const maximum = 24 * 1024;
  if (Buffer.byteLength(encoded, "utf8") <= maximum) return encoded;
  return `${Buffer.from(encoded).subarray(0, maximum).toString("utf8")}...[Quick Review result truncated; inspect the artifact path]`;
}

export default function workflowOrchestration(pi: ExtensionAPI): void {
  if (process.env.SCUFRIS_ROLE !== "orchestrator") return;

  const contexts = new Map<string, ResolvedContext>();
  const jobs = new Map<string, OwnedJob>();
  const deliveredEventIds = new Set<string>();
  let readingEvents = false;
  let readAgain = false;
  let eventReadController: AbortController | undefined;
  let shuttingDown = false;
  let extensionContext: ExtensionContext | undefined;
  let eventError: string | undefined;
  let wakeMode: WakeMode = "minimal";
  const acknowledgmentGate = registerForegroundAcknowledgmentLifecycle(pi);

  const persistWakeMode = () => {
    pi.appendEntry(wakeStateType, {
      version: 1,
      mode: wakeMode,
    } satisfies WakeStateEntry);
  };

  pi.registerCommand("wake", {
    description: "Control delegated-worker progress wakes: minimal or all.",
    getArgumentCompletions: (prefix) => {
      const values: WakeMode[] = ["minimal", "all"];
      const matches = values.filter((value) =>
        value.startsWith(prefix.trim().toLowerCase()),
      );
      return matches.length
        ? matches.map((value) => ({ value, label: value }))
        : null;
    },
    handler: async (args, context) => {
      const result = resolveWakeCommand(args, wakeMode);
      if (result.changed) {
        wakeMode = result.mode;
        persistWakeMode();
      }
      context.ui.notify(result.notice, result.warning ? "warning" : "info");
    },
  });

  const readEvents = async () => {
    if (shuttingDown || !extensionContext) return;
    if (readingEvents) {
      readAgain = true;
      return;
    }
    readingEvents = true;
    const controller = new AbortController();
    eventReadController = controller;
    try {
      do {
        readAgain = false;
        const owned = [...jobs.values()];
        if (owned.length === 0) {
          if (extensionContext.hasUI)
            extensionContext.ui.setStatus("scufris", undefined);
          return;
        }
        const result = await runHelper<EventResult>(
          "events",
          {
            jobs: owned.map((job) => ({ job_id: job.job_id })),
          },
          controller.signal,
        );
        if (shuttingDown) return;
        for (const update of result.jobs) {
          const job = jobs.get(update.job_id);
          if (!job) continue;
          job.window_alive = update.window_alive;
          job.generation = update.generation;
          if (update.more) readAgain = true;
          for (const updateEvent of update.events) {
            if (deliveredEventIds.has(updateEvent.id)) {
              await runHelper("ack-event", {
                job_id: job.job_id,
                event_id: updateEvent.id,
              });
              continue;
            }
            if (updateEvent.type === "invalid") {
              await runHelper("ack-event", {
                job_id: job.job_id,
                event_id: updateEvent.id,
              });
              await runHelper("failure", {
                job_id: job.job_id,
                capability: job.trusted_capability,
                summary: updateEvent.value,
                report: `Trusted orchestration rejected a worker status record: ${updateEvent.line}`,
              });
              readAgain = true;
              continue;
            }
            const event: WorkerEvent = {
              type: updateEvent.type,
              value: updateEvent.value,
              id: updateEvent.id,
              generation: updateEvent.generation,
            };
            job.state = event.type;
            job.summary = event.value;
            deliverWorkerEvent(pi, extensionContext, job, event, wakeMode);
            deliveredEventIds.add(updateEvent.id);
            await runHelper("ack-event", {
              job_id: job.job_id,
              event_id: updateEvent.id,
            });
          }
          if (TERMINAL_OWNERSHIP_STATES.has(job.state)) {
            job.status_watcher?.close();
            job.status_watcher = undefined;
          }
        }
        const running = [...jobs.values()].filter(
          (job) => !TERMINAL_OWNERSHIP_STATES.has(job.state),
        ).length;
        if (extensionContext.hasUI)
          extensionContext.ui.setStatus(
            "scufris",
            running > 0
              ? `${running} delegated job${running === 1 ? "" : "s"}`
              : undefined,
          );
        eventError = undefined;
      } while (readAgain && !shuttingDown);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      if (!shuttingDown && message !== eventError && extensionContext.hasUI)
        extensionContext.ui.notify(message, "error");
      eventError = message;
    } finally {
      if (eventReadController === controller) eventReadController = undefined;
      readingEvents = false;
    }
  };

  const watchJob = (job: OwnedJob) => {
    if (job.status_watcher) return;
    const watcher = watch(job.status_file, { persistent: false }, () => {
      void readEvents();
    });
    watcher.on("error", (error) => {
      if (!shuttingDown && extensionContext?.hasUI)
        extensionContext.ui.notify(
          `Job ${job.job_id} status watch failed: ${error.message}`,
          "error",
        );
    });
    job.status_watcher = watcher;
    void readEvents();
  };

  const closeWorkflowSurfaces = async (rootJob: string) => {
    const related = [...jobs.values()].filter(
      (candidate) => candidate.root_job === rootJob,
    );
    await Promise.all(
      related.map(async (candidate) => {
        candidate.status_watcher?.close();
        candidate.status_watcher = undefined;
        const review = candidate.quick_review;
        candidate.quick_review = undefined;
        if (review) await review.close();
      }),
    );
  };

  const forgetRemovedJobs = (removed: readonly string[]) => {
    for (const removedJob of removed) jobs.delete(removedJob);
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
      label: "Load project agent menu",
      description:
        "Load one project's advisory .scufris.toml agent menu and conventions before planning one new job. Returns a single-use context ID.",
      promptSnippet: "Load the project agent menu for one new job",
      promptGuidelines: [
        "Call scufris_project_context before planning every new project job, even when another job already uses that project.",
        "The menu lists what the project can do. Start only the agents this request names.",
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
            "Compose this job now for the agent this request names. Pass this context ID exactly once to scufris_job_spawn.",
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
        "Use scufris_job_spawn for work expected to take minutes. Spawn one job for the agent this request names, with that menu entry's keywords.",
        "Spawn an agent's first round only. Steer the job you already own for a later round with scufris_job_send, so it keeps what it already accepted.",
        "Select the workspace the request asks for. Fall back to the project conventions only when the request is silent.",
        foregroundActionPolicy,
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
        const trustedCapability = randomBytes(32).toString("hex");
        const result = await runHelper<SpawnResult>(
          "spawn",
          {
            job_id: generatedJobId,
            instructions: params.instructions,
            owner_session: ctx.sessionManager.getSessionId(),
            trusted_capability: trustedCapability,
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
        const { status_file, ...publicResult } = result;
        const job: OwnedJob = {
          ...publicResult,
          status_file,
          trusted_capability: trustedCapability,
          ...(resolved
            ? {
                context_id: resolved.context_id,
                context_fingerprint: resolved.fingerprint,
              }
            : {}),
          summary: "worker starting",
          window_alive: true,
        };
        jobs.set(job.job_id, job);
        watchJob(job);
        acknowledgmentGate.markSuccessfulAction("scufris_job_spawn");
        return toolResult({
          ...publicResult,
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
                content: `Plannotator review completed for Scufris job ${job.job_id}. Inspect this structured result and report it to the user: ${encoded}. Act on it further only where the request asked for that. After reacting, call scufris_final_response with one short useful acknowledgment before ending this wake turn.`,
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
        acknowledgmentGate.markSuccessfulAction(PLANNOTATOR_REVIEW_TOOL);
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
        "Start a separate read-only Pi RPC agent with the standalone Quick Review extension for one owned Sprout job. The agent stays available for page questions and never lands automatically.",
      executionMode: "sequential",
      parameters: Type.Object(
        {
          job_id: Type.String({ pattern: "^[a-f0-9]{12}$" }),
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
      async execute(_id, params, signal) {
        const job = jobs.get(params.job_id);
        if (!job) throw new Error("job is not owned by this Scufris session");
        if (job.quick_review)
          throw new Error("Quick Review is already open for this job");
        const target = await runHelper<QuickReviewTarget>(
          "quick-review-target",
          { job_id: job.job_id },
          signal,
        );
        const agent = await startQuickReviewAgent(
          {
            repository: target.cwd,
            base_revision: target.base_revision,
            revision: target.revision,
            model: params.model ?? "openai-codex/gpt-5.6-sol",
            thinking: params.thinking ?? "medium",
            state_dir: target.state_dir,
          },
          { signal },
        );
        job.quick_review = agent;
        void agent.completion
          .then(async (event) => {
            if (job.quick_review !== agent) return;
            job.quick_review = undefined;
            await agent.close();
            if (shuttingDown) return;
            let milestone = "quick-review-approved";
            let nextState: "done" | "working" = "done";
            if (event.outcome === "changes-requested") {
              const restarted = await runHelper<{
                generation: number;
                status_file: string;
              }>("send", {
                job_id: job.job_id,
                message: quickReviewFeedback(event),
              });
              applySteerResult(job, { ...restarted, restarted: true }, () =>
                watchJob(job),
              );
              milestone = "quick-review-feedback-submitted";
              nextState = "working";
            } else {
              job.state = "done";
              job.summary = milestone;
            }
            pi.sendMessage(
              {
                customType: "scufris-job-event",
                content: `Scufris job ${job.job_id} (${job.project ?? "general"}): ${nextState}: ${milestone}. Inspect this standalone Quick Review result and report it to the user: ${quickReviewSummary(event)}. Act on it further only where the request asked for that. After reacting, call scufris_final_response with one short useful acknowledgment before ending this wake turn.`,
                display: true,
                details: {
                  job_id: job.job_id,
                  context_id: job.context_id,
                  quick_review: event,
                },
              },
              { deliverAs: "followUp", triggerTurn: true },
            );
          })
          .catch(async (error) => {
            if (job.quick_review !== agent) return;
            job.quick_review = undefined;
            await agent.close();
            if (shuttingDown) return;
            deliverRuntimeFailure(
              pi,
              extensionContext!,
              job,
              error instanceof Error ? error.message : String(error),
              wakeMode,
            );
          });
        acknowledgmentGate.markSuccessfulAction(QUICK_REVIEW_TOOL);
        return toolResult({
          job_id: job.job_id,
          state: "quick-review-opened",
          revision: target.revision,
          agent: "pi-rpc",
          extension: "npm:@alexjercan/quick-review@0.1.1",
        });
      },
    }),
  );

  pi.registerTool(
    defineTool({
      name: "scufris_job_land",
      label: "Land Scufris job",
      description:
        "Explicitly land one owned Sprout job after the user has asked for it. This tool never runs automatically.",
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
        await closeWorkflowSurfaces(job.root_job);
        const result = await runHelper<CleanupResult>(
          "land",
          {
            job_id: params.job_id,
            subject: params.subject,
            remove_workspace: params.remove_workspace ?? true,
          },
          signal,
          120_000,
        );
        forgetRemovedJobs(result.removed_jobs);
        acknowledgmentGate.markSuccessfulAction("scufris_job_land");
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
            root_job: job.root_job,
            parent_job: job.parent_job,
            generation: job.generation,
            state: job.state,
            summary: job.summary,
            workspace: job.workspace,
            harness: job.harness,
            model: job.model,
            thinking: job.thinking,
            review_isolation: job.review_isolation,
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
          include_conversation: Type.Optional(Type.Boolean({ default: false })),
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
              include_conversation: params.include_conversation ?? false,
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
      promptGuidelines: [foregroundActionPolicy],
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
        const result = await runHelper<{
          generation: number;
          status_file: string;
          restarted: boolean;
        }>("send", params, signal, 20_000);
        applySteerResult(job, result, () => watchJob(job));
        acknowledgmentGate.markSuccessfulAction("scufris_job_send");
        return toolResult(result);
      },
    }),
  );

  pi.registerTool(
    defineTool({
      name: "scufris_job_stop",
      label: "Stop Scufris job",
      description:
        "Stop and recursively remove one owned workflow and all descendants.",
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
        await closeWorkflowSurfaces(job.root_job);
        const result = await runHelper<CleanupResult>(
          "stop",
          {
            job_id: params.job_id,
            remove_workspace: params.remove_workspace ?? false,
          },
          signal,
          30_000,
        );
        forgetRemovedJobs(result.removed_jobs);
        acknowledgmentGate.markSuccessfulAction("scufris_job_stop");
        return toolResult(result);
      },
    }),
  );

  pi.on("tool_call", (event) => {
    if (
      isToolCallEventType("bash", event) &&
      foregroundCommandWaits(event.input.command)
    ) {
      return {
        block: true,
        reason:
          "Foreground Scufris never sleeps or waits. Use scufris_final_response now; filesystem notifications will deliver worker events.",
      };
    }
  });

  pi.on("before_agent_start", (event) => ({
    systemPrompt: `${event.systemPrompt}\n\n${activeJobPrompt(jobs.values())}`,
  }));

  pi.on("session_start", async (_event, ctx) => {
    extensionContext = ctx;
    shuttingDown = false;
    wakeMode = restoredWakeMode(ctx);
    acknowledgmentGate.reset();
    deliveredEventIds.clear();
    for (const eventId of deliveredWorkerEventIds(
      ctx.sessionManager.getEntries(),
    ))
      deliveredEventIds.add(eventId);
    try {
      const result = await runHelper<{
        jobs: Array<
          SpawnResult & {
            trusted_capability: string;
            summary: string;
            window_alive: boolean;
            status_file: string;
          }
        >;
      }>("recover", {
        owner_session: ctx.sessionManager.getSessionId(),
      });
      for (const recovered of result.jobs) {
        const job: OwnedJob = { ...recovered };
        jobs.set(job.job_id, job);
        if (job.window_alive) watchJob(job);
      }
      await readEvents();
    } catch (error) {
      if (ctx.hasUI)
        ctx.ui.notify(
          error instanceof Error ? error.message : String(error),
          "error",
        );
    }
  });

  pi.on("session_tree", (_event, ctx) => {
    wakeMode = restoredWakeMode(ctx);
  });

  pi.on("session_shutdown", async (_event, context) => {
    shuttingDown = true;
    for (const job of jobs.values()) {
      job.status_watcher?.close();
      job.status_watcher = undefined;
      const review = job.quick_review;
      job.quick_review = undefined;
      if (review) await review.close();
    }
    eventReadController?.abort();
    eventReadController = undefined;
    try {
      const suspended = await runHelper<{ errors: string[] }>(
        "suspend-owner",
        { owner_session: context.sessionManager.getSessionId() },
        undefined,
        30_000,
      );
      if (suspended.errors.length > 0 && extensionContext?.hasUI)
        extensionContext.ui.notify(
          `Scufris suspension incomplete: ${suspended.errors.join("; ")}`,
          "error",
        );
    } catch (error) {
      if (extensionContext?.hasUI)
        extensionContext.ui.notify(
          error instanceof Error ? error.message : String(error),
          "error",
        );
    }
    if (extensionContext?.hasUI)
      extensionContext.ui.setStatus("scufris", undefined);
    contexts.clear();
    jobs.clear();
    deliveredEventIds.clear();
    acknowledgmentGate.reset();
    extensionContext = undefined;
  });
}
