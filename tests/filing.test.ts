import assert from "node:assert/strict";
import test from "node:test";
import { createHash } from "node:crypto";
import { mkdirSync, mkdtempSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import orchestration from "../agent/extensions/scufris/workflow/orchestration.ts";
import { JOB_ROWS_EVENT } from "../agent/extensions/scufris/shared/job-rows.ts";
import {
  FOREGROUND_OWNER,
  OWNER_EVENT,
} from "../agent/extensions/scufris/shared/owner.ts";
import type { JobRow } from "../agent/extensions/scufris/service/protocol.ts";

const JOB = "abcdef123456";
const FILED_ROWS = "scufris-filed-rows-v2";

type Handler = (...args: never[]) => unknown;

/** One finished job of the conversation, as the helper stores it.
 *
 * The record is what `recover` reads, so the fields are the helper's own and
 * not a convenience shape. Nothing runs behind it: the execution is over, the
 * status file holds its terminal event, and the offset is past it so no wake
 * is pending.
 */
function storeJob(state: string, owner: string): void {
  const directory = join(state, "scufris", "jobs", JOB);
  mkdirSync(join(directory, "workspace"), { recursive: true });
  const status = join(directory, "status");
  const events =
    JSON.stringify({
      generation: 1,
      event: "done",
      summary: "fixture finished",
    }) + "\n";
  writeFileSync(status, events);
  const stats = statSync(status);
  writeFileSync(
    join(directory, "job.json"),
    JSON.stringify({
      version: 2,
      job_id: JOB,
      owner_session: owner,
      workflow_id: createHash("sha256").update(`workflow:${JOB}`).digest("hex"),
      root_job: JOB,
      parent_job: null,
      project: null,
      project_root: null,
      project_root_device: null,
      project_root_inode: null,
      context_fingerprint: null,
      workspace: "temporary",
      feature: null,
      review_of: null,
      working_directory: join(directory, "workspace"),
      workspace_device: stats.dev,
      workspace_inode: stats.ino,
      landing_branch: null,
      harness: "pi",
      harness_session: "00000000-0000-4000-8000-000000000000",
      model: "fixture-model",
      thinking: "medium",
      state: "done",
      summary: "fixture finished",
      created_at: "2026-08-23T12:00:00Z",
      archived_at: null,
      generation: 1,
      event_offset: events.length,
      status_device: stats.dev,
      status_inode: stats.ino,
      execution_state: null,
      tmux_session_name: null,
      tmux_session_id: null,
      tmux_window_id: null,
      tmux_pane_id: null,
      execution_token: null,
      cleanup: null,
    }),
  );
  writeFileSync(join(directory, "report.md"), "");
  writeFileSync(join(directory, "report.lock"), "");
  writeFileSync(join(directory, "prompt.md"), "Fixture prompt.\n");
  writeFileSync(join(directory, "conversation.md"), "");
  writeFileSync(
    join(directory, ".report-auth.json"),
    JSON.stringify({
      generation: 1,
      launch_capability_hash: null,
      report_capability_hash: null,
      trusted_capability_hash: "0".repeat(64),
    }),
  );
}

function harness(entries: unknown[]) {
  const lifecycle = new Map<string, Handler[]>();
  const signals = new Map<string, Handler[]>();
  const published: JobRow[][] = [];
  const add = (map: Map<string, Handler[]>, name: string, handler: Handler) => {
    const registered = map.get(name) ?? [];
    registered.push(handler);
    map.set(name, registered);
  };
  const pi = {
    registerTool: () => {},
    registerCommand: () => {},
    on: (name: string, handler: Handler) => add(lifecycle, name, handler),
    appendEntry: (customType: string, data: unknown) =>
      entries.push({ type: "custom", customType, data }),
    sendMessage: () => {},
    events: {
      on: (name: string, handler: Handler) => add(signals, name, handler),
      emit: (name: string, value: unknown) => {
        if (name === JOB_ROWS_EVENT) published.push(value as JobRow[]);
        for (const handler of signals.get(name) ?? [])
          (handler as (value: unknown) => void)(value);
      },
    },
  };
  const context = {
    hasUI: false,
    ui: { notify: () => {} },
    sessionManager: {
      getSessionId: () => "terminal-session",
      getBranch: () => entries,
      getEntries: () => entries,
    },
  };
  return {
    pi,
    context,
    published,
    start: async () => {
      for (const handler of lifecycle.get("session_start") ?? [])
        await (handler as (event: unknown, context: unknown) => unknown)(
          {},
          context,
        );
    },
    announce: (owner: string) => {
      for (const handler of signals.get(OWNER_EVENT) ?? [])
        (handler as (value: unknown) => void)({ owner });
    },
    /** Every publication this process makes, once it has stopped making them.
     *
     * Adoption reads the helper and the events behind it, so a row reaches the
     * surfaces some milliseconds after the lease does, and more than once. The
     * answer is the whole burst, because one right publication among wrong ones
     * is still a filed row drawn on the HUD.
     */
    settled: async (since: number) => {
      let seen = published.length;
      for (let attempt = 0; attempt < 400; attempt += 1) {
        await new Promise((resolve) => setTimeout(resolve, 25));
        if (published.length > seen) {
          seen = published.length;
          continue;
        }
        if (seen > since) return published.slice(since);
      }
      assert.fail("row publications did not settle");
    },
  };
}

// A terminal takes the conversation from the service after its session has
// already started, so the jobs it owns arrive with the lease and not before.
// The filing set is read against the jobs in hand, so restoring it once at
// session start fenced nothing at all: every filed row came back to the HUD on
// the next restart, and filing them again wrote the empty set over what was
// there.
test("work adopted after the session started keeps what was filed", async () => {
  const root = mkdtempSync(join(tmpdir(), "scufris-filing-"));
  const state = join(root, "state");
  mkdirSync(state, { recursive: true });
  storeJob(state, FOREGROUND_OWNER);
  process.env.XDG_STATE_HOME = state;
  process.env.SCUFRIS_ROLE = "orchestrator";
  delete process.env.SCUFRIS_JOB_OWNER;

  const entries: unknown[] = [
    {
      type: "custom",
      customType: FILED_ROWS,
      data: { version: 2, filed: [{ id: JOB, generation: 1 }] },
    },
  ];
  const bench = harness(entries);
  orchestration(bench.pi as never);

  // No lease yet: this process speaks for its own session, which owns nothing.
  await bench.start();
  assert.deepEqual(await bench.settled(0), [[], []]);

  // The lease names this process the conversation, and its work arrives filed.
  const start = bench.published.length;
  bench.announce(FOREGROUND_OWNER);
  for (const rows of await bench.settled(start)) assert.deepEqual(rows, []);

  // What is filed stays filed: nothing rewrote the set as it was published.
  const last = entries
    .filter(
      (entry): entry is { customType: string; data: { filed: unknown } } =>
        (entry as { customType?: string }).customType === FILED_ROWS,
    )
    .at(-1);
  assert.deepEqual(last?.data.filed, [{ id: JOB, generation: 1 }]);
});
