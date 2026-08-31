import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import { Type } from "@earendil-works/pi-ai";
import {
  defineTool,
  type ExtensionAPI,
  type ExtensionContext,
} from "@earendil-works/pi-coding-agent";
import { toolResult } from "../shared/runtime.ts";
import {
  decide,
  localDate,
  parseSchedule,
  untilTomorrow,
  type RunState,
} from "./schedule.ts";

const helperPath = fileURLToPath(
  new URL("../../../tools/briefing/cli.py", import.meta.url),
);

export const BRIEFING_WAKE = "scufris-briefing";
const DATE = "^\\d{4}-\\d{2}-\\d{2}$";
const PROFILE = "^[A-Za-z0-9][A-Za-z0-9_-]*$";
const READ_TIMEOUT = 30_000;
const RUN_DEADLINE = 1800;
const COLLECT_SLACK = 120_000;
const MAX_HELPER_OUTPUT = 4 * 1024 * 1024;

interface Manifest {
  date: string;
  profile: string;
  state: RunState;
  sources: Array<{ project: string; status: string; headline: string }>;
  diagnostics: Array<{ project: string; diagnostic: string }>;
}

function collectTimeout(): number {
  const raw = Number(process.env.SCUFRIS_BRIEFING_DEADLINE);
  const deadline = Number.isFinite(raw) && raw > 0 ? raw : RUN_DEADLINE;
  return deadline * 1000 + COLLECT_SLACK;
}

/** Run the briefing helper and read its JSON answer.
 *
 * The helper is one program with a command line, not a service: a collection
 * is minutes long and everything else is a file read, so there is nothing to
 * keep open between them.
 */
export async function runHelper<T>(
  argv: string[],
  options: { stdin?: string; timeoutMs?: number } = {},
): Promise<T> {
  return await new Promise<T>((resolve, reject) => {
    const child = spawn(helperPath, argv, { stdio: ["pipe", "pipe", "pipe"] });
    const stdout: Buffer[] = [];
    const stderr: Buffer[] = [];
    let bytes = 0;
    let settled = false;
    const finish = (error?: Error, value?: T) => {
      if (settled) return;
      settled = true;
      clearTimeout(deadline);
      if (error) reject(error);
      else resolve(value as T);
    };
    const deadline = setTimeout(() => {
      child.kill("SIGTERM");
      finish(new Error(`The briefing helper timed out during ${argv[0]}`));
    }, options.timeoutMs ?? READ_TIMEOUT);
    const keep = (chunks: Buffer[]) => (chunk: Buffer) => {
      bytes += chunk.length;
      if (bytes > MAX_HELPER_OUTPUT) {
        child.kill("SIGTERM");
        finish(new Error("The briefing helper answered with too much output"));
        return;
      }
      chunks.push(chunk);
    };
    child.stdout.on("data", keep(stdout));
    child.stderr.on("data", keep(stderr));
    child.on("error", (error) => finish(error));
    child.on("close", (code) => {
      if (settled) return;
      const detail = Buffer.concat(stderr).toString("utf8").trim();
      if (code !== 0) {
        finish(new Error(detail || `The briefing helper exited ${code}`));
        return;
      }
      try {
        finish(undefined, JSON.parse(Buffer.concat(stdout).toString("utf8")));
      } catch {
        finish(new Error("The briefing helper gave an unreadable answer"));
      }
    });
    child.stdin.end(options.stdin ?? "");
  });
}

export function wakeMessage(manifest: Manifest): string {
  const answered = manifest.sources.filter(
    (source) => source.status !== "failed",
  ).length;
  const failed = manifest.sources.length - answered;
  const missing = failed ? `, ${failed} could not answer` : "";
  return `The ${manifest.profile} briefing for ${manifest.date} is collected: ${answered} source${answered === 1 ? "" : "s"} answered${missing}. Read it with scufris_briefing_show, then write the briefing yourself: one short piece in your own voice that says what today needs, built from what the sources actually reported. Do not read the sources out one after another, and claim nothing none of them measured. Name any source that could not answer. Call scufris_briefing_publish with that prose, then tell the user the same briefing in the same words.`;
}

export default function briefing(pi: ExtensionAPI): void {
  if (process.env.SCUFRIS_ROLE !== "orchestrator") return;

  // One timer, re-armed after each decision. Nothing here polls: between the
  // morning and the next one this extension holds a single pending timeout and
  // no open handles of its own.
  let timer: ReturnType<typeof setTimeout> | undefined;
  let extensionContext: ExtensionContext | undefined;
  let running = false;
  let stopped = false;

  const notify = (message: string, level: "info" | "error" = "info") => {
    if (extensionContext?.hasUI) extensionContext.ui.notify(message, level);
  };

  const profile = () => process.env.SCUFRIS_BRIEFING_PROFILE || "morning";

  const runState = async (date: string): Promise<RunState> => {
    const answer = await runHelper<{ state: RunState }>([
      "state",
      "--date",
      date,
      "--json",
    ]);
    return answer.state;
  };

  const collect = async (date: string): Promise<Manifest> =>
    await runHelper<Manifest>(
      ["collect", "--date", date, "--profile", profile(), "--json"],
      { timeoutMs: collectTimeout() },
    );

  const wake = (manifest: Manifest) => {
    pi.sendMessage(
      {
        customType: BRIEFING_WAKE,
        content: wakeMessage(manifest),
        display: true,
        details: {
          date: manifest.date,
          profile: manifest.profile,
          sources: manifest.sources.length,
        },
      },
      { deliverAs: "followUp", triggerTurn: true },
    );
  };

  const arm = (delayMs: number) => {
    if (timer !== undefined) clearTimeout(timer);
    timer = setTimeout(() => {
      timer = undefined;
      void tick();
    }, delayMs);
  };

  /** Put the day's timer back when a run of its own consumed it.
   *
   * A briefing asked for by hand can be running when the morning's timer
   * fires. That firing finds a run already going and returns, which spends the
   * one timer this extension holds. Without this the day would stop advancing
   * until the next session.
   */
  const keepTheDayGoing = () => {
    if (stopped || timer !== undefined) return;
    const setting = parseSchedule(process.env.SCUFRIS_BRIEFING_TIME);
    if (setting.kind === "at") arm(untilTomorrow(new Date(), setting));
  };

  /** Decide what today needs and do that one thing.
   *
   * Every path ends by asking again, so the day advances whether the briefing
   * was delivered, gathered and left, or never collected at all.
   */
  const tick = async (): Promise<void> => {
    if (stopped || running) return;
    const setting = parseSchedule(process.env.SCUFRIS_BRIEFING_TIME);
    if (setting.kind === "off") return;
    if (setting.kind === "invalid") {
      notify(
        `SCUFRIS_BRIEFING_TIME is not a time of day: ${setting.raw}. No briefing is scheduled.`,
        "error",
      );
      return;
    }
    const now = new Date();
    const date = localDate(now);
    let next;
    try {
      next = decide(now, setting, await runState(date));
    } catch (error) {
      notify(error instanceof Error ? error.message : String(error), "error");
      return;
    }
    if (next.do === "wait") {
      arm(next.delayMs);
      return;
    }
    if (next.do === "publish") {
      // Gathered, and its prose never written. The sources already answered,
      // so this asks for the writing and not for the morning again.
      try {
        const run = await runHelper<{ manifest: Manifest }>([
          "show",
          "--date",
          date,
          "--json",
        ]);
        if (run.manifest.sources.length > 0) wake(run.manifest);
      } catch (error) {
        notify(error instanceof Error ? error.message : String(error), "error");
      }
      arm(untilTomorrow(new Date(), setting));
      return;
    }
    running = true;
    try {
      const manifest = await collect(date);
      // A morning nothing declared is not an event. Waking the foreground to
      // say that no project asked for anything would be the only noise the
      // briefing ever made.
      if (manifest.sources.length > 0) wake(manifest);
    } catch (error) {
      notify(error instanceof Error ? error.message : String(error), "error");
    } finally {
      running = false;
    }
    arm(untilTomorrow(new Date(), setting));
  };

  pi.registerTool(
    defineTool({
      name: "scufris_briefing_run",
      label: "Collect a briefing",
      description:
        "Ask every project that declares this briefing for its contribution. Returns as soon as the run starts; the finished run arrives as a follow-up.",
      promptSnippet: "Collect the briefing from every configured project",
      promptGuidelines: [
        "Use this when the user asks for a briefing now. The scheduled morning run needs no tool call.",
        "It returns immediately. Do not wait for it; the collected run wakes you when it is ready.",
      ],
      parameters: Type.Object(
        { profile: Type.Optional(Type.String({ pattern: PROFILE })) },
        { additionalProperties: false },
      ),
      async execute(_id, params) {
        if (running)
          return toolResult({
            started: false,
            reason: "a run is already going",
          });
        const date = localDate(new Date());
        const wanted = params.profile ?? profile();
        running = true;
        void (async () => {
          try {
            const manifest = await runHelper<Manifest>(
              ["collect", "--date", date, "--profile", wanted, "--json"],
              { timeoutMs: collectTimeout() },
            );
            wake(manifest);
          } catch (error) {
            notify(
              error instanceof Error ? error.message : String(error),
              "error",
            );
          } finally {
            running = false;
            keepTheDayGoing();
          }
        })();
        return toolResult({ started: true, date, profile: wanted });
      },
    }),
  );

  pi.registerTool(
    defineTool({
      name: "scufris_briefing_show",
      label: "Read a briefing run",
      description:
        "Read one briefing run: every source's contribution, the prose if it has been written, and any project that could not answer.",
      promptSnippet: "Read the collected briefing for a day",
      parameters: Type.Object(
        { date: Type.Optional(Type.String({ pattern: DATE })) },
        { additionalProperties: false },
      ),
      async execute(_id, params) {
        const date = params.date ?? localDate(new Date());
        return toolResult(await runHelper(["show", "--date", date, "--json"]));
      },
    }),
  );

  pi.registerTool(
    defineTool({
      name: "scufris_briefing_publish",
      label: "Publish a briefing",
      description:
        "Keep the briefing you wrote with its run and render the page from the same run.",
      promptSnippet: "Keep the briefing you wrote and render its page",
      promptGuidelines: [
        "Write the prose yourself from the contributions. This tool keeps what you wrote; it writes nothing of its own.",
        "Publish before telling the user, and tell them the same briefing you published.",
      ],
      parameters: Type.Object(
        {
          prose: Type.String({ minLength: 1 }),
          date: Type.Optional(Type.String({ pattern: DATE })),
        },
        { additionalProperties: false },
      ),
      async execute(_id, params) {
        const date = params.date ?? localDate(new Date());
        return toolResult(
          await runHelper(["publish", "--date", date, "--json"], {
            stdin: params.prose,
          }),
        );
      },
    }),
  );

  pi.registerTool(
    defineTool({
      name: "scufris_briefing_open",
      label: "Open the briefing page",
      description:
        "Open a briefing's page on this machine. The page opens only when it is asked for.",
      promptSnippet: "Open the briefing page on this machine",
      parameters: Type.Object(
        { date: Type.Optional(Type.String({ pattern: DATE })) },
        { additionalProperties: false },
      ),
      async execute(_id, params) {
        const date = params.date ?? localDate(new Date());
        return toolResult(await runHelper(["open", "--date", date, "--json"]));
      },
    }),
  );

  pi.on("session_start", async (_event, ctx) => {
    extensionContext = ctx;
    stopped = false;
    await tick();
  });

  pi.on("session_shutdown", () => {
    stopped = true;
    if (timer !== undefined) clearTimeout(timer);
    timer = undefined;
    extensionContext = undefined;
  });
}
