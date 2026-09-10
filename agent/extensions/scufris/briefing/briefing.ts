import { spawn } from "node:child_process";
import { randomBytes } from "node:crypto";
import { Type } from "@earendil-works/pi-ai";
import {
  defineTool,
  type ExtensionAPI,
  type ExtensionContext,
} from "@earendil-works/pi-coding-agent";
import { toolPath, toolResult } from "../shared/runtime.ts";
import { DEFAULT_PROFILE, localDate, type RunState } from "./run.ts";

const helperPath = toolPath("briefing/cli.py", import.meta.url);

export const BRIEFING_WAKE = "scufris-briefing";
const DATE = "^\\d{4}-\\d{2}-\\d{2}$";
const PROFILE = "^[A-Za-z0-9][A-Za-z0-9_-]{0,63}$";
const READ_TIMEOUT = 30_000;
const COLLECT_SLACK = 120_000;
const MAX_HELPER_OUTPUT = 4 * 1024 * 1024;

interface Manifest {
  date: string;
  profile: string;
  state: RunState;
  sources: Array<{ project: string; status: string; headline: string }>;
  diagnostics: Array<{ project: string; diagnostic: string }>;
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

export default function briefing(pi: ExtensionAPI): void {
  if (process.env.SCUFRIS_ROLE !== "orchestrator") return;

  // Collection stays in systemd. Session startup reconciles durable run files
  // with the service inbox; the systemd reconciliation timer covers long-lived
  // sessions without putting a clock inside the capability extension.
  let extensionContext: ExtensionContext | undefined;
  let running = false;

  const notify = (message: string, level: "info" | "error" = "info") => {
    if (extensionContext?.hasUI) extensionContext.ui.notify(message, level);
  };

  /** Replay all retained generations into the durable service store.
   *
   * Collector announcements are latency hints. This startup scan and the
   * external periodic scan are authoritative, so a stopped service or a
   * missed event does not make a terminal generation disappear.
   */
  const reconcile = async (): Promise<void> => {
    try {
      await runHelper(["reconcile", "--json"]);
    } catch (error) {
      notify(error instanceof Error ? error.message : String(error), "error");
    }
  };

  pi.registerTool(
    defineTool({
      name: "scufris_briefing_run",
      label: "Collect a briefing",
      description:
        "Ask every project that declares this briefing for its contribution. Returns as soon as the run starts; the finished run arrives as a follow-up.",
      promptSnippet: "Collect the briefing from every configured project",
      promptGuidelines: [
        "Use this when the user asks for a briefing now. A scheduled briefing needs no tool call: its own timer collects it and wakes you.",
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
        const wanted = params.profile ?? DEFAULT_PROFILE;
        const generation = randomBytes(12).toString("hex");
        running = true;
        void (async () => {
          try {
            const bounds = await runHelper<{ deadline: number }>([
              "bounds",
              "--profile",
              wanted,
              "--json",
            ]);
            const manifest = await runHelper<Manifest>(
              [
                "collect",
                "--date",
                date,
                "--profile",
                wanted,
                "--generation",
                generation,
                "--json",
              ],
              { timeoutMs: bounds.deadline * 1000 + COLLECT_SLACK },
            );
            // The helper has already announced each durable state. Repeat the
            // terminal upsert so a service restart in the last instant still
            // gets it. The control acknowledgement does not require an agent.
            if (manifest.sources.length > 0)
              await runHelper([
                "wake",
                "--date",
                date,
                "--profile",
                wanted,
                "--json",
              ]);
          } catch (error) {
            const reason =
              error instanceof Error ? error.message : String(error);
            notify(reason, "error");
            // If collection wrote a generation before it stopped, close that
            // generation and let the durable service path produce the one
            // correlated terminal turn. A helper failure never injects a
            // second model turn directly.
            try {
              await runHelper([
                "finalize",
                "--date",
                date,
                "--profile",
                wanted,
                "--generation",
                generation,
                "--cause",
                reason,
                "--json",
              ]);
              await reconcile();
            } catch {
              // No run was started, or its collector is still live. The tool
              // notification is the complete result in either case.
            }
          } finally {
            running = false;
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
      promptGuidelines: [
        "Name the profile you were woken for. Without one this reads the single run for the day that is waiting to be written up, and refuses when two are.",
        "The run carries its offers already numbered. End the briefing with those numbers exactly, and add none of your own.",
        "When Alex picks by number, read the run again and act on the offer that number belongs to. Never act on a number the run does not carry, and never invent a step the list does not have.",
        "A source only reports. An offer that wants a file changed is a job you spawn with scufris_job_spawn, in a sprout workspace and never on the project itself, so the work is a branch Alex reads before it lands and a job he can see, steer and cancel.",
      ],
      parameters: Type.Object(
        {
          date: Type.Optional(Type.String({ pattern: DATE })),
          profile: Type.Optional(Type.String({ pattern: PROFILE })),
        },
        { additionalProperties: false },
      ),
      async execute(_id, params) {
        return toolResult(
          await runHelper(["show", ...runArguments(params), "--json"]),
        );
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
        "Publish into the run you read: name the same profile you were woken for. Without one this publishes into the single run waiting to be written up, and refuses when two are or when none is.",
        "Publish before telling the user, and tell them the same briefing you published.",
      ],
      parameters: Type.Object(
        {
          prose: Type.String({ minLength: 1 }),
          date: Type.Optional(Type.String({ pattern: DATE })),
          profile: Type.Optional(Type.String({ pattern: PROFILE })),
        },
        { additionalProperties: false },
      ),
      async execute(_id, params) {
        return toolResult(
          await runHelper(["publish", ...runArguments(params), "--json"], {
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
        {
          date: Type.Optional(Type.String({ pattern: DATE })),
          profile: Type.Optional(Type.String({ pattern: PROFILE })),
        },
        { additionalProperties: false },
      ),
      async execute(_id, params) {
        return toolResult(
          await runHelper(["open", ...runArguments(params), "--json"]),
        );
      },
    }),
  );

  // The read is started here and not awaited. pi runs the session_start
  // listeners one after another, so awaiting the helper would hold back every
  // extension loaded after this one, the agent channel the surfaces speak
  // through included.
  pi.on("session_start", (_event, ctx) => {
    extensionContext = ctx;
    void reconcile();
  });

  pi.on("session_shutdown", () => {
    extensionContext = undefined;
  });
}

/** The run a tool call names: today unless it says otherwise, and whichever
 * run is waiting unless it names a profile. */
function runArguments(params: { date?: string; profile?: string }): string[] {
  const named = params.profile ? ["--profile", params.profile] : [];
  return ["--date", params.date ?? localDate(new Date()), ...named];
}
