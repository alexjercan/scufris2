import { fileURLToPath } from "node:url";
import { StringEnum, Type } from "@earendil-works/pi-ai";
import { defineTool, type ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { runPrivateHelper, toolResult } from "../shared/runtime.ts";

const jobsHelperPath = fileURLToPath(
  new URL("../../../tools/jobs/scufris-jobs", import.meta.url),
);
const JOB_ID = /^[a-f0-9]{12}$/;

export const WORKER_REPORT_TOOL = "scufris_report";
export const WORKER_REPORT_EVENTS = ["working", "blocked", "done"] as const;
export type WorkerReportEvent = (typeof WORKER_REPORT_EVENTS)[number];

export function workerReportTerminatesTurn(event: WorkerReportEvent): boolean {
  return event !== "working";
}

export default function workerReport(pi: ExtensionAPI): void {
  if (process.env.SCUFRIS_ROLE !== "worker") return;
  const jobId = process.env.SCUFRIS_JOB_ID;
  const capability = process.env.SCUFRIS_REPORT_CAPABILITY;
  if (
    !jobId ||
    !JOB_ID.test(jobId) ||
    !capability ||
    !/^[a-f0-9]{64}$/.test(capability)
  )
    return;

  pi.registerTool(
    defineTool({
      name: WORKER_REPORT_TOOL,
      label: "Report to Scufris",
      description:
        "Append one status-linked report entry and then working, blocked, or done for foreground Scufris. Harness failures are reported only by trusted orchestration.",
      executionMode: "sequential",
      parameters: Type.Object(
        {
          event: StringEnum(WORKER_REPORT_EVENTS),
          summary: Type.String({
            description: "One-line status summary.",
            minLength: 1,
            maxLength: 500,
            pattern: "^[^\\r\\n]+$",
          }),
          report: Type.String({
            description:
              "Detailed Markdown evidence linked to this status event.",
            minLength: 1,
            maxLength: 524288,
          }),
        },
        { additionalProperties: false },
      ),
      async execute(_id, params, signal) {
        const envelope = await runPrivateHelper<{
          job_id: string;
          event: string;
          summary: string;
        }>(
          jobsHelperPath,
          "report",
          {
            job_id: jobId,
            capability,
            ...params,
          },
          signal,
        );
        if (!envelope.ok || envelope.result === undefined)
          throw new Error(envelope.error ?? "Could not report to Scufris");
        return {
          ...toolResult(envelope.result),
          ...(workerReportTerminatesTurn(params.event)
            ? { terminate: true }
            : {}),
        };
      },
    }),
  );
}
