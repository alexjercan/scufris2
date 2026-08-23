import assert from "node:assert/strict";
import test from "node:test";
import {
  ACKNOWLEDGED_ACTION_TOOLS,
  applySteerResult,
  deliveredWorkerEventIds,
  deliverRuntimeFailure,
  deliverWorkerEvent,
  FINAL_RESPONSE_TOOL,
  foregroundActionPolicy,
  ForegroundAcknowledgmentGate,
  foregroundCommandWaits,
  parseWorkerEvent,
  PLANNOTATOR_REVIEW_TOOL,
  QUICK_REVIEW_TOOL,
  resolveWakeCommand,
  TERMINAL_OWNERSHIP_STATES,
  toolBatchAllowsAction,
  wakeModeFromEntries,
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

test("foreground action policy requires one natural final acknowledgment", () => {
  assert.match(foregroundActionPolicy, /only permitted follow-up/);
  assert.match(foregroundActionPolicy, /Do not use a canned acknowledgment/);
  assert.deepEqual([...ACKNOWLEDGED_ACTION_TOOLS].sort(), [
    "scufris_job_land",
    "scufris_job_plannotator_review",
    "scufris_job_quick_review",
    "scufris_job_send",
    "scufris_job_spawn",
    "scufris_job_stop",
  ]);
  for (const action of ACKNOWLEDGED_ACTION_TOOLS) {
    assert.equal(toolBatchAllowsAction(action, [action]), true);
    assert.equal(toolBatchAllowsAction(action, [action, "read"]), false);
  }
  assert.equal(
    toolBatchAllowsAction(FINAL_RESPONSE_TOOL, [FINAL_RESPONSE_TOOL]),
    true,
  );
  assert.equal(
    toolBatchAllowsAction(FINAL_RESPONSE_TOOL, ["read", FINAL_RESPONSE_TOOL]),
    false,
  );

  const gate = new ForegroundAcknowledgmentGate();
  for (const action of ["scufris_job_spawn", "scufris_job_send"]) {
    gate.markSuccessfulAction(action);
    assert.match(gate.blockReason("read") ?? "", /only permitted follow-up/);
    assert.equal(gate.blockReason(FINAL_RESPONSE_TOOL), undefined);
    gate.completeFinalResponse(true);
    assert.match(gate.blockReason("bash") ?? "", /only permitted follow-up/);
    gate.completeFinalResponse(false);
    assert.equal(gate.blockReason("read"), undefined);
  }
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

test("done closes execution while its durable logical job remains steerable", () => {
  assert.equal(TERMINAL_OWNERSHIP_STATES.has("done"), true);
  assert.equal(TERMINAL_OWNERSHIP_STATES.has("failed"), true);
  assert.equal(TERMINAL_OWNERSHIP_STATES.has("stopped"), true);
  assert.equal(TERMINAL_OWNERSHIP_STATES.has("landed"), true);
});

test("generation restarts restore status watching including Quick Review corrections", () => {
  const job = {
    state: "done",
    summary: "old generation complete",
    generation: 1,
    status_file: "/old/status",
    window_alive: false,
  };
  let watches = 0;
  applySteerResult(
    job,
    { generation: 2, status_file: "/new/status", restarted: true },
    () => {
      watches += 1;
    },
  );
  assert.deepEqual(job, {
    state: "working",
    summary: "foreground guidance submitted",
    generation: 2,
    status_file: "/new/status",
    window_alive: true,
  });
  assert.equal(watches, 1);
});

test("persisted worker event messages provide restart deduplication", () => {
  assert.deepEqual(
    [
      ...deliveredWorkerEventIds([
        {
          type: "message",
          message: {
            customType: "scufris-job-event",
            details: { event_id: "job:0:10:digest" },
          },
        },
        {
          type: "custom_message",
          customType: "scufris-job-event",
          details: { event_id: "job:10:20:second" },
        },
        {
          type: "message",
          message: {
            customType: "other",
            details: { event_id: "ignored" },
          },
        },
      ]),
    ],
    ["job:0:10:digest", "job:10:20:second"],
  );
});

test("minimal and all wake modes are deterministic", () => {
  assert.equal(workerEventWakes("working", "minimal"), false);
  assert.equal(workerEventWakes("working", "all"), true);
  for (const mode of ["minimal", "all"] as const) {
    for (const type of ["blocked", "done", "failed"] as const) {
      assert.equal(workerEventWakes(type, mode), true);
    }
  }
});

test("wake commands report state, change explicitly, and reject unknown values", () => {
  assert.deepEqual(resolveWakeCommand("", "minimal"), {
    mode: "minimal",
    changed: false,
    notice: "Wake mode minimal.",
    warning: false,
  });
  assert.equal(resolveWakeCommand("minimal", "minimal").changed, false);
  assert.deepEqual(resolveWakeCommand("ALL", "minimal"), {
    mode: "all",
    changed: true,
    notice: "Wake mode all.",
    warning: false,
  });
  assert.deepEqual(resolveWakeCommand("off", "all"), {
    mode: "all",
    changed: false,
    notice: "Use /wake minimal or all.",
    warning: true,
  });
});

test("wake mode restores the latest valid session entry", () => {
  assert.equal(wakeModeFromEntries([]), "minimal");
  assert.equal(
    wakeModeFromEntries([
      {
        type: "custom",
        customType: "scufris-wake-state-v1",
        data: { version: 1, mode: "all" },
      },
      {
        type: "custom",
        customType: "scufris-wake-state-v1",
        data: { version: 1, mode: "invalid" },
      },
    ]),
    "all",
  );
});

test("orchestration delivers exact worker wakes and quiet progress by mode", () => {
  const job = {
    job_id: "abcdef123456",
    project: "personal/scufris2",
    context_id: "1".repeat(24),
  };
  const ordinaryEvents = [
    { type: "working", value: "implementation underway" },
    { type: "blocked", value: "needs mediation" },
    { type: "done", value: "implementation complete" },
  ] as const;

  for (const mode of ["minimal", "all"] as const) {
    const messages: Array<{ message: any; options: any }> = [];
    const notices: Array<{ message: string; type: string }> = [];
    const pi = {
      sendMessage(message: any, options: any) {
        messages.push({ message, options });
      },
    };
    const context = {
      hasUI: true,
      ui: {
        notify(message: string, type: string) {
          notices.push({ message, type });
        },
      },
    };

    for (const event of ordinaryEvents)
      deliverWorkerEvent(pi as never, context as never, job, event, mode);
    deliverRuntimeFailure(
      pi as never,
      context as never,
      job,
      "worker harness exited unexpectedly",
      mode,
    );

    const expectedTypes =
      mode === "minimal"
        ? ["blocked", "done", "failed"]
        : ["working", "blocked", "done", "failed"];
    assert.deepEqual(
      messages.map(({ message }) => message.details.event.split(":", 1)[0]),
      expectedTypes,
    );
    for (const { message, options } of messages) {
      assert.deepEqual(options, {
        deliverAs: "followUp",
        triggerTurn: true,
      });
      assert.equal(message.customType, "scufris-job-event");
      assert.equal(message.display, true);
      assert.equal(message.details.job_id, job.job_id);
      assert.equal(message.details.project, job.project);
      assert.equal(message.details.context_id, job.context_id);
      assert.match(message.content, new RegExp(message.details.event));
      assert.match(message.content, /call scufris_final_response/);
    }
    assert.deepEqual(
      notices,
      mode === "minimal"
        ? [
            {
              message: "abcdef123456: implementation underway",
              type: "info",
            },
            {
              message: "Job abcdef123456: worker harness exited unexpectedly",
              type: "error",
            },
          ]
        : [
            {
              message: "Job abcdef123456: worker harness exited unexpectedly",
              type: "error",
            },
          ],
    );
  }
});
