import { spawn } from "node:child_process";

const maxHelperOutput = 2 * 1024 * 1024;

export interface HelperEnvelope<T> {
  ok: boolean;
  result?: T;
  error?: string;
  error_code?: string;
}

type TimeoutHandle = ReturnType<typeof setTimeout>;
type ScheduleTimeout = (callback: () => void, delayMs: number) => TimeoutHandle;
type CancelTimeout = (handle: TimeoutHandle) => void;

export function createRestartableDeadline(
  timeoutMs: number,
  onTimeout: () => void,
  schedule: ScheduleTimeout = setTimeout,
  cancel: CancelTimeout = clearTimeout,
) {
  let generation = 0;
  let timer: TimeoutHandle | undefined;

  const restart = () => {
    generation += 1;
    if (timer !== undefined) cancel(timer);
    const armedGeneration = generation;
    timer = schedule(() => {
      if (generation === armedGeneration) onTimeout();
    }, timeoutMs);
  };
  const clear = () => {
    generation += 1;
    if (timer !== undefined) cancel(timer);
    timer = undefined;
  };

  restart();
  return { restart, clear };
}

export function toolResult(value: unknown, isError = false, text?: string) {
  return {
    content: [
      {
        type: "text" as const,
        text: text ?? JSON.stringify(value, null, 2),
      },
    ],
    details: value,
    ...(isError ? { isError: true } : {}),
  };
}

export async function runPrivateHelper<T>(
  helperPath: string,
  command: string,
  request: unknown,
  signal?: AbortSignal,
  timeoutMs = 30_000,
  readyLine?: string,
): Promise<HelperEnvelope<T>> {
  return await new Promise<HelperEnvelope<T>>((resolve, reject) => {
    const env = { ...process.env };
    if (readyLine === undefined) delete env.SCUFRIS_HELPER_READY_LINE;
    else env.SCUFRIS_HELPER_READY_LINE = readyLine;
    const child = spawn(helperPath, [command], {
      env,
      stdio: ["pipe", "pipe", "pipe"],
    });
    const stdout: Buffer[] = [];
    const stderr: Buffer[] = [];
    let outputBytes = 0;
    let settled = false;
    let ready = false;
    const encodedReadyLine =
      readyLine === undefined ? undefined : Buffer.from(`${readyLine}\n`);

    const finish = (error?: Error, value?: HelperEnvelope<T>) => {
      if (settled) return;
      settled = true;
      deadline.clear();
      signal?.removeEventListener("abort", abort);
      if (error) reject(error);
      else resolve(value as HelperEnvelope<T>);
    };
    const abort = () => {
      child.kill("SIGTERM");
      finish(new Error("Scufris helper request aborted"));
    };
    const deadline = createRestartableDeadline(timeoutMs, () => {
      child.kill("SIGTERM");
      finish(new Error(`Scufris helper timed out during ${command}`));
    });

    signal?.addEventListener("abort", abort, { once: true });
    child.stdout.on("data", (chunk: Buffer) => {
      outputBytes += chunk.length;
      if (outputBytes > maxHelperOutput) {
        child.kill("SIGTERM");
        finish(new Error("Scufris helper output exceeded 2 MiB"));
        return;
      }
      stdout.push(chunk);
    });
    child.stderr.on("data", (chunk: Buffer) => {
      outputBytes += chunk.length;
      if (outputBytes > maxHelperOutput) {
        child.kill("SIGTERM");
        finish(new Error("Scufris helper output exceeded 2 MiB"));
        return;
      }
      stderr.push(chunk);
      if (
        !ready &&
        encodedReadyLine !== undefined &&
        Buffer.concat(stderr).includes(encodedReadyLine)
      ) {
        ready = true;
        deadline.restart();
      }
    });
    child.on("error", (error) => finish(error));
    child.on("close", () => {
      if (settled) return;
      const text = Buffer.concat(stdout).toString("utf8");
      let envelope: HelperEnvelope<T>;
      try {
        envelope = JSON.parse(text) as HelperEnvelope<T>;
      } catch {
        const diagnostic = Buffer.concat(stderr).toString("utf8").trim();
        finish(
          new Error(
            `Invalid Scufris helper response${diagnostic ? `: ${diagnostic}` : ""}`,
          ),
        );
        return;
      }
      finish(undefined, envelope);
    });
    child.stdin.end(JSON.stringify(request));
  });
}
