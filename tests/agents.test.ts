import assert from "node:assert/strict";
import test from "node:test";

import {
  isDelegatedFeature,
  jobEventTriggersTurn,
} from "../extensions/scufris/agents.ts";

test("delegated feature validation accepts only bounded lowercase slugs", () => {
  for (const feature of ["fix-login-timeout", "protocol-v2", "a".repeat(48)]) {
    assert.equal(isDelegatedFeature(feature), true, feature);
  }
  for (const feature of [
    "Fix-login",
    "fix_login",
    "-fix-login",
    "fix--login",
    "fix-login-",
    "fix login",
    "fix-login\n",
    "a".repeat(49),
    "",
  ]) {
    assert.equal(isDelegatedFeature(feature), false, JSON.stringify(feature));
  }
});

test("review-ready wakes the orchestrator while working stays routine", () => {
  assert.equal(jobEventTriggersTurn("working"), false);
  assert.equal(jobEventTriggersTurn("review-ready"), true);
  assert.equal(jobEventTriggersTurn("needs-decision"), true);
  assert.equal(jobEventTriggersTurn("blocked"), true);
  assert.equal(jobEventTriggersTurn("done"), true);
  assert.equal(jobEventTriggersTurn("failed"), true);
});
