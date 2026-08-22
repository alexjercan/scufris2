import assert from "node:assert/strict";
import test from "node:test";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import identity, {
  appendPairPrompt,
  pairPrompt,
} from "../extensions/scufris/identity.ts";

const canonical =
  "You are Scufris, the foreground conversational orchestrator. Answer conversation and product decisions directly. Handle narrow project work directly when it should take seconds, including reading one named file, a small task record, or a focused repository question. Delegate work expected to take minutes, such as broad codebase review, substantial research, implementation, full checks, releases, or deployment. Route by scope and latency, not the presence of project tools. Require workers to inspect applicable instructions, context, code, history, and checks. Keep tools and skills loaded. Native delegation and widget orchestration remain available. Preserve decision depth in private detail and keep spoken output short. Workers and reviewers do not receive this foreground policy.";

test("embedded canonical Pair prompt is exact, bounded ASCII", () => {
  assert.equal(pairPrompt, `${canonical}\n`);
  assert.equal(Buffer.byteLength(pairPrompt, "ascii") <= 800, true);
  assert.match(pairPrompt, /Handle narrow project work directly/);
  assert.match(pairPrompt, /reading one named file/);
  assert.match(pairPrompt, /Delegate work expected to take minutes/);
  assert.match(pairPrompt, /Route by scope and latency/);
  assert.match(pairPrompt, /Keep tools and skills loaded/);
  assert.match(pairPrompt, /Workers and reviewers do not receive/);
  assert.match(pairPrompt, /^[\x00-\x7f]+$/);
  assert.equal(appendPairPrompt("base"), `base\n\n${canonical}`);
});

test("Pair prompt is added on every Scufris agent start only", () => {
  const original = process.env.SCUFRIS_ROLE;
  const handlers: Array<(event: { systemPrompt: string }) => unknown> = [];
  const api = {
    on(event: string, handler: (event: { systemPrompt: string }) => unknown) {
      if (event === "before_agent_start") handlers.push(handler);
    },
  } as unknown as ExtensionAPI;

  try {
    delete process.env.SCUFRIS_ROLE;
    identity(api);
    process.env.SCUFRIS_ROLE = "worker";
    identity(api);
    assert.equal(handlers.length, 0);

    process.env.SCUFRIS_ROLE = "orchestrator";
    identity(api);
    assert.equal(handlers.length, 1);
    for (const base of ["first turn", "post-compaction turn"]) {
      assert.deepEqual(handlers[0]?.({ systemPrompt: base }), {
        systemPrompt: `${base}\n\n${canonical}`,
      });
    }
  } finally {
    if (original === undefined) delete process.env.SCUFRIS_ROLE;
    else process.env.SCUFRIS_ROLE = original;
  }
});
