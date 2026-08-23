import assert from "node:assert/strict";
import test from "node:test";
import {
  parseWorkerEvent,
  PLANNOTATOR_REVIEW_TOOL,
  QUICK_REVIEW_TOOL,
  workerEventWakes,
} from "../extensions/scufris/workflow/orchestration.ts";
import { WORKER_REPORT_TOOL } from "../extensions/scufris/workflow/worker-report.ts";

test("worker events use only the replacement protocol", () => {
  assert.deepEqual(parseWorkerEvent("working: checking docs"), {
    type: "working",
    value: "checking docs",
  });
  assert.deepEqual(parseWorkerEvent("ready: implementation-complete"), {
    type: "ready",
    value: "implementation-complete",
  });
  assert.equal(parseWorkerEvent("ready: Review now"), undefined);
  assert.equal(parseWorkerEvent("ready: review: now"), undefined);
  assert.deepEqual(parseWorkerEvent("done: report saved"), {
    type: "done",
    value: "report saved",
  });
});

test("delegated workers use one dedicated reporting tool", () => {
  assert.equal(WORKER_REPORT_TOOL, "scufris_report");
});

test("Quick Review and Plannotator remain separate tools", () => {
  assert.equal(QUICK_REVIEW_TOOL, "scufris_job_quick_review");
  assert.equal(PLANNOTATOR_REVIEW_TOOL, "scufris_job_plannotator_review");
  assert.notEqual(QUICK_REVIEW_TOOL, PLANNOTATOR_REVIEW_TOOL);
});

test("all actionable and terminal events wake foreground Scufris", () => {
  assert.equal(workerEventWakes("working"), false);
  for (const type of [
    "needs-decision",
    "blocked",
    "ready",
    "done",
    "failed",
  ] as const) {
    assert.equal(workerEventWakes(type), true);
  }
});
