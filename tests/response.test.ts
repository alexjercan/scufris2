import assert from "node:assert/strict";
import test from "node:test";
import response, {
  FINAL_TOOL,
  RESPONSE_ENTRY,
  finalResponsePolicy,
  plainProse,
} from "../agent/extensions/scufris/response.ts";
import { AGENT_RESPONSE_EVENT } from "../agent/extensions/scufris/service/client.ts";

test("plain response text is bounded prose", () => {
  assert.equal(plainProse("  All tests passed.  "), "All tests passed.");
  assert.equal(plainProse(""), undefined);
  assert.equal(plainProse("x".repeat(9 * 1024)), undefined);
});

test("the final response tool emits one atomic response", async () => {
  const handlers = new Map<string, Array<(event: any, context: any) => any>>();
  const emitted: Array<{ name: string; value: unknown }> = [];
  const entries: Array<{ type: string; value: unknown }> = [];
  let tool: any;
  const api = {
    on(name: string, handler: (event: any, context: any) => any) {
      handlers.set(name, [...(handlers.get(name) ?? []), handler]);
    },
    events: {
      on() {},
      emit(name: string, value: unknown) {
        emitted.push({ name, value });
      },
    },
    registerTool(value: any) {
      tool = value;
    },
    registerEntryRenderer() {},
    registerMarkdownTransformer() {},
    appendEntry(type: string, value: unknown) {
      entries.push({ type, value });
    },
  };
  const previous = process.env.SCUFRIS_ROLE;
  process.env.SCUFRIS_ROLE = "orchestrator";
  try {
    response(api as never);
    assert.equal(tool.name, FINAL_TOOL);
    const call = {
      id: "call-1",
      name: FINAL_TOOL,
      arguments: {
        text: "All tests passed.",
        details: "## Verification\n\n84 passed.",
        widgets: [
          { id: "widget-1", name: "summary", arguments: { passed: 84 } },
        ],
      },
    };
    const message = {
      role: "assistant",
      content: [call],
      stopReason: "toolUse",
    };
    await handlers.get("message_end")![0]!({ message }, {});
    await tool.execute("call-1", call.arguments, undefined, undefined, {});
    assert.deepEqual(emitted, [
      {
        name: AGENT_RESPONSE_EVENT,
        value: {
          text: "All tests passed.",
          details: "## Verification\n\n84 passed.",
          widgets: [
            { id: "widget-1", name: "summary", arguments: { passed: 84 } },
          ],
        },
      },
    ]);
    assert.equal(entries[0]?.type, RESPONSE_ENTRY);
  } finally {
    if (previous === undefined) delete process.env.SCUFRIS_ROLE;
    else process.env.SCUFRIS_ROLE = previous;
  }
});

test("the policy requires atomic details and widgets", () => {
  assert.match(finalResponsePolicy, /mandatory short plain prose/);
  assert.match(finalResponsePolicy, /optional Markdown/);
  assert.match(finalResponsePolicy, /optional stored attachment IDs/);
  assert.match(finalResponsePolicy, /optional best-effort presentation calls/);
});
