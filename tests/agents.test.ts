import assert from "node:assert/strict";
import test from "node:test";
import {
  foregroundCommandWaits,
  parseWorkerEvent,
  PLANNOTATOR_REVIEW_TOOL,
  QUICK_REVIEW_TOOL,
  TERMINAL_OWNERSHIP_STATES,
  workerEventWakes,
} from "../extensions/scufris/workflow/orchestration.ts";
import {
  WORKER_REPORT_EVENTS,
  WORKER_REPORT_TOOL,
  workerReportTerminatesTurn,
} from "../extensions/scufris/workflow/worker-report.ts";

test("worker events use only the replacement protocol", () => {
  assert.deepEqual(parseWorkerEvent("working: checking docs"), {
    type: "working",
    value: "checking docs",
  });
  assert.equal(parseWorkerEvent("ready: implementation-complete"), undefined);
  assert.equal(parseWorkerEvent("needs-decision: choose an API"), undefined);
  assert.deepEqual(parseWorkerEvent("done: report saved"), {
    type: "done",
    value: "report saved",
  });
  assert.deepEqual(parseWorkerEvent("failed: harness exited"), {
    type: "failed",
    value: "harness exited",
  });
});

test("foreground Scufris rejects shell waits", () => {
  assert.equal(foregroundCommandWaits("sleep 30"), true);
  assert.equal(
    foregroundCommandWaits("echo started && /usr/bin/sleep 30"),
    true,
  );
  assert.equal(foregroundCommandWaits("DELAY=30 command sleep $DELAY"), true);
  assert.equal(foregroundCommandWaits("job & wait"), true);
  assert.equal(foregroundCommandWaits("rg -n sleep extensions"), false);
  assert.equal(foregroundCommandWaits("npm test"), false);
});

test("delegated workers use one dedicated reporting tool and state set", () => {
  assert.equal(WORKER_REPORT_TOOL, "scufris_report");
  assert.deepEqual(WORKER_REPORT_EVENTS, ["working", "blocked", "done"]);
  assert.equal(workerReportTerminatesTurn("working"), false);
  assert.equal(workerReportTerminatesTurn("blocked"), true);
  assert.equal(workerReportTerminatesTurn("done"), true);
});

test("Quick Review and Plannotator remain separate tools", () => {
  assert.equal(QUICK_REVIEW_TOOL, "scufris_job_quick_review");
  assert.equal(PLANNOTATOR_REVIEW_TOOL, "scufris_job_plannotator_review");
  assert.notEqual(QUICK_REVIEW_TOOL, PLANNOTATOR_REVIEW_TOOL);
});

test("done remains steerable and only runtime lifecycle states terminate ownership", () => {
  assert.equal(TERMINAL_OWNERSHIP_STATES.has("done"), false);
  assert.equal(TERMINAL_OWNERSHIP_STATES.has("failed"), true);
  assert.equal(TERMINAL_OWNERSHIP_STATES.has("stopped"), true);
  assert.equal(TERMINAL_OWNERSHIP_STATES.has("landed"), true);
});

test("all actionable and terminal events wake foreground Scufris", () => {
  assert.equal(workerEventWakes("working"), false);
  for (const type of ["blocked", "done", "failed"] as const) {
    assert.equal(workerEventWakes(type), true);
  }
});
