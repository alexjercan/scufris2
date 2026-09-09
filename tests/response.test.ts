import assert from "node:assert/strict";
import test from "node:test";
import response, {
  FINAL_TOOL,
  RESPONSE_ENTRY,
  finalResponsePolicy,
  offerPolicy,
  plainProse,
  restoredOfferPrompts,
} from "../agent/extensions/scufris/response.ts";
import { AGENT_RESPONSE_EVENT } from "../agent/extensions/scufris/service/client.ts";
import {
  JOB_CITATION_EVENT,
  OFFER_TAKE_EVENT,
} from "../agent/extensions/scufris/shared/citations.ts";

test("plain response text is bounded literal prose", () => {
  assert.equal(plainProse("  All tests passed.  "), "All tests passed.");
  assert.equal(
    plainProse("**Accidental Markdown** and `code` stay literal."),
    "**Accidental Markdown** and `code` stay literal.",
  );
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

test("the policy keeps Markdown out of text and in atomic details", () => {
  assert.match(finalResponsePolicy, /mandatory short literal plain prose/);
  assert.match(finalResponsePolicy, /do not put Markdown headings, lists/);
  assert.match(finalResponsePolicy, /code fences, inline code/);
  assert.match(
    finalResponsePolicy,
    /formatted explanation in optional Markdown details/,
  );
  assert.match(finalResponsePolicy, /optional stored attachment IDs/);
  assert.match(finalResponsePolicy, /optional best-effort presentation calls/);
});

test("the offer policy asks for one obvious next step and nothing else", () => {
  assert.match(offerPolicy, /only when you are reporting on a job this turn/);
  assert.match(offerPolicy, /Name the job in job_id/);
  assert.match(offerPolicy, /Never offer a measured fact, a question/);
  assert.match(offerPolicy, /carries no measured badges for is dropped/);
});

/** One orchestrator response extension with its events and messages captured. */
function orchestrator() {
  const listeners = new Map<string, Array<(value: unknown) => void>>();
  const emitted: Array<{ name: string; value: any }> = [];
  const entries: Array<{ type: string; value: any }> = [];
  const sent: Array<{ message: any; options: any }> = [];
  let tool: any;
  const api = {
    on() {},
    events: {
      on(name: string, handler: (value: unknown) => void) {
        listeners.set(name, [...(listeners.get(name) ?? []), handler]);
      },
      emit(name: string, value: unknown) {
        emitted.push({ name, value });
        for (const handler of listeners.get(name) ?? []) handler(value);
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
    sendMessage(message: any, options: any) {
      sent.push({ message, options });
      return Promise.resolve();
    },
  };
  const previous = process.env.SCUFRIS_ROLE;
  process.env.SCUFRIS_ROLE = "orchestrator";
  try {
    response(api as never);
  } finally {
    if (previous === undefined) delete process.env.SCUFRIS_ROLE;
    else process.env.SCUFRIS_ROLE = previous;
  }
  return { api, tool: tool!, emitted, entries, sent };
}

const LANDED = {
  label: "landed",
  value: "yes",
  state: "measured",
} as const;
const UNMEASURED = {
  label: "tests",
  value: "unknown",
  state: "unknown",
} as const;

test("measured badges reach the answer that reports them, grouped by job", async () => {
  const run = orchestrator();
  run.api.events.emit(JOB_CITATION_EVENT, {
    job_id: "3f81c204b1e9",
    badges: [LANDED, UNMEASURED],
  });
  run.api.events.emit(JOB_CITATION_EVENT, {
    job_id: "7c2ad0114f60",
    badges: [LANDED],
  });
  await run.tool.execute("call-1", { text: "Both jobs are done." });
  const entry = run.entries.at(-1)!.value;
  assert.deepEqual(entry.receipts, [
    { job_id: "3f81c204b1e9", badges: [LANDED, UNMEASURED], offers: [] },
    { job_id: "7c2ad0114f60", badges: [LANDED], offers: [] },
  ]);
  // The badges are spent. A later answer that reports on nothing carries none.
  await run.tool.execute("call-2", { text: "Nothing else to say." });
  assert.equal("receipts" in run.entries.at(-1)!.value, false);
});

test("an offer is a label over words the surface never sees", async () => {
  const run = orchestrator();
  run.api.events.emit(JOB_CITATION_EVENT, {
    job_id: "3f81c204b1e9",
    badges: [LANDED],
  });
  await run.tool.execute("call-1", {
    text: "It is ready to land.",
    offers: [
      {
        job_id: "3f81c204b1e9",
        label: "land it",
        prompt: "merge the branch and push it",
      },
      {
        job_id: "7c2ad0114f60",
        label: "no strip",
        prompt: "this job has no badges here",
      },
    ],
  });
  const entry = run.entries.at(-1)!.value;
  assert.equal(entry.receipts.length, 1);
  const offers = entry.receipts[0].offers;
  assert.equal(offers.length, 1);
  assert.deepEqual(Object.keys(offers[0]), ["id", "label"]);
  assert.equal(offers[0].label, "land it");
  assert.match(offers[0].id, /^offer-[0-9a-f]{12}$/);
  const prompt = "for job 3f81c204b1e9: merge the branch and push it";
  assert.equal(entry.offer_prompts[offers[0].id], prompt);
  const answered = run.emitted
    .filter((event) => event.name === AGENT_RESPONSE_EVENT)
    .at(-1)!.value;
  assert.equal("offer_prompts" in answered, false);

  run.api.events.emit(OFFER_TAKE_EVENT, { id: offers[0].id });
  assert.equal(run.sent.length, 1);
  // A follow-up, not a user line: nothing in the conversation is recorded as
  // words Alex typed.
  assert.equal(run.entries.length, 1);
  assert.equal(run.sent[0]!.message.content, prompt);
  assert.equal(run.sent[0]!.message.customType, "scufris-offer");
  assert.deepEqual(run.sent[0]!.options, {
    deliverAs: "followUp",
    triggerTurn: true,
  });
  // Pressing a spent offer, or one this session never wrote, does nothing.
  run.api.events.emit(OFFER_TAKE_EVENT, { id: offers[0].id });
  run.api.events.emit(OFFER_TAKE_EVENT, { id: "offer-000000000000" });
  assert.equal(run.sent.length, 1);
});

test("a restart honours the buttons an earlier session drew", () => {
  const prompts = restoredOfferPrompts([
    {
      type: "custom",
      customType: RESPONSE_ENTRY,
      data: {
        version: 5,
        text: "It landed.",
        offer_prompts: { "offer-a1": "for job 3f81c204b1e9: land it" },
      },
    },
    {
      type: "custom",
      customType: RESPONSE_ENTRY,
      data: { version: 4, offer_prompts: { "offer-old": "an older entry" } },
    },
    {
      type: "message",
      data: { offer_prompts: { "offer-x": "not a response" } },
    },
  ]);
  assert.deepEqual(
    [...prompts],
    [["offer-a1", "for job 3f81c204b1e9: land it"]],
  );
});
