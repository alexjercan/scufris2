import assert from "node:assert/strict";
import test from "node:test";

import {
  APPROVAL_DONE_SUMMARY,
  APPROVAL_INSTRUCTION,
  classifyPreflightResult,
  classifyReviewResponse,
  codeReviewPayload,
  completeApprovedLanding,
  consumeReviewRetry,
  isApprovalDone,
  invalidatePreflight,
  isDelegatedFeature,
  jobEventTriggersTurn,
  nextPreflightFeedbackCycle,
  preflightFindingsMessage,
  reviewEventRejection,
  reviewRetryRejection,
  sameReviewRevisions,
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

test("annotations remain actionable and return to the same worker", () => {
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
    classifyReviewResponse({
      status: "handled",
      result: {
        approved: true,
        feedback: "LGTM - no changes requested.",
        annotations: [],
      },
    }),
    { kind: "approved" },
  );
  assert.deepEqual(
    classifyReviewResponse({
      status: "handled",
      result: {
        approved: false,
        feedback: "LGTM - no changes requested.",
        annotations: [],
      },
    }),
    {
      kind: "feedback",
      message:
        'Plannotator requested changes. Address this exact feedback: {"feedback":"LGTM - no changes requested.","annotations":[]}',
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
  assert.deepEqual(
    classifyReviewResponse({
      status: "handled",
      result: { approved: false, feedback: "x".repeat(17 * 1024) },
    }),
    {
      kind: "blocked",
      reason: "Plannotator feedback exceeds the steering limit",
    },
  );
});

test("preflight classification accepts only consistent fix-worthy results", () => {
  assert.deepEqual(
    classifyPreflightResult({ verdict: "approve", findings: [] }),
    { kind: "approved" },
  );
  const finding = {
    severity: "MAJOR" as const,
    path: "src/run.ts",
    line: 12,
    reason: "Failure loses committed state.",
    change: "Persist state before returning.",
  };
  assert.deepEqual(
    classifyPreflightResult({
      verdict: "request_changes",
      findings: [finding],
    }),
    { kind: "feedback", findings: [finding] },
  );
  assert.equal(
    preflightFindingsMessage([finding]),
    'Independent preflight requested changes: {"findings":[{"severity":"MAJOR","path":"src/run.ts","line":12,"reason":"Failure loses committed state.","change":"Persist state before returning."}]}',
  );
  for (const malformed of [
    { verdict: "approve", findings: [finding] },
    { verdict: "request_changes", findings: [] },
    {
      verdict: "request_changes",
      findings: [{ ...finding, severity: "NIT" }],
    },
    {
      verdict: "request_changes",
      findings: [{ ...finding, path: "../escape" }],
    },
  ]) {
    assert.equal(classifyPreflightResult(malformed).kind, "blocked");
  }
});

test("preflight lifecycle permits two feedback cycles then requires mediation", () => {
  assert.equal(nextPreflightFeedbackCycle(0), 1);
  assert.equal(nextPreflightFeedbackCycle(1), 2);
  assert.equal(nextPreflightFeedbackCycle(2), undefined);
  assert.equal(nextPreflightFeedbackCycle(-1), undefined);
});

test("preflight approval binds Plannotator ordering to exact revisions", () => {
  const snapshot = {
    job_id: "abc123def456",
    worktree: "/trusted/worktree",
    landing_branch: "master",
    landing_sha: "1".repeat(40),
    feature_sha: "2".repeat(40),
    subject: "Change",
  };
  assert.equal(sameReviewRevisions(snapshot, { ...snapshot }), true);
  assert.equal(
    sameReviewRevisions(snapshot, { ...snapshot, feature_sha: "3".repeat(40) }),
    false,
  );
  assert.equal(
    sameReviewRevisions(snapshot, { ...snapshot, landing_sha: "3".repeat(40) }),
    false,
  );
});

test("human feedback invalidates approval and the reviewer session", () => {
  const state = {
    preflightReviewId: "abc123def456",
    preflightFeedbackCycles: 2,
    preflightApproval: { landing_sha: "1".repeat(40) },
  };
  invalidatePreflight(state);
  assert.deepEqual(state, {
    preflightReviewId: undefined,
    preflightFeedbackCycles: 0,
    preflightApproval: undefined,
  });
});

test("review policy enforces landable and non-landable terminal states", () => {
  assert.equal(
    reviewEventRejection({ profile: "none" }, "review-ready", "idle"),
    "non-landable jobs cannot enter review-ready",
  );
  assert.equal(
    reviewEventRejection(
      { profile: "code", brief: "Audience and outcome" },
      "done",
      "idle",
    ),
    "landable jobs require review approval before done",
  );
  assert.equal(
    reviewEventRejection(
      { profile: "code", brief: "Audience and outcome" },
      "done",
      "awaiting-done",
    ),
    undefined,
  );
  assert.equal(
    reviewEventRejection({ profile: "none" }, "done", "idle"),
    undefined,
  );
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

test("landing cleanup defaults call land, stop, then remove", async () => {
  const calls: string[] = [];
  const outcome = await completeApprovedLanding("remove", {
    land: async () => void calls.push("land"),
    stop: async () => void calls.push("stop"),
    remove: async () => void calls.push("remove"),
  });
  assert.deepEqual(calls, ["land", "stop", "remove"]);
  assert.deepEqual(outcome, {
    state: "landed",
    summary: "approved revision landed and resources removed",
  });
});

test("retain lands and stops without resource removal", async () => {
  const calls: string[] = [];
  const outcome = await completeApprovedLanding("retain", {
    land: async () => void calls.push("land"),
    stop: async () => void calls.push("stop"),
    remove: async () => void calls.push("remove"),
  });
  assert.deepEqual(calls, ["land", "stop"]);
  assert.match(outcome.summary, /branch and worktree retained/);
});

test("cleanup failure preserves the successful landing result", async () => {
  const calls: string[] = [];
  const outcome = await completeApprovedLanding("remove", {
    land: async () => void calls.push("land"),
    stop: async () => void calls.push("stop"),
    remove: async () => {
      calls.push("remove");
      throw new Error("resource is busy");
    },
  });
  assert.deepEqual(calls, ["land", "stop", "remove"]);
  assert.equal(outcome.state, "landed-with-retained-resources");
  assert.match(outcome.summary, /landing succeeded/);
  assert.match(outcome.summary, /resource is busy/);
});

test("approval requires the exact done acknowledgment", () => {
  assert.match(APPROVAL_INSTRUCTION, /Do not make repository changes/);
  assert.match(APPROVAL_INSTRUCTION, /append exactly/);
  assert.equal(isApprovalDone("done", APPROVAL_DONE_SUMMARY), true);
  assert.equal(isApprovalDone("done", "approved"), false);
  assert.equal(isApprovalDone("review-ready", APPROVAL_DONE_SUMMARY), false);
});
