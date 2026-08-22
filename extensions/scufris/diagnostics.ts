import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import { Type } from "@earendil-works/pi-ai";
import { defineTool } from "@earendil-works/pi-coding-agent";
import { toolResult } from "./shared/runtime.ts";

const diagnosticsHelperPath = fileURLToPath(
  new URL("../../scripts/scufris-jobs", import.meta.url),
);
const jobIdPattern = /^[a-z0-9]{12}$/;
const maxHelperOutput = 8 * 1024 * 1024;
const maxReportBytes = 32 * 1024;
const maxDiagnosticItems = 12;
const maxStatusEvents = 100;

export interface DiagnosticsParams {
  job_id?: string;
  include_finished?: boolean;
  include_report?: boolean;
}

export interface DiagnosticsProcessResult {
  code: number;
  stdout: Buffer;
}

export type DiagnosticsInvocation = (
  args: string[],
  signal?: AbortSignal,
) => Promise<DiagnosticsProcessResult>;

interface OwnershipLookup {
  has(jobId: string): boolean;
}

export const diagnosticsParameters = Type.Object(
  {
    job_id: Type.Optional(Type.String({ pattern: "^[a-z0-9]{12}$" })),
    include_finished: Type.Optional(Type.Boolean({ default: false })),
    include_report: Type.Optional(Type.Boolean({ default: false })),
  },
  { additionalProperties: false },
);

function conciseError(error: unknown): string {
  const message = error instanceof Error ? error.message : String(error);
  return sanitizeText(message, 300, false);
}

function boundedUtf8(value: string, maximum: number): string {
  const encoded = Buffer.from(value, "utf8");
  if (encoded.length <= maximum) return value;
  let bounded = encoded.subarray(0, Math.max(0, maximum - 3)).toString("utf8");
  if (bounded.endsWith("\ufffd")) bounded = bounded.slice(0, -1);
  return `${bounded}...`;
}

export function sanitizeText(
  value: string,
  maximum: number,
  multiline: boolean,
): string {
  let text = value
    .replace(/\r\n?/g, "\n")
    .replace(/[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]/g, " ")
    .replace(/\b[A-Za-z][A-Za-z0-9+.-]*:\/\/[^\s<>]+/g, "[redacted-url]")
    .replace(/\bwww\.[^\s<>]+/gi, "[redacted-url]")
    .replace(/\b[A-Za-z]:\\[^\s<>"'`]+/g, "[redacted-path]")
    .replace(/(^|[\s("'`=])\/(?:[^\s<>"'`|]+\/?)+/gm, "$1[redacted-path]")
    .replace(/(^|[\s("'`=])~\/(?:[^\s<>"'`|]+\/?)+/gm, "$1[redacted-path]")
    .replace(
      /\b(api[_ -]?key|authorization|credential|password|secret|token)\s*[:=]\s*[^\s,;]+/gi,
      "$1=[redacted]",
    )
    .replace(
      /\b(?:gh[opsu]_[A-Za-z0-9_]{12,}|sk-[A-Za-z0-9_-]{12,})\b/g,
      "[redacted-credential]",
    )
    .replace(
      /^(prompt|pane transcript|transcript|environment|env):.*$/gim,
      "$1: [redacted]",
    );
  if (!multiline) text = text.replace(/[\n\t]+/g, " ");
  return boundedUtf8(text, maximum);
}

function sanitizeReportText(value: string): string {
  const sanitized = sanitizeText(value, 1024 * 1024, true)
    .replace(
      /^(?:export\s+)?[A-Za-z_][A-Za-z0-9_]*\s*=.*$/gm,
      "[redacted-environment]",
    )
    .replace(/\b[A-Za-z0-9_+/-]{32,}={0,2}\b/g, "[redacted-credential]");
  return boundedUtf8(sanitized, maxReportBytes);
}

function exactObject(
  value: unknown,
  keys: readonly string[],
  name: string,
): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`Invalid Scufris diagnostics response: ${name}`);
  }
  const object = value as Record<string, unknown>;
  const actual = Object.keys(object).sort();
  const expected = [...keys].sort();
  if (
    actual.length !== expected.length ||
    actual.some((key, index) => key !== expected[index])
  ) {
    throw new Error(`Invalid Scufris diagnostics response: ${name} fields`);
  }
  return object;
}

function stringValue(value: unknown, name: string, maximum = 4096): string {
  if (
    typeof value !== "string" ||
    Buffer.byteLength(value, "utf8") > maximum ||
    value.includes("\0")
  ) {
    throw new Error(`Invalid Scufris diagnostics response: ${name}`);
  }
  return value;
}

function nullableString(value: unknown, name: string): string | null {
  return value === null ? null : stringValue(value, name);
}

function booleanValue(value: unknown, name: string): boolean {
  if (typeof value !== "boolean") {
    throw new Error(`Invalid Scufris diagnostics response: ${name}`);
  }
  return value;
}

function nullableBoolean(value: unknown, name: string): boolean | null {
  return value === null ? null : booleanValue(value, name);
}

function integerValue(value: unknown, name: string): number {
  if (!Number.isSafeInteger(value) || (value as number) < 0) {
    throw new Error(`Invalid Scufris diagnostics response: ${name}`);
  }
  return value as number;
}

function nullableInteger(value: unknown, name: string): number | null {
  return value === null ? null : integerValue(value, name);
}

function jobIdValue(value: unknown, name = "job_id"): string {
  const jobId = stringValue(value, name, 12);
  if (!jobIdPattern.test(jobId)) {
    throw new Error(`Invalid Scufris diagnostics response: ${name}`);
  }
  return jobId;
}

function shaValue(value: unknown, name: string): string {
  const sha = stringValue(value, name, 40);
  if (!/^[0-9a-f]{40}$/.test(sha)) {
    throw new Error(`Invalid Scufris diagnostics response: ${name}`);
  }
  return sha;
}

function nullableSha(value: unknown, name: string): string | null {
  return value === null ? null : shaValue(value, name);
}

function diagnosticValues(
  value: unknown,
  name: string,
  maximumItems = maxDiagnosticItems,
) {
  if (!Array.isArray(value) || value.length > 100) {
    throw new Error(`Invalid Scufris diagnostics response: ${name}`);
  }
  const diagnostics = value.map((item, index) =>
    sanitizeText(
      stringValue(item, `${name}[${index}]`, 256 * 1024),
      500,
      false,
    ),
  );
  return {
    diagnostics: diagnostics.slice(0, maximumItems),
    ...(diagnostics.length > maximumItems
      ? { diagnostics_truncated: diagnostics.length - maximumItems }
      : {}),
  };
}

function statusValue(value: unknown, name: string): string {
  const status = stringValue(value, name, 32);
  if (!/^[a-z][a-z-]*$/.test(status)) {
    throw new Error(`Invalid Scufris diagnostics response: ${name}`);
  }
  return status;
}

function reviewValue(value: unknown, name: string) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`Invalid Scufris diagnostics response: ${name}`);
  }
  const object = value as Record<string, unknown>;
  const profile = stringValue(object.profile, `${name}.profile`, 10);
  if (profile === "none") {
    exactObject(value, ["profile"], name);
    return { profile: "none" as const };
  }
  if (!new Set(["code", "consumer", "operations", "interface"]).has(profile)) {
    throw new Error(`Invalid Scufris diagnostics response: ${name}.profile`);
  }
  exactObject(value, ["profile", "brief"], name);
  return {
    profile,
    brief: sanitizeText(
      stringValue(object.brief, `${name}.brief`, 4096),
      4096,
      false,
    ),
  };
}

function sanitizeList(
  value: unknown,
  ownership: OwnershipLookup,
  expectedScope: "live" | "all",
) {
  const root = exactObject(value, ["scope", "jobs"], "list");
  const scope = stringValue(root.scope, "scope", 4);
  if (scope !== expectedScope) {
    throw new Error("Invalid Scufris diagnostics response: scope");
  }
  if (!Array.isArray(root.jobs) || root.jobs.length > 256) {
    throw new Error("Invalid Scufris diagnostics response: jobs");
  }
  const jobs = root.jobs.map((rawJob, index) => {
    const prefix = `jobs[${index}]`;
    if (!rawJob || typeof rawJob !== "object" || Array.isArray(rawJob)) {
      throw new Error(`Invalid Scufris diagnostics response: ${prefix}`);
    }
    const valid = (rawJob as Record<string, unknown>).valid;
    if (valid === false) {
      const job = exactObject(
        rawJob,
        ["job_id", "valid", "diagnostics"],
        prefix,
      );
      const jobId = jobIdValue(job.job_id, `${prefix}.job_id`);
      return {
        job_id: jobId,
        valid: false,
        owned_by_current_session: ownership.has(jobId),
        ...diagnosticValues(job.diagnostics, `${prefix}.diagnostics`, 3),
      };
    }
    const job = exactObject(
      rawJob,
      [
        "job_id",
        "valid",
        "project",
        "feature",
        "harness",
        "model",
        "state",
        "summary",
        "created_at",
        "elapsed_seconds",
        "tmux_session",
        "pane_liveness",
        "cleanup",
        "review",
        "diagnostics",
      ],
      prefix,
    );
    if (job.valid !== true) {
      throw new Error(`Invalid Scufris diagnostics response: ${prefix}.valid`);
    }
    const jobId = jobIdValue(job.job_id, `${prefix}.job_id`);
    stringValue(job.tmux_session, `${prefix}.tmux_session`, 128);
    return {
      job_id: jobId,
      valid: true,
      owned_by_current_session: ownership.has(jobId),
      project: sanitizeText(
        stringValue(job.project, `${prefix}.project`, 256),
        256,
        false,
      ),
      feature: sanitizeText(
        stringValue(job.feature, `${prefix}.feature`, 48),
        48,
        false,
      ),
      harness: sanitizeText(
        stringValue(job.harness, `${prefix}.harness`, 16),
        16,
        false,
      ),
      model: sanitizeText(
        stringValue(job.model, `${prefix}.model`, 200),
        200,
        false,
      ),
      state: statusValue(job.state, `${prefix}.state`),
      summary: sanitizeText(
        stringValue(job.summary, `${prefix}.summary`, 2048),
        500,
        false,
      ),
      created_at: sanitizeText(
        stringValue(job.created_at, `${prefix}.created_at`, 32),
        32,
        false,
      ),
      elapsed_seconds: integerValue(
        job.elapsed_seconds,
        `${prefix}.elapsed_seconds`,
      ),
      pane_liveness: sanitizeText(
        stringValue(job.pane_liveness, `${prefix}.pane_liveness`, 32),
        32,
        false,
      ),
      cleanup: sanitizeText(
        stringValue(job.cleanup, `${prefix}.cleanup`, 16),
        16,
        false,
      ),
      review: reviewValue(job.review, `${prefix}.review`),
      ...diagnosticValues(job.diagnostics, `${prefix}.diagnostics`, 3),
    };
  });
  return { scope, jobs };
}

function sanitizeDetail(value: unknown, ownership: OwnershipLookup) {
  const root = exactObject(
    value,
    [
      "job_id",
      "metadata",
      "state",
      "summary",
      "created_at",
      "elapsed_seconds",
      "pane_liveness",
      "tmux",
      "status",
      "report",
      "git",
      "diagnostics",
    ],
    "detail",
  );
  const jobId = jobIdValue(root.job_id);
  const metadata = exactObject(
    root.metadata,
    [
      "version",
      "job_id",
      "harness",
      "model",
      "thinking",
      "feature",
      "cleanup",
      "review",
      "project",
      "landing_branch",
      "landing_sha",
      "tmux_session",
      "tmux_session_id",
      "tmux_window_id",
      "tmux_pane_id",
      "created_at",
    ],
    "metadata",
  );
  if (
    metadata.version !== 2 ||
    jobIdValue(metadata.job_id, "metadata.job_id") !== jobId
  ) {
    throw new Error("Invalid Scufris diagnostics response: metadata identity");
  }
  for (const key of [
    "harness",
    "model",
    "thinking",
    "feature",
    "cleanup",
    "project",
    "landing_branch",
    "tmux_session",
    "tmux_session_id",
    "tmux_window_id",
    "tmux_pane_id",
    "created_at",
  ]) {
    stringValue(metadata[key], `metadata.${key}`);
  }
  const review = reviewValue(metadata.review, "metadata.review");
  shaValue(metadata.landing_sha, "metadata.landing_sha");
  const tmux = exactObject(
    root.tmux,
    ["session", "session_id", "window_id", "pane_id"],
    "tmux",
  );
  for (const key of ["session", "session_id", "window_id", "pane_id"]) {
    stringValue(tmux[key], `tmux.${key}`, 128);
  }
  const status = exactObject(
    root.status,
    ["size_bytes", "events", "protocol_errors"],
    "status",
  );
  const statusSize = nullableInteger(status.size_bytes, "status.size_bytes");
  if (!Array.isArray(status.events) || status.events.length > maxStatusEvents) {
    throw new Error("Invalid Scufris diagnostics response: status.events");
  }
  const events = status.events.map((event, index) =>
    sanitizeText(
      stringValue(event, `status.events[${index}]`, 2048),
      1000,
      false,
    ),
  );
  const protocol = diagnosticValues(
    status.protocol_errors,
    "status.protocol_errors",
  );
  const report = exactObject(root.report, ["size_bytes", "content"], "report");
  const reportSize = nullableInteger(report.size_bytes, "report.size_bytes");
  const reportContent =
    report.content === null
      ? null
      : stringValue(report.content, "report.content", 1024 * 1024);
  const sanitizedReport =
    reportContent === null ? null : sanitizeReportText(reportContent);
  const git = exactObject(
    root.git,
    [
      "path",
      "exists",
      "branch",
      "revision",
      "clean",
      "recorded_landing_revision",
      "recorded_landing_revision_valid",
    ],
    "git",
  );
  nullableString(git.path, "git.path");
  const branch = nullableString(git.branch, "git.branch");
  const revision = nullableSha(git.revision, "git.revision");
  const recordedRevision = shaValue(
    git.recorded_landing_revision,
    "git.recorded_landing_revision",
  );
  return {
    job_id: jobId,
    valid: true,
    owned_by_current_session: ownership.has(jobId),
    project: sanitizeText(
      stringValue(metadata.project, "metadata.project", 256),
      256,
      false,
    ),
    feature: sanitizeText(
      stringValue(metadata.feature, "metadata.feature", 48),
      48,
      false,
    ),
    harness: sanitizeText(
      stringValue(metadata.harness, "metadata.harness", 16),
      16,
      false,
    ),
    model: sanitizeText(
      stringValue(metadata.model, "metadata.model", 200),
      200,
      false,
    ),
    review,
    thinking: sanitizeText(
      stringValue(metadata.thinking, "metadata.thinking", 16),
      16,
      false,
    ),
    cleanup: sanitizeText(
      stringValue(metadata.cleanup, "metadata.cleanup", 16),
      16,
      false,
    ),
    state: statusValue(root.state, "state"),
    summary: sanitizeText(
      stringValue(root.summary, "summary", 2048),
      500,
      false,
    ),
    created_at: sanitizeText(
      stringValue(root.created_at, "created_at", 32),
      32,
      false,
    ),
    elapsed_seconds: integerValue(root.elapsed_seconds, "elapsed_seconds"),
    pane_liveness: sanitizeText(
      stringValue(root.pane_liveness, "pane_liveness", 32),
      32,
      false,
    ),
    status: {
      size_bytes: statusSize,
      events,
      protocol_errors: protocol.diagnostics,
      ...(protocol.diagnostics_truncated === undefined
        ? {}
        : { protocol_errors_truncated: protocol.diagnostics_truncated }),
    },
    report: {
      size_bytes: reportSize,
      content: sanitizedReport,
      ...(reportContent !== null &&
      Buffer.byteLength(reportContent, "utf8") > maxReportBytes
        ? { content_truncated: true }
        : {}),
    },
    repository: {
      exists: booleanValue(git.exists, "git.exists"),
      branch: branch === null ? null : sanitizeText(branch, 255, false),
      revision: revision === null ? null : sanitizeText(revision, 64, false),
      clean: nullableBoolean(git.clean, "git.clean"),
      recorded_landing_revision: sanitizeText(recordedRevision, 64, false),
      recorded_landing_revision_valid: nullableBoolean(
        git.recorded_landing_revision_valid,
        "git.recorded_landing_revision_valid",
      ),
    },
    ...diagnosticValues(root.diagnostics, "diagnostics"),
  };
}

export async function invokePackagedDiagnostics(
  args: string[],
  signal?: AbortSignal,
  helperPath = diagnosticsHelperPath,
  timeoutMs = 30_000,
  outputLimit = maxHelperOutput,
): Promise<DiagnosticsProcessResult> {
  if (signal?.aborted) {
    throw new Error("Scufris diagnostics request aborted");
  }
  return await new Promise((resolve, reject) => {
    const child = spawn(helperPath, args, {
      env: process.env,
      shell: false,
      stdio: ["ignore", "pipe", "ignore"],
    });
    const chunks: Buffer[] = [];
    let bytes = 0;
    let settled = false;
    const finish = (error?: Error, result?: DiagnosticsProcessResult) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      signal?.removeEventListener("abort", abort);
      if (error) reject(error);
      else resolve(result as DiagnosticsProcessResult);
    };
    const stop = (message: string) => {
      child.kill("SIGTERM");
      finish(new Error(message));
    };
    const abort = () => stop("Scufris diagnostics request aborted");
    const timer = setTimeout(
      () => stop("Scufris diagnostics helper timed out"),
      timeoutMs,
    );
    signal?.addEventListener("abort", abort, { once: true });
    child.stdout.on("data", (chunk: Buffer) => {
      bytes += chunk.length;
      if (bytes > outputLimit) {
        stop("Scufris diagnostics helper output exceeded the limit");
        return;
      }
      chunks.push(chunk);
    });
    child.on("error", () =>
      finish(new Error("Scufris diagnostics helper could not be executed")),
    );
    child.on("close", (code) => {
      if (settled) return;
      finish(undefined, { code: code ?? 1, stdout: Buffer.concat(chunks) });
    });
  });
}

function helperArguments(params: DiagnosticsParams): string[] {
  if (params.include_report && !params.job_id) {
    throw new Error("include_report requires job_id");
  }
  if (params.include_finished && params.job_id) {
    throw new Error("include_finished cannot be used with job_id");
  }
  return [
    ...(params.job_id ? [params.job_id] : []),
    ...(params.include_finished ? ["--all"] : []),
    ...(params.include_report ? ["--report"] : []),
    "--json",
  ];
}

export async function agentDiagnostics(
  params: DiagnosticsParams,
  ownership: OwnershipLookup,
  invoke: DiagnosticsInvocation = invokePackagedDiagnostics,
  signal?: AbortSignal,
) {
  const args = helperArguments(params);
  const process = await invoke(args, signal);
  let value: unknown;
  try {
    value = JSON.parse(
      new TextDecoder("utf-8", { fatal: true }).decode(process.stdout),
    );
  } catch {
    throw new Error("Scufris diagnostics helper returned invalid JSON");
  }
  if (process.code !== 0) {
    let message = "helper process failed";
    if (value && typeof value === "object" && !Array.isArray(value)) {
      const error = (value as Record<string, unknown>).error;
      if (typeof error === "string") message = sanitizeText(error, 300, false);
    }
    throw new Error(`Scufris diagnostics failed: ${message}`);
  }
  return params.job_id
    ? sanitizeDetail(value, ownership)
    : sanitizeList(value, ownership, params.include_finished ? "all" : "live");
}

export function createAgentDiagnosticsTool(
  ownership: OwnershipLookup,
  invoke: DiagnosticsInvocation = invokePackagedDiagnostics,
) {
  return defineTool({
    name: "scufris_agent_diagnostics",
    label: "Inspect durable agent diagnostics",
    description:
      "Read bounded sanitized durable Scufris job diagnostics across sessions. Discovery does not grant control of a job.",
    parameters: diagnosticsParameters,
    async execute(_id, params, signal) {
      try {
        return toolResult(
          await agentDiagnostics(params, ownership, invoke, signal),
        );
      } catch (error) {
        return toolResult({ error: conciseError(error) }, true);
      }
    },
  });
}
