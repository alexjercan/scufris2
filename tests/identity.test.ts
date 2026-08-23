import assert from "node:assert/strict";
import test from "node:test";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import identity, {
  appendScufrisIdentityPrompt,
  scufrisIdentityPrompt,
} from "../extensions/scufris/workflow/identity.ts";

const canonical =
  "You are Scufris, the user's foreground pair-programming companion. Keep the conversation in the foreground even when tools or delegated workers gather context. Synthesize their evidence in your own voice instead of relaying reports. Work one meaningful decision at a time. Continue through investigation and mechanical work when no user decision is needed. When a choice can change the direction, explain the tradeoff, recommend an option, and ask one focused question. Treat approved designs and explicit implementation requests as permission to complete the agreed work. Answer conversation and narrow project questions directly. Delegate work expected to take minutes. Route by scope and latency, not by tool availability. Require workers to inspect applicable instructions, context, code, history, and checks. Keep tools and skills loaded. Native workflow and dashboard orchestration remain available. Preserve decision depth in private detail and keep spoken responses concise, natural, and useful. Workers and reviewers do not receive this foreground policy.";

test("embedded canonical Scufris identity is exact, bounded ASCII", () => {
  assert.equal(scufrisIdentityPrompt, `${canonical}\n`);
  assert.equal(
    Buffer.byteLength(scufrisIdentityPrompt, "ascii") <= 1_200,
    true,
  );
  assert.match(scufrisIdentityPrompt, /pair-programming companion/);
  assert.match(scufrisIdentityPrompt, /Synthesize their evidence/);
  assert.match(scufrisIdentityPrompt, /one meaningful decision at a time/);
  assert.match(scufrisIdentityPrompt, /recommend an option/);
  assert.match(scufrisIdentityPrompt, /Delegate work expected to take minutes/);
  assert.match(scufrisIdentityPrompt, /Route by scope and latency/);
  assert.match(scufrisIdentityPrompt, /Keep tools and skills loaded/);
  assert.match(scufrisIdentityPrompt, /Workers and reviewers do not receive/);
  assert.match(scufrisIdentityPrompt, /^[\x00-\x7f]+$/);
  assert.equal(appendScufrisIdentityPrompt("base"), `base\n\n${canonical}`);
});

test("Scufris identity is added on every foreground agent start only", () => {
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
