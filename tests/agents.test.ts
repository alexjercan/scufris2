import assert from "node:assert/strict";
import test from "node:test";

import {
  APPROVAL_DONE_SUMMARY,
  APPROVAL_INSTRUCTION,
  classifyReviewResponse,
  codeReviewPayload,
  consumeReviewRetry,
  isApprovalDone,
  isDelegatedFeature,
  jobEventTriggersTurn,
  reviewRetryRejection,
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

test("review request uses the public structured since-base contract", () => {
  assert.deepEqual(
    codeReviewPayload({
      job_id: "abc123def456",
      worktree: "/trusted/worktree",
      landing_branch: "master",
      landing_sha: "1".repeat(40),
      feature_sha: "2".repeat(40),
      subject: "Change",
    }),
    {
      cwd: "/trusted/worktree",
      defaultBranch: "master",
      diffType: "since-base",
    },
  );
});

test("structured feedback takes precedence and returns to the same worker", () => {
  assert.deepEqual(
    classifyReviewResponse({
      status: "handled",
      result: {
        approved: true,
        feedback: "Fix the race.\nAdd a regression test.",
        annotations: [{ path: "src/a.ts", line: 3, comment: "Guard this" }],
      },
    }),
    {
      kind: "feedback",
      message:
        'Plannotator requested changes. Address this exact feedback: {"feedback":"Fix the race.\\nAdd a regression test.","annotations":[{"path":"src/a.ts","line":3,"comment":"Guard this"}]}',
    },
  );
  assert.deepEqual(
    classifyReviewResponse({ status: "handled", result: { approved: false } }),
    {
      kind: "blocked",
      reason: "Plannotator closed without approval or actionable feedback",
    },
  );
  assert.deepEqual(classifyReviewResponse({ status: "unavailable" }), {
    kind: "blocked",
    reason: "Plannotator review was unavailable",
  });
});

test("review retry requires and consumes the exact blocked lifecycle", () => {
  const retryable = {
    state: "blocked",
    summary: "review precondition failed: main checkout has tracked changes",
    reviewPhase: "idle" as const,
    reviewRetryable: true,
    reviewRequestId: "stale-request",
  };
  assert.equal(consumeReviewRetry(retryable), undefined);
  assert.deepEqual(retryable, {
    state: "review-ready",
    summary: "retrying fresh review preconditions",
    reviewPhase: "idle",
    reviewRetryable: false,
    reviewRequestId: undefined,
    approval: undefined,
  });
  assert.equal(
    reviewRetryRejection({
      state: "blocked",
      reviewPhase: "idle",
      reviewRetryable: true,
      hasApproval: false,
    }),
    undefined,
  );
  assert.equal(
    reviewRetryRejection({
      state: "working",
      reviewPhase: "idle",
      reviewRetryable: true,
      hasApproval: false,
    }),
    "job is not lifecycle blocked",
  );
  assert.equal(
    reviewRetryRejection({
      state: "blocked",
      reviewPhase: "reviewing",
      reviewRetryable: true,
      hasApproval: false,
    }),
    "review retry is invalid during reviewing",
  );
  assert.equal(
    reviewRetryRejection({
      state: "blocked",
      reviewPhase: "idle",
      reviewRetryable: false,
      hasApproval: false,
    }),
    "job is not blocked by a retryable review precondition",
  );
  assert.equal(
    reviewRetryRejection({
      state: "blocked",
      reviewPhase: "idle",
      reviewRetryable: true,
      hasApproval: true,
    }),
    "review retry cannot reuse an approval",
  );
});

test("approval requires the exact done acknowledgment", () => {
  assert.match(APPROVAL_INSTRUCTION, /Do not make repository changes/);
  assert.match(APPROVAL_INSTRUCTION, /append exactly/);
  assert.equal(isApprovalDone("done", APPROVAL_DONE_SUMMARY), true);
  assert.equal(isApprovalDone("done", "approved"), false);
  assert.equal(isApprovalDone("review-ready", APPROVAL_DONE_SUMMARY), false);
});
