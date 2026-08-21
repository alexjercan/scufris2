import assert from "node:assert/strict";
import test from "node:test";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import identity, {
  appendPairPrompt,
  pairPrompt,
} from "../extensions/scufris/identity.ts";

const canonical =
  "You are Scufris, the foreground orchestrator. Pair by default: inspect project instructions, docs, code, tests, history, and user style before proposing work. Stop only at real decisions. Give recommendations and consequences; options are not exhaustive. Record durable decisions. Full-send only bounded, well-defined work. Give workers complete context, scope, artifacts, checks, and protocol. Mediate decisions, blockers, review, and landing. Workers are bounded executors, not user sessions.";

test("canonical Pair prompt is exact, bounded ASCII", () => {
  assert.equal(pairPrompt, `${canonical}\n`);
  assert.equal(Buffer.byteLength(pairPrompt, "ascii"), 495);
  assert.match(pairPrompt, /^[\x00-\x7f]+$/);
  assert.equal(appendPairPrompt("base"), `base\n\n${canonical}`);
});

test("Pair prompt is added on every Scufris agent start only", () => {
  const original = process.env.SCUFRIS_FOREGROUND;
  const handlers: Array<(event: { systemPrompt: string }) => unknown> = [];
  const api = {
    on(event: string, handler: (event: { systemPrompt: string }) => unknown) {
      if (event === "before_agent_start") handlers.push(handler);
    },
  } as unknown as ExtensionAPI;

  try {
    delete process.env.SCUFRIS_FOREGROUND;
    identity(api);
    assert.equal(handlers.length, 0);

    process.env.SCUFRIS_FOREGROUND = "1";
    identity(api);
    assert.equal(handlers.length, 1);
    for (const base of ["first turn", "post-compaction turn"]) {
      assert.deepEqual(handlers[0]?.({ systemPrompt: base }), {
        systemPrompt: `${base}\n\n${canonical}`,
      });
    }
  } finally {
    if (original === undefined) delete process.env.SCUFRIS_FOREGROUND;
    else process.env.SCUFRIS_FOREGROUND = original;
  }
});
