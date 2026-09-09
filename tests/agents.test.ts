import assert from "node:assert/strict";
import test from "node:test";
import {
  ACKNOWLEDGED_ACTION_TOOLS,
  applySteerResult,
  boundedRows,
  deliveredWorkerEventIds,
  deliverRuntimeFailure,
  deliverWorkerEvent,
  EVENT_DRAIN_ROW,
  FINAL_RESPONSE_TOOL,
  foregroundActionPolicy,
  ForegroundAcknowledgmentGate,
  foregroundCommandWaits,
  JOB_OBSERVATION_TOOLS,
  jobRow,
  literalDelegationPolicy,
  parseWorkerEvent,
  PLANNOTATOR_REVIEW_TOOL,
  publishedRows,
  QUICK_REVIEW_TOOL,
  resolveWakeCommand,
  TERMINAL_OWNERSHIP_STATES,
  toolBatchAllowsAction,
  wakeModeFromEntries,
  workerEventWakes,
} from "../agent/extensions/scufris/workflow/orchestration.ts";
import { JOB_CITATION_EVENT } from "../agent/extensions/scufris/shared/citations.ts";
import { receiptBadges } from "../agent/extensions/scufris/workflow/citation.ts";
import { MAX_JOB_ROWS } from "../agent/extensions/scufris/service/protocol.ts";
import {
  WORKER_REPORT_EVENTS,
  WORKER_REPORT_TOOL,
  workerReportTerminatesTurn,
} from "../agent/extensions/scufris/workflow/worker-report.ts";

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

test("foreground action policy starts everything and watches nothing", () => {
  assert.match(foregroundActionPolicy, /Start everything the request asks for/);
  // The failure this sentence exists for: Alex asked for a job and then said
  // "open the brief" while it was starting, and got "I still need to open
  // today's brief in a separate action" instead of the brief.
  assert.match(foregroundActionPolicy, /a request dropped/);
  assert.match(foregroundActionPolicy, /Never watch what you started/);
  assert.match(foregroundActionPolicy, /Do not use a canned acknowledgment/);
  assert.deepEqual([...JOB_OBSERVATION_TOOLS], ["scufris_job_inspect"]);
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
    gate.markSuccessfulAction(action, "a1b2c3d4e5f6");
    // Watching the job it just started is the whole of what is refused.
    assert.match(
      gate.blockReason("scufris_job_inspect", "a1b2c3d4e5f6") ?? "",
      /do not watch it/,
    );
    assert.match(gate.blockReason("scufris_job_inspect") ?? "", /do not watch/);
    // A different job is a different question. "Steer this one and tell me what
    // the reviewer on that one found" is one request, not a poll.
    assert.equal(
      gate.blockReason("scufris_job_inspect", "9f8e7d6c5b4a"),
      undefined,
    );
    // The fleet list reads memory and cannot wait, so it was never the risk.
    assert.equal(gate.blockReason("scufris_job_list"), undefined);
    // Everything else is work the request may have asked for, including a
    // second job and an instruction that arrived while this one was starting.
    assert.equal(gate.blockReason("scufris_job_spawn"), undefined);
    assert.equal(gate.blockReason("scufris_briefing_open"), undefined);
    assert.equal(gate.blockReason("read"), undefined);
    assert.equal(gate.blockReason("bash"), undefined);
    assert.equal(gate.blockReason(FINAL_RESPONSE_TOOL), undefined);
    // A refused acknowledgment leaves the job unwatched; only a good one clears.
    gate.completeFinalResponse(true);
    assert.match(gate.blockReason("scufris_job_inspect") ?? "", /do not watch/);
    gate.completeFinalResponse(false);
    assert.equal(gate.blockReason("scufris_job_inspect"), undefined);
  }
});

test("delegation policy runs the named agents and nothing else", () => {
  assert.match(literalDelegationPolicy, /menu of agent types, not a workflow/);
  assert.match(
    literalDelegationPolicy,
    /start no\nagent because the project declares it/,
  );
  assert.match(
    literalDelegationPolicy,
    /"Implement X" is the work agent alone\./,
  );
  assert.match(
    literalDelegationPolicy,
    /"Implement X and review it" is the work agent and then the review agent\./,
  );
  // An unfamiliar agent name is still delegated to by name.
  assert.match(
    literalDelegationPolicy,
    /name you have never seen is still delegated to/,
  );
  // The request outranks the inferred conventions.
  assert.match(
    literalDelegationPolicy,
    /an explicit instruction in the request\nwins over any of them/,
  );
  // A later round steers the owned job, so a reviewer keeps what it accepted.
  assert.match(
    literalDelegationPolicy,
    /A later round of an agent you already own for this work is scufris_job_send,\nnot a second spawn\./,
  );
  assert.match(
    literalDelegationPolicy,
    /a fresh job keeps nothing and re-derives its findings/,
  );
  assert.match(literalDelegationPolicy, /One request is one job\./);
  assert.match(
    literalDelegationPolicy,
    /Never queue follow-on work of\nyour own\./,
  );
  assert.doesNotMatch(literalDelegationPolicy, /Follow the returned project/);
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

test("standalone Quick Review and Plannotator remain separate tools", () => {
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

test("generation restarts restore status watching", () => {
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

test("a job is drawn as one row, and a row says what state it is in", () => {
  const job = {
    job_id: "abcdef123456",
    project: "personal/scufris2",
    summary: "needs mediation",
    created_at: "2026-09-08T21:00:00Z",
  };
  assert.deepEqual(jobRow({ ...job, state: "blocked" }), {
    id: "abcdef123456",
    project: "personal/scufris2",
    state: "blocked",
    since: 1_788_901_200,
    summary: "needs mediation",
  });
  // A workflow whose cleanup did not finish needs Alex, so it is not done.
  assert.equal(jobRow({ ...job, state: "suspended" }).state, "failed");
  assert.equal(jobRow({ ...job, state: "landed" }).state, "done");
  assert.equal(jobRow({ ...job, state: "stopped" }).state, "done");
  // A project is optional, and a row without one is still a row.
  assert.equal(
    jobRow({ ...job, project: null, state: "working" }).project,
    undefined,
  );
});

test("live rows never drop and the oldest finished rows fall off first", () => {
  const row = (id: string, since: number, state: "working" | "done") => ({
    id,
    state,
    since,
    summary: "",
  });
  const listed = [
    ...Array.from({ length: 6 }, (_value, index) =>
      row(`d${index}`, 100 + index, "done"),
    ),
    ...Array.from({ length: 5 }, (_value, index) =>
      row(`w${index}`, 200 + index, "working"),
    ),
  ];
  const kept = boundedRows(listed);
  assert.equal(kept.length, MAX_JOB_ROWS);
  // Oldest first, because that is the order they were started in.
  assert.deepEqual(
    kept.map((held) => held.id),
    ["d3", "d4", "d5", "w0", "w1", "w2", "w3", "w4"],
  );
  // Past the cap in live rows alone the newest win: the job just started is
  // never the one that vanishes.
  const live = Array.from({ length: MAX_JOB_ROWS + 2 }, (_value, index) =>
    row(`w${index}`, 300 + index, "working"),
  );
  assert.deepEqual(
    boundedRows(live).map((held) => held.id),
    ["w2", "w3", "w4", "w5", "w6", "w7", "w8", "w9"],
  );
});

test("a row outlives its job, and filing it is what clears it", () => {
  const job = (id: string, state: string) => ({
    job_id: id,
    project: "personal/scufris2",
    state,
    summary: `job ${id}`,
    created_at: "2026-09-08T21:00:00Z",
  });
  const live = [job("abcdef123456", "working"), job("123456abcdef", "working")];
  const filed = new Set<string>();
  assert.deepEqual(
    publishedRows(live, filed).map((row) => [row.id, row.state]),
    [
      ["123456abcdef", "working"],
      ["abcdef123456", "working"],
    ],
  );
  // Finishing is not filing. Both rows stay, and say what became of them.
  const done = [job("abcdef123456", "landed"), job("123456abcdef", "failed")];
  assert.deepEqual(
    publishedRows(done, filed).map((row) => [row.id, row.state]),
    [
      ["123456abcdef", "failed"],
      ["abcdef123456", "done"],
    ],
  );
  for (const row of done) filed.add(row.job_id);
  assert.deepEqual(publishedRows(done, filed), []);
});

test("a drain that stopped is a row, because nothing else reports it", () => {
  const drain = { since: 1_788_901_200, error: "permission denied" };
  const [row] = publishedRows([], new Set(), drain);
  assert.deepEqual(row, {
    id: EVENT_DRAIN_ROW,
    state: "failed",
    since: 1_788_901_200,
    summary: "Scufris cannot read worker events: permission denied",
  });
  // Filing it is the acknowledgement, exactly as it is for a job.
  assert.deepEqual(publishedRows([], new Set([EVENT_DRAIN_ROW]), drain), []);
});

test("no model writes a badge: they are read off the measured receipt", () => {
  const receipt = {
    job_id: "abcdef123456",
    measured_at: "2026-09-08T21:00:00Z",
    trigger: "done",
    commit: "75919d8",
    facts: { landed: true, pushed: false, dirty: true },
    claims: [],
    sentences: [],
    unavailable: {},
  };
  assert.deepEqual(receiptBadges(receipt), [
    { label: "landed", value: "yes", state: "measured" },
    { label: "pushed", value: "no", state: "refuted" },
    { label: "worktree", value: "dirty", state: "refuted" },
  ]);
  // `unknown` is not a no. A fetch that failed leaves the fact unmeasured,
  // and drawing that as "not pushed" invents what nobody measured.
  assert.deepEqual(
    receiptBadges({
      ...receipt,
      facts: { landed: true, pushed: null, dirty: false },
      unavailable: { pushed: "the remote could not be reached" },
    }),
    [
      { label: "landed", value: "yes", state: "measured" },
      {
        label: "pushed",
        value: "the remote could not be reached",
        state: "unknown",
      },
    ],
  );
  // A claim replaces the standing badge for its own field, because it carries
  // both the measurement and what the worker said about it.
  assert.deepEqual(
    receiptBadges({
      ...receipt,
      facts: { landed: true, pushed: true, dirty: false, release_run: null },
      claims: [
        {
          claim: "released",
          said: "released v2.4.0",
          field: "release_run",
          measured: null,
          reason: null,
          verdict: "claimed, not verified",
        },
        {
          claim: "pushed",
          said: "pushed master",
          field: "pushed",
          measured: true,
          reason: null,
          verdict: "verified",
        },
      ],
    }),
    [
      { label: "landed", value: "yes", state: "measured" },
      { label: "pushed", value: "yes", state: "measured" },
      { label: "released", value: "claimed, not verified", state: "claimed" },
    ],
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
    const signals: Array<{ event: string; value: unknown }> = [];
    const pi = {
      events: {
        emit(event: string, value: unknown) {
          signals.push({ event, value });
        },
      },
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

    // Badges are measured, so they are published whether or not the event
    // wakes a turn. An event with no receipt publishes nothing at all.
    assert.deepEqual(signals, []);
    deliverWorkerEvent(
      pi as never,
      context as never,
      job,
      { type: "done", value: "implementation complete" },
      mode,
      {
        job_id: job.job_id,
        measured_at: "2026-09-08T21:00:00Z",
        trigger: "done",
        commit: "75919d8",
        facts: { landed: false },
        claims: [],
        sentences: ["not landed"],
        unavailable: {},
      },
    );
    assert.deepEqual(signals, [
      {
        event: JOB_CITATION_EVENT,
        value: {
          job_id: job.job_id,
          badges: [{ label: "landed", value: "no", state: "refuted" }],
        },
      },
    ]);
    const expectedTypes =
      mode === "minimal"
        ? ["blocked", "done", "failed", "done"]
        : ["working", "blocked", "done", "failed", "done"];
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
