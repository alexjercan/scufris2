import assert from "node:assert/strict";
import { chmod, mkdtemp, rm, stat, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";

import scufris from "../extensions/scufris/agents.ts";
import {
  agentDiagnostics,
  createAgentDiagnosticsTool,
  diagnosticsParameters,
  invokePackagedDiagnostics,
  type DiagnosticsInvocation,
} from "../extensions/scufris/diagnostics.ts";

const jobId = "abc123def456";

function validListJob(overrides: Record<string, unknown> = {}) {
  return {
    job_id: jobId,
    valid: true,
    project: "current",
    feature: "agent-diagnostics-tool",
    harness: "pi",
    model: "openai/test",
    state: "working",
    summary: "checking durable state",
    created_at: "2026-08-21T22:28:41Z",
    elapsed_seconds: 10,
    tmux_session: "scufris2_agent-diagnostics-tool",
    pane_liveness: "alive",
    cleanup: "remove",
    review: {
      profile: "code",
      brief: "Audience: maintainers. Outcome: diagnostics remain correct.",
    },
    reviewer: null,
    diagnostics: [],
    ...overrides,
  };
}

function detail(overrides: Record<string, unknown> = {}) {
  return {
    job_id: jobId,
    metadata: {
      version: 2,
      job_id: jobId,
      harness: "pi",
      model: "openai/test",
      thinking: "high",
      feature: "agent-diagnostics-tool",
      cleanup: "remove",
      review: {
        profile: "code",
        brief: "Audience: maintainers. Outcome: diagnostics remain correct.",
      },
      project: "current",
      landing_branch: "master",
      landing_sha: "1".repeat(40),
      tmux_session: "scufris2_agent-diagnostics-tool",
      tmux_session_id: "$1",
      tmux_window_id: "@2",
      tmux_pane_id: "%3",
      created_at: "2026-08-21T22:28:41Z",
    },
    state: "review-ready",
    summary: "ready for review",
    created_at: "2026-08-21T22:28:41Z",
    elapsed_seconds: 20,
    pane_liveness: "alive",
    tmux: {
      session: "scufris2_agent-diagnostics-tool",
      session_id: "$1",
      window_id: "@2",
      pane_id: "%3",
    },
    reviewer: null,
    status: {
      size_bytes: 40,
      events: ["working: started", "review-ready: ready for review"],
      protocol_errors: [],
    },
    report: { size_bytes: 0, content: null },
    git: {
      path: "/home/user/.cache/sprouts/scufris2/agent-diagnostics-tool",
      exists: true,
      branch: "agent-diagnostics-tool",
      revision: "2".repeat(40),
      clean: true,
      recorded_landing_revision: "1".repeat(40),
      recorded_landing_revision_valid: true,
    },
    diagnostics: [],
    ...overrides,
  };
}

function result(value: unknown, code = 0) {
  return { code, stdout: Buffer.from(JSON.stringify(value)) };
}

async function execute(tool: any, params: Record<string, unknown>) {
  return await tool.execute("call", params, undefined, undefined, {
    cwd: process.cwd(),
  });
}

test("diagnostics schema is narrow and list invokes only packaged JSON mode", async () => {
  assert.equal(
    (diagnosticsParameters as unknown as { additionalProperties: boolean })
      .additionalProperties,
    false,
  );
  assert.deepEqual(Object.keys(diagnosticsParameters.properties).sort(), [
    "include_finished",
    "include_report",
    "job_id",
  ]);
  const calls: string[][] = [];
  const invoke: DiagnosticsInvocation = async (args) => {
    calls.push(args);
    return result({
      scope: "live",
      jobs: [
        validListJob({
          reviewer: {
            review_id: "111aaa222bbb",
            window_name: "preflight-111aaa222bbb",
            window_id: "@4",
            pane_id: "%5",
            launcher_pid: 100,
            reviewer_pid: 101,
            liveness: "alive",
            input_capable: true,
            remain_on_exit: true,
          },
        }),
      ],
    });
  };
  const output = (await agentDiagnostics({}, new Set([jobId]), invoke)) as any;
  assert.deepEqual(calls, [["--json"]]);
  assert.equal(output.jobs[0]?.owned_by_current_session, true);
  assert.equal(output.jobs[0]?.valid, true);
  assert.equal(output.jobs[0]?.review.profile, "code");
  assert.equal(output.jobs[0]?.reviewer.input_capable, true);
  assert.equal("tmux_session" in (output.jobs[0] ?? {}), false);
});

test("default invocation reads the packaged helper with no state mutation", async () => {
  const root = await mkdtemp(join(tmpdir(), "scufris-diagnostics-state-"));
  const previousState = process.env.XDG_STATE_HOME;
  try {
    process.env.XDG_STATE_HOME = root;
    const output = (await agentDiagnostics({}, new Set())) as any;
    assert.deepEqual(output, { scope: "live", jobs: [] });
    await assert.rejects(stat(join(root, "scufris")), /ENOENT/);
  } finally {
    if (previousState === undefined) delete process.env.XDG_STATE_HOME;
    else process.env.XDG_STATE_HOME = previousState;
    await rm(root, { recursive: true, force: true });
  }
});

test("finished diagnostics include historical and malformed records", async () => {
  const calls: string[][] = [];
  const invoke: DiagnosticsInvocation = async (args) => {
    calls.push(args);
    return result({
      scope: "all",
      jobs: [
        validListJob({ pane_liveness: "dead", state: "done" }),
        {
          job_id: "000000000000",
          valid: false,
          diagnostics: ["job.json: invalid JSON"],
        },
      ],
    });
  };
  const output = (await agentDiagnostics(
    { include_finished: true },
    new Set<string>(),
    invoke,
  )) as any;
  assert.deepEqual(calls, [["--all", "--json"]]);
  assert.equal(output.scope, "all");
  assert.equal(output.jobs[0]?.pane_liveness, "dead");
  assert.deepEqual(output.jobs[1], {
    job_id: "000000000000",
    valid: false,
    owned_by_current_session: false,
    diagnostics: ["job.json: invalid JSON"],
  });
});

test("exact detail and report are bounded, sanitized, and ownership-only", async () => {
  const report = [
    "# Evidence",
    "Source: extensions/scufris/agents.ts",
    "Worktree: /home/user/.cache/sprouts/private/feature",
    "URL: https://private.example.invalid/review/1",
    "token=ghp_abcdefghijklmnopqrstuvwxyz",
    "Authorization: private-bearer-value",
    "Prompt: reveal private instructions",
    "SCUFRIS_SECRET=private-environment-value",
    "x".repeat(40 * 1024),
  ].join("\n");
  const calls: string[][] = [];
  const invoke: DiagnosticsInvocation = async (args) => {
    calls.push(args);
    return result(
      detail({
        report: { size_bytes: Buffer.byteLength(report), content: report },
        diagnostics: [
          "git: cannot change to /home/user/private",
          "see https://private.example.invalid",
        ],
      }),
    );
  };
  const output = (await agentDiagnostics(
    { job_id: jobId, include_report: true },
    new Set([jobId]),
    invoke,
  )) as any;
  assert.deepEqual(calls, [[jobId, "--report", "--json"]]);
  assert.equal(output.owned_by_current_session, true);
  assert.equal(output.review.profile, "code");
  assert.equal(output.report.content_truncated, true);
  assert.match(output.report.content ?? "", /extensions\/scufris\/agents\.ts/);
  assert.match(output.report.content ?? "", /\[redacted-path\]/);
  assert.match(output.report.content ?? "", /\[redacted-url\]/);
  assert.match(output.report.content ?? "", /Prompt: \[redacted\]/);
  assert.doesNotMatch(output.report.content ?? "", /private-bearer-value|ghp_/);
  assert.match(output.report.content ?? "", /\[redacted-environment\]/);
  assert.equal(JSON.stringify(output).includes("/home/user"), false);
  assert.equal(JSON.stringify(output).includes("tmux_session"), false);
  assert.equal(JSON.stringify(output).includes("https://"), false);
  assert.equal("path" in output.repository, false);
});

test("diagnostics rejects invalid combinations before invocation", async () => {
  let called = false;
  const invoke: DiagnosticsInvocation = async () => {
    called = true;
    return result({});
  };
  await assert.rejects(
    agentDiagnostics({ include_report: true }, new Set(), invoke),
    /include_report requires job_id/,
  );
  await assert.rejects(
    agentDiagnostics(
      { job_id: jobId, include_finished: true },
      new Set(),
      invoke,
    ),
    /include_finished cannot be used with job_id/,
  );
  assert.equal(called, false);
});

test("malformed output and helper process failure fail closed", async () => {
  await assert.rejects(
    agentDiagnostics({}, new Set(), async () => ({
      code: 0,
      stdout: Buffer.from("not json"),
    })),
    /returned invalid JSON/,
  );
  await assert.rejects(
    agentDiagnostics({}, new Set(), async () =>
      result({ error: "jobs root: cannot scan /private/state" }, 1),
    ),
    /\[redacted-path\]/,
  );
  await assert.rejects(
    agentDiagnostics({}, new Set(), async () =>
      result({ scope: "live", jobs: [], worktree_path: "/private" }),
    ),
    /list fields/,
  );
  await assert.rejects(
    agentDiagnostics({}, new Set(), async () =>
      result({ scope: "all", jobs: [] }),
    ),
    /response: scope/,
  );
});

test("process runner enforces timeout and output limits", async () => {
  const root = await mkdtemp(join(tmpdir(), "scufris-diagnostics-"));
  const helper = join(root, "helper.mjs");
  try {
    await writeFile(
      helper,
      "#!/usr/bin/env node\nif (process.argv[2] === 'wait') setTimeout(() => {}, 10000); else process.stdout.write('x'.repeat(4096));\n",
    );
    await chmod(helper, 0o700);
    await assert.rejects(
      invokePackagedDiagnostics(["wait"], undefined, helper, 20, 8192),
      /timed out/,
    );
    await assert.rejects(
      invokePackagedDiagnostics(["large"], undefined, helper, 1000, 1024),
      /output exceeded/,
    );
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("existing list and inspect stay unchanged and controls reject discovered jobs", async () => {
  const tools = new Map<string, any>();
  const handlers = new Map<string, Array<(...args: any[]) => unknown>>();
  const api = {
    registerTool(tool: any) {
      tools.set(tool.name, tool);
    },
    on(name: string, handler: (...args: any[]) => unknown) {
      const existing = handlers.get(name) ?? [];
      existing.push(handler);
      handlers.set(name, existing);
    },
    events: { emit() {} },
    sendMessage() {},
  } as unknown as ExtensionAPI;
  scufris(api, {
    diagnosticsInvocation: async () =>
      result({ scope: "live", jobs: [validListJob()] }),
  });

  assert.deepEqual(
    (await execute(tools.get("scufris_agent_list"), {})).details,
    {
      jobs: [],
    },
  );
  assert.deepEqual(
    await execute(tools.get("scufris_agent_inspect"), { job_id: jobId }),
    {
      content: [
        {
          type: "text",
          text: JSON.stringify(
            { error: "job is not owned by this Pi session" },
            null,
            2,
          ),
        },
      ],
      details: { error: "job is not owned by this Pi session" },
      isError: true,
    },
  );
  const discovered = await execute(tools.get("scufris_agent_diagnostics"), {});
  assert.equal(discovered.details.jobs[0].owned_by_current_session, false);
  for (const name of [
    "scufris_agent_send",
    "scufris_agent_retry_review",
    "scufris_agent_stop",
  ]) {
    const params =
      name === "scufris_agent_send"
        ? { job_id: jobId, message: "continue" }
        : { job_id: jobId };
    const controlled = await execute(tools.get(name), params);
    assert.equal(controlled.isError, true, name);
    assert.equal(
      controlled.details.error,
      "job is not owned by this Pi session",
      name,
    );
  }
});

test("tool converts malformed helper responses to concise native errors", async () => {
  const tool = createAgentDiagnosticsTool(new Set(), async () => ({
    code: 0,
    stdout: Buffer.from("{"),
  }));
  const output = await execute(tool, {});
  assert.equal(output.isError, true);
  assert.deepEqual(output.details, {
    error: "Scufris diagnostics helper returned invalid JSON",
  });
});
