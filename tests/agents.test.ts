import assert from "node:assert/strict";
import test from "node:test";

import { jobEventTriggersTurn } from "../extensions/scufris/agents.ts";

test("review-ready wakes the orchestrator while working stays routine", () => {
  assert.equal(jobEventTriggersTurn("working"), false);
  assert.equal(jobEventTriggersTurn("review-ready"), true);
  assert.equal(jobEventTriggersTurn("needs-decision"), true);
  assert.equal(jobEventTriggersTurn("blocked"), true);
  assert.equal(jobEventTriggersTurn("done"), true);
  assert.equal(jobEventTriggersTurn("failed"), true);
});
