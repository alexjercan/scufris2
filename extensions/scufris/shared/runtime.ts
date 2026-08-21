import { spawn } from "node:child_process";

const maxHelperOutput = 2 * 1024 * 1024;

export interface HelperEnvelope<T> {
  ok: boolean;
  result?: T;
  error?: string;
  error_code?: string;
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
): Promise<HelperEnvelope<T>> {
  return await new Promise<HelperEnvelope<T>>((resolve, reject) => {
    const child = spawn(helperPath, [command], {
      env: process.env,
      stdio: ["pipe", "pipe", "pipe"],
    });
    const stdout: Buffer[] = [];
    const stderr: Buffer[] = [];
    let outputBytes = 0;
    let settled = false;

    const finish = (error?: Error, value?: HelperEnvelope<T>) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      signal?.removeEventListener("abort", abort);
      if (error) reject(error);
      else resolve(value as HelperEnvelope<T>);
    };
    const abort = () => {
      child.kill("SIGTERM");
      finish(new Error("Scufris helper request aborted"));
    };
    const timer = setTimeout(() => {
      child.kill("SIGTERM");
      finish(new Error(`Scufris helper timed out during ${command}`));
    }, timeoutMs);

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
