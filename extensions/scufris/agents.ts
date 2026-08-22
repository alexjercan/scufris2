import { randomBytes } from "node:crypto";
import { fileURLToPath } from "node:url";
import { StringEnum, Type } from "@earendil-works/pi-ai";
import {
  defineTool,
  type ExtensionAPI,
  type ExtensionContext,
} from "@earendil-works/pi-coding-agent";
import {
  createAgentDiagnosticsTool,
  type DiagnosticsInvocation,
} from "./diagnostics.ts";
import { runPrivateHelper, toolResult } from "./shared/runtime.ts";

const jobHelperPath = fileURLToPath(
  new URL("../../scripts/scufris-job", import.meta.url),
);

export const PREFLIGHT_REVIEW_TIMEOUT_MS = 1_800_000;
// Preserve time for exact-child shutdown and the fail-closed helper response.
export const PREFLIGHT_HELPER_SHUTDOWN_MARGIN_MS = 10_000;
export const PREFLIGHT_HELPER_TIMEOUT_MS =
  PREFLIGHT_REVIEW_TIMEOUT_MS + PREFLIGHT_HELPER_SHUTDOWN_MARGIN_MS;
export const PREFLIGHT_REVIEW_READY_LINE = "scufris-preflight-reviewer-started";

interface ReviewSnapshot {
  job_id: string;
  worktree: string;
  landing_branch: string;
  landing_sha: string;
  feature_sha: string;
  subject: string;
}

interface ApprovedReview extends ReviewSnapshot {
  request_id: string;
}

export type LandableReviewProfile =
  | "code"
  | "consumer"
  | "operations"
  | "interface";
export type ReviewPolicy =
  | { profile: "none" }
  | { profile: LandableReviewProfile; brief: string };

export interface PreflightFinding {
  severity: "BLOCKER" | "MAJOR" | "MINOR";
  path: string;
  line: number;
  reason: string;
  change: string;
}

interface PreflightResult {
  verdict: "approve" | "request_changes";
  findings: PreflightFinding[];
  review_id: string;
  landing_sha: string;
  feature_sha: string;
}

export type PreflightOutcome =
  | { kind: "approved" }
  | { kind: "feedback"; findings: PreflightFinding[] }
  | { kind: "blocked"; reason: string };

export function classifyPreflightResult(value: unknown): PreflightOutcome {
  if (!value || typeof value !== "object") {
    return { kind: "blocked", reason: "preflight returned malformed output" };
  }
  const result = value as Partial<PreflightResult>;
  if (result.verdict !== "approve" && result.verdict !== "request_changes") {
    return { kind: "blocked", reason: "preflight verdict is invalid" };
  }
  if (!Array.isArray(result.findings)) {
    return { kind: "blocked", reason: "preflight findings are invalid" };
  }
  const findings: PreflightFinding[] = [];
  for (const finding of result.findings) {
    if (!finding || typeof finding !== "object") {
      return { kind: "blocked", reason: "preflight finding is malformed" };
    }
    const item = finding as Partial<PreflightFinding>;
    if (
      !["BLOCKER", "MAJOR", "MINOR"].includes(String(item.severity)) ||
      typeof item.path !== "string" ||
      !item.path ||
      item.path.startsWith("/") ||
      item.path
        .split("/")
        .some((part) => !part || part === "." || part === "..") ||
      !Number.isSafeInteger(item.line) ||
      Number(item.line) <= 0 ||
      typeof item.reason !== "string" ||
      !item.reason ||
      typeof item.change !== "string" ||
      !item.change
    ) {
      return { kind: "blocked", reason: "preflight finding is invalid" };
    }
    findings.push(item as PreflightFinding);
  }
  if (result.verdict === "approve") {
    return findings.length === 0
      ? { kind: "approved" }
      : { kind: "blocked", reason: "preflight approval contains findings" };
  }
  return findings.length > 0
    ? { kind: "feedback", findings }
    : {
        kind: "blocked",
        reason: "preflight change request has no findings",
      };
}

export function preflightFindingsMessage(findings: PreflightFinding[]): string {
  return `Independent preflight requested changes: ${JSON.stringify({ findings })}`;
}

export function nextPreflightFeedbackCycle(
  completedCycles: number,
): number | undefined {
  return Number.isInteger(completedCycles) &&
    completedCycles >= 0 &&
    completedCycles < 2
    ? completedCycles + 1
    : undefined;
}

export interface MutablePreflightState {
  preflightReviewId?: string;
  preflightFeedbackCycles: number;
  preflightApproval?: unknown;
}

export function invalidatePreflight(state: MutablePreflightState): void {
  state.preflightReviewId = undefined;
  state.preflightFeedbackCycles = 0;
  state.preflightApproval = undefined;
}

export function sameReviewRevisions(
  left: ReviewSnapshot,
  right: ReviewSnapshot,
): boolean {
  return (
    left.landing_sha === right.landing_sha &&
    left.feature_sha === right.feature_sha &&
    left.subject === right.subject
  );
}

export function reviewEventRejection(
  policy: ReviewPolicy,
  eventState: string,
  phase: ReviewPhase,
): string | undefined {
  if (eventState === "review-ready" && policy.profile === "none") {
    return "non-landable jobs cannot enter review-ready";
  }
  if (
    eventState === "done" &&
    policy.profile !== "none" &&
    phase !== "awaiting-done"
  ) {
    return "landable jobs require review approval before done";
  }
  return undefined;
}

export type ReviewPhase =
  | "idle"
  | "preflight"
  | "reviewing"
  | "feedback"
  | "awaiting-done"
  | "landing";

export interface ReviewRetryState {
  state: string;
  reviewPhase: ReviewPhase;
  reviewRetryable: boolean;
  hasApproval: boolean;
}

interface MutableReviewRetryState {
  state: string;
  summary: string;
  reviewPhase: ReviewPhase;
  reviewRetryable: boolean;
  reviewRequestId?: string;
  approval?: unknown;
}

export function reviewRetryRejection(
  state: ReviewRetryState,
): string | undefined {
  if (state.state !== "blocked") return "job is not lifecycle blocked";
  if (state.reviewPhase !== "idle") {
    return `review retry is invalid during ${state.reviewPhase}`;
  }
  if (state.hasApproval) return "review retry cannot reuse an approval";
  if (!state.reviewRetryable) {
    return "job is not blocked by a retryable review precondition";
  }
  return undefined;
}

export function consumeReviewRetry(
  state: MutableReviewRetryState,
): string | undefined {
  const rejection = reviewRetryRejection({
    state: state.state,
    reviewPhase: state.reviewPhase,
    reviewRetryable: state.reviewRetryable,
    hasApproval: state.approval !== undefined,
  });
  if (rejection) return rejection;
  state.reviewRetryable = false;
  state.reviewRequestId = undefined;
  state.approval = undefined;
  state.state = "review-ready";
  state.summary = "retrying fresh review preconditions";
  return undefined;
}

export type CleanupPolicy = "remove" | "retain";

export interface LandingOperations {
  land(): Promise<void>;
  stop(): Promise<void>;
  remove(): Promise<void>;
}

export type LandingOutcome =
  | { state: "landed"; summary: string }
  | { state: "landed-with-retained-resources"; summary: string };

export async function completeApprovedLanding(
  cleanup: CleanupPolicy,
  operations: LandingOperations,
): Promise<LandingOutcome> {
  await operations.land();
  try {
    await operations.stop();
    if (cleanup === "remove") await operations.remove();
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    return {
      state: "landed-with-retained-resources",
      summary: `landing succeeded but cleanup failed: ${message}. Inspect the job and remove retained resources manually.`,
    };
  }
  return cleanup === "remove"
    ? {
        state: "landed",
        summary: "approved revision landed and resources removed",
      }
    : {
        state: "landed",
        summary: "approved revision landed; branch and worktree retained",
      };
}

interface OwnedJob {
  job_id: string;
  harness: "pi" | "claude";
  project: string;
  project_root: string;
  worktree: string;
  feature: string;
  cleanup: CleanupPolicy;
  review: ReviewPolicy;
  state: string;
  summary: string;
  offset: number;
  tail: string;
  inode: number | null;
  window_alive: boolean;
  protocolErrors: Set<string>;
  exitReported: boolean;
  reviewPhase: ReviewPhase;
  reviewRetryable: boolean;
  lastReviewedFeature?: string;
  reviewRequestId?: string;
  preflightReviewId?: string;
  preflightFeedbackCycles: number;
  preflightApproval?: ReviewSnapshot;
  reviewAbort?: AbortController;
  approval?: ApprovedReview;
}

interface SpawnResult {
  job_id: string;
  state: string;
  project_root: string;
  worktree: string;
  harness: "pi" | "claude";
  project: string;
  feature: string;
  cleanup: CleanupPolicy;
  review: ReviewPolicy;
  tmux_session: string;
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
  project: string;
  feature: string;
  cleanup: CleanupPolicy;
  review: ReviewPolicy;
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
  readyLine?: string,
): Promise<T> {
  const envelope = await runPrivateHelper<T>(
    jobHelperPath,
    command,
    request,
    signal,
    timeoutMs,
    readyLine,
  );
  if (!envelope.ok || envelope.result === undefined) {
    throw new Error(envelope.error ?? "Scufris helper failed");
  }
  return envelope.result;
}

function generatedJobId(): string {
  return randomBytes(6).toString("hex");
}

export const delegatedFeaturePattern = "^[a-z0-9]+(?:-[a-z0-9]+)*(?![\\s\\S])";

export function isDelegatedFeature(value: string): boolean {
  return value.length <= 48 && new RegExp(delegatedFeaturePattern).test(value);
}

export function jobEventTriggersTurn(state: string): boolean {
  return state !== "working";
}

export const APPROVAL_INSTRUCTION =
  "Review approved. Do not make repository changes. Finalize report.md, append exactly `done: review approved with no changes requested`, then wait.";
export const APPROVAL_DONE_SUMMARY =
  "review approved with no changes requested";

interface PlannotatorResponse {
  status: "handled" | "unavailable" | "error";
  error?: string;
  result?: {
    approved?: unknown;
    feedback?: unknown;
    annotations?: unknown;
  };
}

export type ReviewOutcome =
  | { kind: "approved" }
  | { kind: "feedback"; message: string }
  | { kind: "blocked"; reason: string };

export function classifyReviewResponse(response: unknown): ReviewOutcome {
  if (!response || typeof response !== "object") {
    return {
      kind: "blocked",
      reason: "Plannotator returned a malformed response",
    };
  }
  const value = response as PlannotatorResponse;
  if (
    value.status !== "handled" ||
    !value.result ||
    typeof value.result !== "object"
  ) {
    const reason =
      typeof value.error === "string" && value.error
        ? value.error
        : `Plannotator review was ${String(value.status)}`;
    return { kind: "blocked", reason };
  }
  const feedback =
    typeof value.result.feedback === "string"
      ? value.result.feedback.trim()
      : "";
  const annotations = Array.isArray(value.result.annotations)
    ? value.result.annotations
    : [];
  if (annotations.length === 0 && value.result.approved === true) {
    return { kind: "approved" };
  }
  if (feedback || annotations.length > 0) {
    let details: string;
    try {
      details = JSON.stringify({ feedback, annotations });
    } catch {
      return {
        kind: "blocked",
        reason: "Plannotator feedback is not valid structured data",
      };
    }
    const message = `Plannotator requested changes. Address this exact feedback: ${details}`;
    if (Buffer.byteLength(message, "utf8") > 16 * 1024) {
      return {
        kind: "blocked",
        reason: "Plannotator feedback exceeds the steering limit",
      };
    }
    return { kind: "feedback", message };
  }
  return {
    kind: "blocked",
    reason: "Plannotator closed without approval or actionable feedback",
  };
}

export function isApprovalDone(state: string, summary: string): boolean {
  return state === "done" && summary === APPROVAL_DONE_SUMMARY;
}

export function codeReviewPayload(snapshot: ReviewSnapshot) {
  return {
    cwd: snapshot.worktree,
    defaultBranch: snapshot.landing_branch,
    diffType: "since-base" as const,
  };
}

function parseEvent(line: string): { state: string; summary: string } {
  const separator = line.indexOf(": ");
  return {
    state: line.slice(0, separator),
    summary: line.slice(separator + 2),
  };
}

export default function scufris(
  pi: ExtensionAPI,
  options: { diagnosticsInvocation?: DiagnosticsInvocation } = {},
): void {
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

  const blockLifecycle = (
    job: OwnedJob,
    reason: string,
    reviewRetryable = false,
  ) => {
    job.reviewPhase = "idle";
    job.reviewRetryable = reviewRetryable;
    job.reviewRequestId = undefined;
    job.preflightApproval = undefined;
    job.approval = undefined;
    job.state = "blocked";
    job.summary = reason;
    context?.ui.notify(`Job ${job.job_id}: ${reason}`, "error");
    sendJobEvent(job, `blocked: ${reason}`, true);
  };

  const currentSnapshot = (job: OwnedJob) =>
    runHelper<ReviewSnapshot>("review-snapshot", {
      job_id: job.job_id,
      project_root: job.project_root,
    });

  const handleReviewResponse = async (
    job: OwnedJob,
    requestId: string,
    snapshot: ReviewSnapshot,
    response: unknown,
  ) => {
    if (
      shuttingDown ||
      jobs.get(job.job_id) !== job ||
      job.reviewPhase !== "reviewing" ||
      job.reviewRequestId !== requestId
    ) {
      return;
    }
    const outcome = classifyReviewResponse(response);
    if (outcome.kind === "blocked") {
      blockLifecycle(job, outcome.reason);
      return;
    }
    if (outcome.kind === "feedback") {
      job.reviewPhase = "feedback";
      invalidatePreflight(job);
      try {
        await runHelper("send", {
          job_id: job.job_id,
          message: outcome.message,
        });
        job.reviewPhase = "idle";
        job.state = "working";
        job.summary = "review feedback submitted to worker";
        context?.ui.notify(`Review feedback sent to job ${job.job_id}`, "info");
      } catch (error) {
        blockLifecycle(
          job,
          `could not return review feedback: ${error instanceof Error ? error.message : String(error)}`,
        );
      }
      return;
    }
    try {
      const verified = await currentSnapshot(job);
      if (
        !job.preflightApproval ||
        !sameReviewRevisions(snapshot, job.preflightApproval) ||
        !sameReviewRevisions(snapshot, verified)
      ) {
        throw new Error(
          "reviewed revisions changed before approval acknowledgment",
        );
      }
      job.reviewPhase = "awaiting-done";
      job.approval = { ...snapshot, request_id: requestId };
      await runHelper("send", {
        job_id: job.job_id,
        message: APPROVAL_INSTRUCTION,
      });
      job.state = "working";
      job.summary = "waiting for required review approval acknowledgment";
      context?.ui.notify(`Review approved for job ${job.job_id}`, "info");
    } catch (error) {
      job.approval = undefined;
      blockLifecycle(
        job,
        `approval could not be finalized: ${error instanceof Error ? error.message : String(error)}`,
      );
    }
  };

  const beginUserReview = async (
    job: OwnedJob,
    snapshot: ReviewSnapshot,
  ): Promise<boolean> => {
    job.reviewPhase = "reviewing";
    const requestId = `scufris-${job.job_id}-${randomBytes(6).toString("hex")}`;
    job.reviewRequestId = requestId;
    try {
      context?.ui.notify(`Opening review for job ${job.job_id}`, "info");
      pi.events.emit("plannotator:request", {
        requestId,
        action: "code-review",
        payload: codeReviewPayload(snapshot),
        respond: (response: unknown) => {
          void handleReviewResponse(job, requestId, snapshot, response);
        },
      });
      return true;
    } catch (error) {
      blockLifecycle(
        job,
        `review request failed: ${error instanceof Error ? error.message : String(error)}`,
      );
      return false;
    }
  };

  const beginReview = async (job: OwnedJob): Promise<boolean> => {
    if (job.reviewPhase !== "idle") {
      blockLifecycle(job, `review-ready is invalid during ${job.reviewPhase}`);
      return false;
    }
    if (job.review.profile === "none") {
      blockLifecycle(job, "non-landable jobs cannot enter review-ready");
      return false;
    }
    job.reviewPhase = "preflight";
    job.reviewRetryable = false;
    job.reviewRequestId = undefined;
    job.preflightApproval = undefined;
    job.approval = undefined;
    let snapshot: ReviewSnapshot;
    try {
      snapshot = await currentSnapshot(job);
    } catch (error) {
      blockLifecycle(
        job,
        `review precondition failed: ${error instanceof Error ? error.message : String(error)}`,
        true,
      );
      return false;
    }
    if (snapshot.feature_sha === job.lastReviewedFeature) {
      blockLifecycle(job, "review-ready requires a new committed revision");
      return false;
    }
    job.lastReviewedFeature = snapshot.feature_sha;
    const reviewId = job.preflightReviewId ?? randomBytes(6).toString("hex");
    const continued = job.preflightReviewId !== undefined;
    const controller = new AbortController();
    job.reviewAbort = controller;
    try {
      context?.ui.notify(`Running preflight for job ${job.job_id}`, "info");
      const result = await runHelper<PreflightResult>(
        "preflight-review",
        {
          job_id: job.job_id,
          project_root: job.project_root,
          review_id: reviewId,
          landing_sha: snapshot.landing_sha,
          feature_sha: snapshot.feature_sha,
          ...(continued ? { continue_session: true } : {}),
        },
        controller.signal,
        PREFLIGHT_HELPER_TIMEOUT_MS,
        PREFLIGHT_REVIEW_READY_LINE,
      );
      if (shuttingDown || jobs.get(job.job_id) !== job) return false;
      job.reviewAbort = undefined;
      if (
        result.review_id !== reviewId ||
        result.landing_sha !== snapshot.landing_sha ||
        result.feature_sha !== snapshot.feature_sha
      ) {
        blockLifecycle(
          job,
          "preflight result does not match the exact snapshot",
        );
        return false;
      }
      const verified = await currentSnapshot(job);
      if (!sameReviewRevisions(snapshot, verified)) {
        blockLifecycle(job, "review revisions changed after preflight");
        return false;
      }
      const outcome = classifyPreflightResult(result);
      if (outcome.kind === "blocked") {
        blockLifecycle(job, outcome.reason);
        return false;
      }
      job.preflightReviewId = reviewId;
      if (outcome.kind === "feedback") {
        const nextCycle = nextPreflightFeedbackCycle(
          job.preflightFeedbackCycles,
        );
        if (nextCycle === undefined) {
          blockLifecycle(
            job,
            "preflight requested a third correction cycle; Pair mediation is required",
          );
          return false;
        }
        job.preflightFeedbackCycles = nextCycle;
        job.reviewPhase = "feedback";
        const message = preflightFindingsMessage(outcome.findings);
        await runHelper("send", { job_id: job.job_id, message });
        job.reviewPhase = "idle";
        job.state = "working";
        job.summary = "preflight findings submitted to worker";
        context?.ui.notify(
          `Preflight findings sent to job ${job.job_id}`,
          "info",
        );
        return true;
      }
      job.preflightApproval = snapshot;
      return await beginUserReview(job, snapshot);
    } catch (error) {
      job.reviewAbort = undefined;
      if (!shuttingDown && !controller.signal.aborted) {
        blockLifecycle(
          job,
          `preflight failed closed: ${error instanceof Error ? error.message : String(error)}`,
        );
      }
      return false;
    }
  };

  const landApproved = async (job: OwnedJob) => {
    const approval = job.approval;
    if (!approval) {
      blockLifecycle(
        job,
        "review approval acknowledgment has no approved revision",
      );
      return;
    }
    job.reviewPhase = "landing";
    try {
      const outcome = await completeApprovedLanding(job.cleanup, {
        land: async () => {
          await runHelper(
            "land",
            {
              job_id: job.job_id,
              project_root: job.project_root,
              landing_sha: approval.landing_sha,
              feature_sha: approval.feature_sha,
              subject: approval.subject,
            },
            undefined,
            120_000,
          );
        },
        stop: async () => {
          await runHelper("stop", { job_id: job.job_id }, undefined, 15_000);
          job.window_alive = false;
        },
        remove: async () => {
          await runHelper(
            "remove",
            { job_id: job.job_id, project_root: job.project_root },
            undefined,
            120_000,
          );
        },
      });
      job.state = outcome.state;
      job.summary = outcome.summary;
      if (outcome.state === "landed-with-retained-resources") {
        job.summary =
          job.cleanup === "remove"
            ? `${outcome.summary} Run sprout rm ${job.feature} in project ${job.project}.`
            : `${outcome.summary} Stop owned job ${job.job_id} manually; keep its feature resources.`;
      }
      if (outcome.state === "landed") {
        context?.ui.notify(`Job ${job.job_id}: ${outcome.summary}`, "info");
      } else {
        context?.ui.notify(`Job ${job.job_id}: ${outcome.summary}`, "error");
        sendJobEvent(job, `${outcome.state}: ${outcome.summary}`, true);
      }
    } catch (error) {
      blockLifecycle(
        job,
        `guarded landing failed: ${error instanceof Error ? error.message : String(error)}`,
      );
    }
  };

  const poll = async () => {
    if (pollRunning || shuttingDown || !context) return;
    const active = [...jobs.values()].filter(
      (job) =>
        job.state !== "stopped" &&
        job.state !== "done" &&
        job.state !== "landed" &&
        job.state !== "landed-with-retained-resources" &&
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
          const rejection = reviewEventRejection(
            job.review,
            event.state,
            job.reviewPhase,
          );
          if (rejection) {
            blockLifecycle(job, rejection);
            continue;
          }
          if (
            ["needs-decision", "blocked", "failed"].includes(event.state) &&
            job.reviewPhase !== "idle"
          ) {
            job.reviewAbort?.abort();
            job.reviewAbort = undefined;
            job.reviewPhase = "idle";
            job.reviewRequestId = undefined;
            job.preflightApproval = undefined;
            job.approval = undefined;
          }
          if (event.state === "done" && job.reviewPhase === "awaiting-done") {
            if (!isApprovalDone(event.state, event.summary)) {
              blockLifecycle(
                job,
                "approved review requires the exact worker done acknowledgment",
              );
              continue;
            }
            job.state = event.state;
            job.summary = event.summary;
            sendJobEvent(job, line, true);
            void landApproved(job);
            continue;
          }
          if (
            event.state === "done" &&
            ["preflight", "reviewing", "feedback", "landing"].includes(
              job.reviewPhase,
            )
          ) {
            blockLifecycle(job, `done is invalid during ${job.reviewPhase}`);
            continue;
          }
          job.state = event.state;
          job.summary = event.summary;
          if (event.state === "review-ready") {
            sendJobEvent(job, line, true);
            void beginReview(job);
          } else if (!jobEventTriggersTurn(event.state)) {
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
          job.state !== "landed" &&
          job.state !== "landed-with-retained-resources" &&
          job.state !== "stopped"
        ) {
          job.exitReported = true;
          if (job.reviewPhase === "awaiting-done") {
            blockLifecycle(
              job,
              "worker exited without the required review approval acknowledgment",
            );
          } else {
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

  const projectsTool = defineTool({
    name: "scufris_agent_projects",
    label: "List delegation projects",
    description:
      "List opaque Git project IDs accepted by scufris_agent_spawn. Use this before cross-project delegation.",
    parameters: Type.Object({}, { additionalProperties: false }),
    async execute() {
      try {
        return toolResult(
          await runHelper<{ projects: string[] }>("projects", {}),
        );
      } catch (error) {
        return toolResult(
          { error: error instanceof Error ? error.message : String(error) },
          true,
        );
      }
    },
  });

  const spawnTool = defineTool({
    name: "scufris_agent_spawn",
    label: "Spawn delegated agent",
    description:
      "Start one independent Pi or Claude coding worker in an isolated worktree.",
    parameters: Type.Object(
      {
        harness: StringEnum(["pi", "claude"] as const),
        project: Type.Optional(
          Type.String({
            description:
              "Opaque project ID from scufris_agent_projects. Omit to use the current Git repository.",
            pattern: "^[A-Za-z0-9_.-]+(?:/[A-Za-z0-9_.-]+)*$",
          }),
        ),
        instructions: Type.String({ minLength: 1, maxLength: 262_144 }),
        review: Type.Union([
          Type.Object(
            { profile: Type.Literal("none") },
            { additionalProperties: false },
          ),
          Type.Object(
            {
              profile: StringEnum([
                "code",
                "consumer",
                "operations",
                "interface",
              ] as const),
              brief: Type.String({
                description:
                  "Concise accepted outcome and audience for independent preflight review.",
                minLength: 1,
                maxLength: 4096,
                pattern: "^[^\\x00-\\x1f\\x7f]+$",
              }),
            },
            { additionalProperties: false },
          ),
        ]),
        feature: Type.Optional(
          Type.String({
            description:
              "Concise descriptive Sprout feature slug. Omit when the task has no clear name.",
            minLength: 1,
            maxLength: 48,
            pattern: delegatedFeaturePattern,
          }),
        ),
        model: Type.Optional(Type.String({ minLength: 1, maxLength: 200 })),
        cleanup: Type.Optional(StringEnum(["remove", "retain"] as const)),
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
        if (
          params.feature !== undefined &&
          !isDelegatedFeature(params.feature)
        ) {
          throw new Error("invalid feature");
        }
        const result = await runHelper<SpawnResult>(
          "spawn",
          {
            job_id: generatedJobId(),
            harness: params.harness,
            instructions: params.instructions,
            review: params.review,
            current_root: ctx.cwd,
            ...(params.project === undefined
              ? {}
              : { project: params.project }),
            ...(params.feature === undefined
              ? {}
              : { feature: params.feature }),
            ...(params.model === undefined ? {} : { model: params.model }),
            ...(params.cleanup === undefined
              ? {}
              : { cleanup: params.cleanup }),
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
          project: result.project,
          project_root: result.project_root,
          worktree: result.worktree,
          feature: result.feature,
          cleanup: result.cleanup,
          review: result.review,
          state: result.state,
          summary: "worker starting",
          offset: 0,
          tail: "",
          inode: null,
          window_alive: true,
          protocolErrors: new Set(),
          exitReported: false,
          reviewPhase: "idle",
          reviewRetryable: false,
          preflightFeedbackCycles: 0,
        });
        void poll();
        const {
          project_root: _projectRoot,
          worktree: _worktree,
          ...publicResult
        } = result;
        return toolResult(publicResult);
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
          project: job.project,
          state: job.state,
          summary: job.summary,
          feature: job.feature,
          cleanup: job.cleanup,
          review: job.review,
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
        const result = await runHelper("send", params, signal, 20_000);
        const job = jobs.get(params.job_id);
        if (job && ["needs-decision", "blocked"].includes(job.state)) {
          job.state = "working";
          job.summary = "Scufris response submitted to worker";
        }
        return toolResult(result);
      } catch (error) {
        return toolResult(
          { error: error instanceof Error ? error.message : String(error) },
          true,
        );
      }
    },
  });

  const retryReviewTool = defineTool({
    name: "scufris_agent_retry_review",
    label: "Retry delegated agent review",
    description:
      "Retry fresh review preconditions for one owned lifecycle-blocked job.",
    parameters: Type.Object(
      { job_id: Type.String({ pattern: "^[a-z0-9]{12}$" }) },
      { additionalProperties: false },
    ),
    async execute(_id, params) {
      const job = jobs.get(params.job_id);
      if (!job) {
        return toolResult(
          { error: "job is not owned by this Pi session" },
          true,
        );
      }
      const rejection = consumeReviewRetry(job);
      if (rejection) return toolResult({ error: rejection }, true);
      const started = await beginReview(job);
      if (!started) {
        return toolResult(
          { job_id: job.job_id, state: job.state, summary: job.summary },
          true,
        );
      }
      return toolResult({
        job_id: job.job_id,
        state: "reviewing",
        message: "Fresh review sequence started.",
      });
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
        job.reviewAbort?.abort();
        job.reviewAbort = undefined;
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

  pi.registerTool(projectsTool);
  pi.registerTool(spawnTool);
  pi.registerTool(listTool);
  pi.registerTool(
    createAgentDiagnosticsTool(jobs, options.diagnosticsInvocation),
  );
  pi.registerTool(inspectTool);
  pi.registerTool(sendTool);
  pi.registerTool(retryReviewTool);
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
    for (const job of jobs.values()) job.reviewAbort?.abort();
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
