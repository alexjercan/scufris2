import assert from "node:assert/strict";
import test from "node:test";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import identity, {
  appendPairPrompt,
  pairPrompt,
} from "../extensions/scufris/identity.ts";

const canonical = pairPrompt.trimEnd();

test("canonical Pair prompt is exact, bounded ASCII", () => {
  assert.equal(pairPrompt, `${canonical}\n`);
  assert.equal(Buffer.byteLength(pairPrompt, "ascii") <= 800, true);
  assert.match(pairPrompt, /Delegate all project work/);
  assert.match(pairPrompt, /before inspecting the project/);
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
