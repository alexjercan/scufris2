import { fileURLToPath } from "node:url";
import { StringEnum, Type } from "@earendil-works/pi-ai";
import { defineTool, type ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { runPrivateHelper, toolResult } from "../shared/runtime.ts";

const jobsHelperPath = fileURLToPath(
  new URL("../../../tools/jobs/scufris-jobs", import.meta.url),
);
const JOB_ID = /^[a-f0-9]{12}$/;

export const WORKER_REPORT_TOOL = "scufris_report";

export default function workerReport(pi: ExtensionAPI): void {
  if (process.env.SCUFRIS_ROLE !== "worker") return;
  const jobId = process.env.SCUFRIS_JOB_ID;
  if (!jobId || !JOB_ID.test(jobId)) return;

  pi.registerTool(
    defineTool({
      name: WORKER_REPORT_TOOL,
      label: "Report to Scufris",
      description:
        "Replace this delegated job's detailed report and append one validated status event for foreground Scufris.",
      executionMode: "sequential",
      parameters: Type.Object(
        {
          event: StringEnum([
            "working",
            "needs-decision",
            "blocked",
            "ready",
            "done",
            "failed",
          ] as const),
          summary: Type.String({
            description:
              "One-line summary. For ready, use a lowercase milestone slug.",
            minLength: 1,
            maxLength: 500,
            pattern: "^[^\\r\\n]+$",
          }),
          report: Type.String({
            description:
              "Current detailed Markdown evidence for foreground Scufris.",
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
        }>(jobsHelperPath, "report", { job_id: jobId, ...params }, signal);
        if (!envelope.ok || envelope.result === undefined)
          throw new Error(envelope.error ?? "Could not report to Scufris");
        return toolResult(envelope.result);
      },
    }),
  );
}
