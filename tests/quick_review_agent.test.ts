import assert from "node:assert/strict";
import { chmodSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { startQuickReviewAgent } from "../agent/extensions/scufris/workflow/quick-review-agent.ts";

const base = "1".repeat(40);
const revision = "2".repeat(40);

function fakeHelper(messages: object[], keepAlive = true): string {
  const directory = mkdtempSync(join(tmpdir(), "scufris-quick-review-agent-"));
  const path = join(directory, "helper.mjs");
  writeFileSync(
    path,
    `#!/usr/bin/env node
process.stdin.resume();
process.stdin.on("end", () => {
  for (const message of ${JSON.stringify(messages)}) console.log(JSON.stringify(message));
  if (${keepAlive}) setInterval(() => undefined, 1000);
});
`,
    { mode: 0o700 },
  );
  chmodSync(path, 0o700);
  return path;
}

test("standalone Quick Review adapter returns readiness then completion", async () => {
  const event = {
    version: 1,
    outcome: "approved",
    repository: "/repo",
    baseRef: base,
    targetRef: revision,
    baseRevision: base,
    revision,
    identity: "3".repeat(64),
    sections: 1,
    comments: [],
    overallComment: "",
    questions: [],
    artifact: "/state/walkthrough.md",
    state: "/state/state.json",
    completedAt: "2026-08-24T00:00:00.000Z",
  } as const;
  const agent = await startQuickReviewAgent(
    {
      repository: "/repo",
      base_revision: base,
      revision,
      model: "model",
      thinking: "medium",
      state_dir: "/state",
    },
    {
      helperPath: fakeHelper([{ type: "ready" }, { type: "completed", event }]),
    },
  );
  assert.deepEqual(await agent.completion, event);
  await agent.close();
});

test("standalone Quick Review adapter rejects an early helper failure", async () => {
  await assert.rejects(
    startQuickReviewAgent(
      {
        repository: "/repo",
        base_revision: base,
        revision,
        model: "model",
        thinking: "medium",
        state_dir: "/state",
      },
      { helperPath: fakeHelper([{ broken: true }], false) },
    ),
    /exited before completion/,
  );
});
