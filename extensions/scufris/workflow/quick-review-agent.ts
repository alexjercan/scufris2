import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import { fileURLToPath } from "node:url";

const MAX_LINE_BYTES = 4 * 1024 * 1024;
const helperPath = fileURLToPath(
  new URL(
    "../../../tools/quick-review-agent/scufris-quick-review-agent",
    import.meta.url,
  ),
);

export interface QuickReviewCompletion {
  version: 1;
  outcome: "approved" | "changes-requested";
  repository: string;
  baseRef: string;
  targetRef: string;
  baseRevision: string;
  revision: string;
  identity: string;
  sections: number;
  comments: Array<{
    sectionId: string;
    file: string;
    lines: string;
    body: string;
  }>;
  overallComment: string;
  questions: Array<{ sectionId: string; question: string; answer: string }>;
  artifact: string;
  state: string;
  completedAt: string;
}

export interface QuickReviewAgentInput {
  repository: string;
  base_revision: string;
  revision: string;
  model: string;
  thinking: string;
  state_dir: string;
}

export interface QuickReviewAgent {
  completion: Promise<QuickReviewCompletion>;
  close(): Promise<void>;
}

function errorDetail(stderr: string, code: number | null): Error {
  const detail = stderr.trim().slice(-1000);
  return new Error(
    `Quick Review agent exited before completion (${code}): ${detail || "no error detail"}`,
  );
}

export async function startQuickReviewAgent(
  input: QuickReviewAgentInput,
  options: { helperPath?: string; signal?: AbortSignal } = {},
): Promise<QuickReviewAgent> {
  const child: ChildProcessWithoutNullStreams = spawn(
    options.helperPath ?? helperPath,
    [],
    { stdio: ["pipe", "pipe", "pipe"] },
  );
  let stderr = "";
  let buffer = Buffer.alloc(0);
  let readySettled = false;
  let completionSettled = false;
  let closeRequested = false;
  let resolveReady!: () => void;
  let rejectReady!: (error: Error) => void;
  let resolveCompletion!: (event: QuickReviewCompletion) => void;
  let rejectCompletion!: (error: Error) => void;
  const ready = new Promise<void>((resolve, reject) => {
    resolveReady = resolve;
    rejectReady = reject;
  });
  const completion = new Promise<QuickReviewCompletion>((resolve, reject) => {
    resolveCompletion = resolve;
    rejectCompletion = reject;
  });
  // A caller can intentionally ignore the terminal review while closing a
  // workflow. Keep that from becoming an unhandled rejection.
  void completion.catch(() => undefined);

  const fail = (error: Error) => {
    if (!readySettled) {
      readySettled = true;
      rejectReady(error);
    }
    if (!completionSettled) {
      completionSettled = true;
      rejectCompletion(error);
    }
  };
  const parseLine = (line: Buffer) => {
    let value: unknown;
    try {
      value = JSON.parse(line.toString("utf8"));
    } catch {
      fail(new Error("Quick Review agent returned invalid JSON"));
      child.kill("SIGTERM");
      return;
    }
    if (!value || typeof value !== "object" || Array.isArray(value)) {
      fail(new Error("Quick Review agent returned an invalid message"));
      child.kill("SIGTERM");
      return;
    }
    const message = value as { type?: unknown; event?: unknown };
    if (message.type === "ready") {
      if (!readySettled) {
        readySettled = true;
        resolveReady();
      }
      return;
    }
    if (message.type === "completed") {
      if (!message.event || typeof message.event !== "object") {
        fail(new Error("Quick Review agent completion is invalid"));
        child.kill("SIGTERM");
        return;
      }
      if (!completionSettled) {
        completionSettled = true;
        resolveCompletion(message.event as QuickReviewCompletion);
      }
    }
  };

  child.stderr.setEncoding("utf8");
  child.stderr.on("data", (chunk: string) => {
    stderr = (stderr + chunk).slice(-4096);
  });
  child.stdout.on("data", (chunk: Buffer) => {
    buffer = Buffer.concat([buffer, chunk]);
    if (buffer.length > MAX_LINE_BYTES) {
      fail(new Error("Quick Review agent output exceeds 4 MiB"));
      child.kill("SIGTERM");
      return;
    }
    while (true) {
      const newline = buffer.indexOf(0x0a);
      if (newline < 0) break;
      let line = buffer.subarray(0, newline);
      buffer = buffer.subarray(newline + 1);
      if (line.at(-1) === 0x0d) line = line.subarray(0, -1);
      parseLine(line);
    }
  });
  child.on("error", (error) => fail(error));
  const exited = new Promise<void>((resolve) => {
    child.once("exit", (code) => {
      if (!completionSettled && !closeRequested)
        fail(errorDetail(stderr, code));
      resolve();
    });
  });
  child.stdin.on("error", (error) => {
    if (!closeRequested) fail(error);
  });
  child.stdin.end(`${JSON.stringify(input)}\n`);

  const close = async () => {
    if (closeRequested) return exited;
    closeRequested = true;
    if (child.exitCode === null) child.kill("SIGTERM");
    await exited;
  };
  const abort = () => void close();
  options.signal?.addEventListener("abort", abort, { once: true });
  try {
    await ready;
  } catch (error) {
    await close();
    throw error;
  } finally {
    options.signal?.removeEventListener("abort", abort);
  }
  return { completion, close };
}
