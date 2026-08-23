import assert from "node:assert/strict";
import test from "node:test";
import {
  parseWorkerEvent,
  workerEventWakes,
} from "../extensions/scufris/agents.ts";

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
