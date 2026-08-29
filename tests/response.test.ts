import assert from "node:assert/strict";
import {
  chmodSync,
  existsSync,
  lstatSync,
  mkdtempSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import type { AssistantMessage } from "@earendil-works/pi-ai";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { Check } from "typebox/value";
import response, {
  ArtifactStore,
  FINAL_TOOL,
  RESPONSE_ENTRY,
  assembleScufrisPrompt,
  maxArtifacts,
  promptInspectionMarkdown,
  responseText,
  splitDirectResponse,
} from "../agent/extensions/scufris/response.ts";
import {
  FINAL_RESPONSE_TOOL,
  registerForegroundAcknowledgmentLifecycle,
} from "../agent/extensions/scufris/workflow/orchestration.ts";
import { SPOKEN_EVENT } from "../agent/extensions/scufris/shared/spoken.ts";

/**
 * The paragraph the speaker was last handed, if any.
 *
 * The extension emits it; nothing here decides whether a sound is made, which
 * is the companion's, so what these tests assert is what was offered.
 */
function spoken(emitted: Array<{ channel: string; request: any }>) {
  return emitted
    .filter((item) => item.channel === SPOKEN_EVENT)
    .map((item) => item.request?.speak)
    .filter((paragraph): paragraph is string => typeof paragraph === "string")
    .at(-1);
}

const usage = {
  input: 0,
  output: 0,
  cacheRead: 0,
  cacheWrite: 0,
  totalTokens: 0,
  cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
};

function assistant(text: string): AssistantMessage {
  return {
    role: "assistant",
    content: [{ type: "text", text }],
    api: "test",
    provider: "test",
    model: "test",
    usage,
    stopReason: "stop",
    timestamp: 0,
  };
}

function fixture() {
  const root = mkdtempSync(join(tmpdir(), "scufris-response-"));
  const sessionFile = join(root, "session.jsonl");
  writeFileSync(sessionFile, "{}\n", { mode: 0o600 });
  return { root, sessionFile };
}

function harness() {
  const { sessionFile } = fixture();
  const handlers = new Map<string, Array<(event: any, context: any) => any>>();
  const entries: any[] = [];
  const tools = new Map<string, any>();
  const commands = new Map<string, any>();
  const renderers = new Map<string, any>();
  const emitted: Array<{ channel: string; request: any }> = [];
  const eventHandlers = new Map<string, Array<(value: unknown) => void>>();
  const messages: any[] = [];
  const notices: string[] = [];
  const api = {
    on(name: string, handler: (event: any, context: any) => any) {
      handlers.set(name, [...(handlers.get(name) ?? []), handler]);
    },
    registerTool(tool: any) {
      tools.set(tool.name, tool);
    },
    registerCommand(name: string, command: any) {
      commands.set(name, command);
    },
    registerEntryRenderer(name: string, renderer: any) {
      renderers.set(name, renderer);
    },
    registerMarkdownTransformer() {},
    appendEntry(customType: string, data: any) {
      entries.push({
        type: "custom",
        id: `custom-${entries.length}`,
        customType,
        data,
      });
    },
    sendMessage(message: any) {
      messages.push(message);
    },
    getAllTools() {
      return [...tools.values()].map((tool) => ({ name: tool.name }));
    },
    getActiveTools() {
      return [...tools.keys()];
    },
    events: {
      on(channel: string, handler: (value: unknown) => void) {
        eventHandlers.set(channel, [
          ...(eventHandlers.get(channel) ?? []),
          handler,
        ]);
      },
      emit(channel: string, request: any) {
        for (const handler of eventHandlers.get(channel) ?? [])
          handler(request);
        emitted.push({ channel, request });
      },
    },
  } as unknown as ExtensionAPI;
  const context = {
    ui: {
      notify(message: string) {
        notices.push(message);
      },
    },
    sessionManager: {
      getSessionFile() {
        return sessionFile;
      },
      getSessionId() {
        return "session-owned-id";
      },
      getBranch() {
        return entries;
      },
    },
    getSystemPrompt() {
      return "Pi base";
    },
    getSystemPromptOptions() {
      return { selectedTools: [...tools.keys()] };
    },
  };
  response(api);
  return {
    api,
    entries,
    tools,
    commands,
    renderers,
    emitted,
    messages,
    notices,
    context,
    async emit(name: string, event: any) {
      let result;
      for (const handler of handlers.get(name) ?? [])
        result = await handler(event, context);
      return result;
    },
  };
}

test("response policy and final tool apply only to the foreground role", (t) => {
  const previous = process.env.SCUFRIS_ROLE;
  process.env.SCUFRIS_ROLE = "worker";
  t.after(() =>
    previous === undefined
      ? delete process.env.SCUFRIS_ROLE
      : (process.env.SCUFRIS_ROLE = previous),
  );
  const handlers: string[] = [];
  const tools: string[] = [];
  response({
    on(name: string) {
      handlers.push(name);
    },
    registerTool(tool: any) {
      tools.push(tool.name);
    },
  } as unknown as ExtensionAPI);
  assert.deepEqual(handlers, []);
  assert.deepEqual(tools, []);
});

test("direct output is split, hidden detail is persisted, and malformed output fails safe", async (t) => {
  const previous = process.env.SCUFRIS_ROLE;
  process.env.SCUFRIS_ROLE = "orchestrator";
  t.after(() =>
    previous === undefined
      ? delete process.env.SCUFRIS_ROLE
      : (process.env.SCUFRIS_ROLE = previous),
  );
  const app = harness();
  const result = await app.emit("message_end", {
    message: assistant(
      "The implementation is ready.\n\n# Detail\n\n- Tests pass",
    ),
  });
  const text = result.message.content[0].text as string;
  assert.match(
    text,
    /^The implementation is ready\.\n\n\/detail [a-f0-9]{24}$/,
  );
  assert.doesNotMatch(text, /Tests pass|# Detail/);
  assert.equal(
    app.entries.some((entry) => entry.customType === RESPONSE_ENTRY),
    false,
  );
  const artifactId = text.match(/\/detail ([a-f0-9]{24})$/)?.[1];
  assert.ok(artifactId);
  const store = new ArtifactStore({
    sessionFile: app.context.sessionManager.getSessionFile(),
    sessionId: "session-owned-id",
  });
  assert.match(store.read(artifactId).markdown, /Tests pass/);

  const malformed = await app.emit("message_end", {
    message: assistant("# unsafe\n/path"),
  });
  assert.match(
    malformed.message.content[0].text,
    /^I could not safely present/,
  );
  assert.doesNotMatch(malformed.message.content[0].text, /unsafe|path/);
});

test("final tool keeps scrubbed arguments executable and terminates", async (t) => {
  const previous = process.env.SCUFRIS_ROLE;
  process.env.SCUFRIS_ROLE = "orchestrator";
  t.after(() =>
    previous === undefined
      ? delete process.env.SCUFRIS_ROLE
      : (process.env.SCUFRIS_ROLE = previous),
  );
  const app = harness();
  const call = {
    type: "toolCall" as const,
    id: "final-call",
    name: FINAL_TOOL,
    arguments: {
      spoken: "The work is ready for review.",
      detail: "# Private\n\nSecret detail.",
    },
  };
  const message = {
    ...assistant(""),
    content: [call],
    stopReason: "toolUse" as const,
  };
  const replaced = await app.emit("message_end", { message });
  assert.deepEqual(replaced.message.content[0].arguments, {
    spoken: "The work is ready for review.",
  });
  const finalTool = app.tools.get(FINAL_TOOL);
  assert.equal(
    Check(finalTool.parameters, replaced.message.content[0].arguments),
    true,
  );
  assert.equal(
    Check(finalTool.parameters, {
      spoken: "The work is ready for review.",
      artifact_id: "0123456789abcdef01234567",
    }),
    false,
  );
  const toolResult = await finalTool.execute(
    "final-call",
    replaced.message.content[0].arguments,
    undefined,
    undefined,
    app.context,
  );
  assert.equal(toolResult.terminate, true);
  assert.deepEqual(toolResult.content, [
    { type: "text", text: "Final response recorded." },
  ]);
  assert.equal(
    app.entries.filter((entry) => entry.customType === RESPONSE_ENTRY).length,
    1,
  );

  const intermediate = {
    ...assistant("# Hidden intermediate detail"),
    content: [
      { type: "text" as const, text: "# Hidden intermediate detail" },
      {
        type: "toolCall" as const,
        id: "read-call",
        name: "read",
        arguments: { path: "private" },
      },
    ],
    stopReason: "toolUse" as const,
  };
  const hidden = await app.emit("message_end", { message: intermediate });
  assert.equal(
    hidden.message.content.some((item: any) => item.type === "text"),
    false,
  );
});

test("text-only pending acknowledgment is hidden and lifecycle gate resets", async (t) => {
  const previous = process.env.SCUFRIS_ROLE;
  process.env.SCUFRIS_ROLE = "orchestrator";
  t.after(() =>
    previous === undefined
      ? delete process.env.SCUFRIS_ROLE
      : (process.env.SCUFRIS_ROLE = previous),
  );
  const app = harness();
  const gate = registerForegroundAcknowledgmentLifecycle(app.api);
  gate.markSuccessfulAction("scufris_job_spawn");

  const replaced = await app.emit("message_end", {
    message: assistant("I started the worker."),
  });
  app.entries.push({
    type: "message",
    id: "text-only-acknowledgment",
    message: replaced.message,
  });
  assert.deepEqual(replaced.message.content, []);
  assert.equal(spoken(app.emitted), undefined);
  assert.equal(
    app.entries.some((entry) => entry.customType === RESPONSE_ENTRY),
    false,
  );
  assert.match(gate.blockReason("read") ?? "", /only permitted/);

  await app.emit("agent_settled", {});
  assert.equal(gate.blockReason("read"), undefined);
});

test("rejected and preflight-blocked final batches create no response artifacts", async (t) => {
  const previous = process.env.SCUFRIS_ROLE;
  process.env.SCUFRIS_ROLE = "orchestrator";
  t.after(() =>
    previous === undefined
      ? delete process.env.SCUFRIS_ROLE
      : (process.env.SCUFRIS_ROLE = previous),
  );
  const app = harness();
  registerForegroundAcknowledgmentLifecycle(app.api);
  const call = {
    type: "toolCall" as const,
    id: "blocked-final",
    name: FINAL_TOOL,
    arguments: {
      spoken: "This batch must not become visible.",
      detail: "# Private rejected detail",
    },
  };
  const replaced = await app.emit("message_end", {
    message: {
      ...assistant(""),
      content: [
        call,
        {
          type: "toolCall" as const,
          id: "sibling-read",
          name: "read",
          arguments: { path: "private" },
        },
      ],
      stopReason: "toolUse" as const,
    },
  });
  app.entries.push({
    type: "message",
    id: "rejected-final-batch",
    message: replaced.message,
  });

  const block = await app.emit("tool_call", {
    toolName: FINAL_TOOL,
    toolCallId: call.id,
    input: replaced.message.content[0].arguments,
  });
  assert.equal(block.block, true);
  await app.emit("tool_result", {
    toolName: FINAL_TOOL,
    toolCallId: call.id,
    input: replaced.message.content[0].arguments,
    content: [{ type: "text", text: block.reason }],
    details: undefined,
    isError: true,
  });

  assert.equal(
    app.entries.some((entry) => entry.customType === RESPONSE_ENTRY),
    false,
  );
  assert.equal(
    existsSync(`${app.context.sessionManager.getSessionFile()}.scufris`),
    false,
  );
  assert.equal(spoken(app.emitted), undefined);
});

test("spawn and send can each finish with one safe final response and no polling", async (t) => {
  const previous = process.env.SCUFRIS_ROLE;
  process.env.SCUFRIS_ROLE = "orchestrator";
  t.after(() =>
    previous === undefined
      ? delete process.env.SCUFRIS_ROLE
      : (process.env.SCUFRIS_ROLE = previous),
  );
  const app = harness();
  const gate = registerForegroundAcknowledgmentLifecycle(app.api);
  const finalTool = app.tools.get(FINAL_TOOL);

  for (const [index, action] of [
    "scufris_job_spawn",
    "scufris_job_send",
  ].entries()) {
    gate.markSuccessfulAction(action);
    assert.match(
      gate.blockReason("scufris_job_inspect") ?? "",
      /only permitted/,
    );
    assert.match(gate.blockReason("bash") ?? "", /only permitted/);
    assert.equal(gate.blockReason(FINAL_RESPONSE_TOOL), undefined);

    const call = {
      type: "toolCall" as const,
      id: `ack-${index}`,
      name: FINAL_TOOL,
      arguments: {
        spoken:
          action === "scufris_job_spawn"
            ? "I started the implementation and will let you know when it needs attention."
            : "I sent that guidance and will let you know when the worker responds.",
      },
    };
    const replaced = await app.emit("message_end", {
      message: {
        ...assistant(""),
        content: [call],
        stopReason: "toolUse" as const,
      },
    });
    app.entries.push({
      type: "message",
      id: `assistant-${index}`,
      message: replaced.message,
    });
    const result = await finalTool.execute(
      call.id,
      replaced.message.content[0].arguments,
      undefined,
      undefined,
      app.context,
    );
    app.entries.push({
      type: "message",
      id: `result-${index}`,
      message: {
        role: "toolResult",
        toolName: FINAL_TOOL,
        toolCallId: call.id,
        content: result.content,
        details: result.details,
        isError: false,
        timestamp: 0,
      },
    });
    assert.equal(result.terminate, true);
    await assert.rejects(
      finalTool.execute(
        call.id,
        replaced.message.content[0].arguments,
        undefined,
        undefined,
        app.context,
      ),
      /already completed/,
    );
    await app.emit("tool_result", {
      toolName: FINAL_TOOL,
      toolCallId: call.id,
      input: replaced.message.content[0].arguments,
      content: result.content,
      details: result.details,
      isError: false,
    });
    assert.equal(gate.blockReason("read"), undefined);
    assert.equal(spoken(app.emitted), call.arguments.spoken);
  }
  assert.deepEqual(app.notices, []);
});

test("structured spoken-only response remains available to settled speech", async (t) => {
  const previous = process.env.SCUFRIS_ROLE;
  process.env.SCUFRIS_ROLE = "orchestrator";
  t.after(() =>
    previous === undefined
      ? delete process.env.SCUFRIS_ROLE
      : (process.env.SCUFRIS_ROLE = previous),
  );
  const app = harness();
  const call = {
    type: "toolCall" as const,
    id: "spoken-only-final",
    name: FINAL_TOOL,
    arguments: { spoken: "This prose-only response is ready to speak." },
  };
  const replaced = await app.emit("message_end", {
    message: {
      ...assistant(""),
      content: [call],
      stopReason: "toolUse" as const,
    },
  });
  app.entries.push({
    type: "message",
    id: "assistant-final",
    message: replaced.message,
  });

  const toolResult = await app.tools
    .get(FINAL_TOOL)
    .execute(
      call.id,
      replaced.message.content[0].arguments,
      undefined,
      undefined,
      app.context,
    );
  app.entries.push({
    type: "message",
    id: "tool-result",
    message: {
      role: "toolResult",
      toolCallId: call.id,
      toolName: FINAL_TOOL,
      content: toolResult.content,
      details: toolResult.details,
      isError: false,
      timestamp: 0,
    },
  });

  assert.equal(
    spoken(app.emitted),
    "This prose-only response is ready to speak.",
  );
});

test("artifacts enforce private modes, ownership metadata, bounds, and symlink refusal", () => {
  const { root, sessionFile } = fixture();
  const store = new ArtifactStore({ sessionFile, sessionId: "owner" });
  const id = store.create("# Detail\n");
  assert.equal(lstatSync(store.root).mode & 0o777, 0o700);
  assert.equal(lstatSync(join(store.root, `${id}.md`)).mode & 0o777, 0o600);
  assert.equal(store.read(id).metadata.session_id, "owner");
  assert.throws(
    () => new ArtifactStore({ sessionFile, sessionId: "other" }).read(id),
    /active session/,
  );
  const bad = "a".repeat(24);
  symlinkSync(join(store.root, `${id}.md`), join(store.root, `${bad}.md`));
  writeFileSync(join(store.root, `${bad}.json`), "{}", { mode: 0o600 });
  assert.throws(() => store.read(bad), /ownership/);

  const boundedSession = join(root, "bounded.jsonl");
  writeFileSync(boundedSession, "{}\n", { mode: 0o600 });
  const bounded = new ArtifactStore({
    sessionFile: boundedSession,
    sessionId: "bounded",
  });
  for (let index = 0; index < maxArtifacts; index += 1) {
    const fake = index.toString(16).padStart(24, "0");
    writeFileSync(join(bounded.root, `${fake}.md`), "x", { mode: 0o600 });
  }
  assert.throws(() => bounded.create("more"), /count limit/);
  assert.throws(
    () => store.create("x".repeat(256 * 1024 + 1)),
    /artifact limit/,
  );
  chmodSync(store.root, 0o755);
  assert.throws(
    () => new ArtifactStore({ sessionFile, sessionId: "owner" }),
    /mode is invalid/,
  );
});

test("detail uses public Plannotator annotate gate and stores compact feedback", async (t) => {
  const previous = process.env.SCUFRIS_ROLE;
  process.env.SCUFRIS_ROLE = "orchestrator";
  t.after(() =>
    previous === undefined
      ? delete process.env.SCUFRIS_ROLE
      : (process.env.SCUFRIS_ROLE = previous),
  );
  const app = harness();
  const store = new ArtifactStore({
    sessionFile: app.context.sessionManager.getSessionFile(),
    sessionId: "session-owned-id",
  });
  const id = store.create("# Review me\n");
  await app.commands.get("detail")!.handler(id, app.context);
  const request = app.emitted[0]!;
  assert.equal(request.channel, "plannotator:request");
  assert.equal(request.request.action, "annotate");
  assert.equal(request.request.payload.gate, true);
  request.request.respond({
    status: "handled",
    result: { feedback: "Change the decision wording.", approved: false },
  });
  assert.equal(app.entries.at(-1)?.customType, "scufris-detail-review-v1");
  assert.equal(app.entries.at(-1)?.data.outcome, "feedback");
  assert.equal(app.messages[0]!.display, false);
  const review = store.read(id).metadata.review as { feedback: string };
  assert.equal(review.feedback, "Change the decision wording.");

  await app.commands.get("detail")!.handler(id, app.context);
  app.emitted.at(-1)!.request.respond({
    status: "handled",
    result: { feedback: "", approved: true },
  });
  assert.equal(app.entries.at(-1)?.data.outcome, "approved");

  await app.commands.get("detail")!.handler(id, app.context);
  app.emitted.at(-1)!.request.respond({
    status: "handled",
    result: { feedback: "", exit: true },
  });
  assert.equal(app.entries.at(-1)?.data.outcome, "closed");
});

test("live fallback and restored structured responses each render once", async (t) => {
  const previous = process.env.SCUFRIS_ROLE;
  process.env.SCUFRIS_ROLE = "orchestrator";
  t.after(() =>
    previous === undefined
      ? delete process.env.SCUFRIS_ROLE
      : (process.env.SCUFRIS_ROLE = previous),
  );
  const app = harness();
  const result = await app.emit("message_end", {
    message: assistant("One visible response.\n\n# Private detail"),
  });
  const liveRows = [
    ...app.entries
      .filter((entry) => entry.customType === RESPONSE_ENTRY)
      .map((entry) => responseText(entry.data)),
    ...result.message.content
      .filter((item: any) => item.type === "text")
      .map((item: any) => item.text),
  ];
  assert.equal(liveRows.length, 1);
  assert.match(
    liveRows[0]!,
    /^One visible response\.\n\n\/detail [a-f0-9]{24}$/,
  );

  const renderer = app.renderers.get(RESPONSE_ENTRY)!;
  const component = renderer(
    {
      data: {
        version: 1,
        spoken: "A restored response stays concise.",
        artifact_id: "0123456789abcdef01234567",
      },
    },
    {},
    {
      fg(_color: string, text: string) {
        return text;
      },
    },
  );
  assert.equal(
    component
      .render(100)
      .map((line: string) => line.trimEnd())
      .join("\n"),
    "A restored response stays concise.\n\n/detail 0123456789abcdef01234567",
  );
});

test("persistence failure preserves spoken output", async (t) => {
  const previous = process.env.SCUFRIS_ROLE;
  process.env.SCUFRIS_ROLE = "orchestrator";
  t.after(() =>
    previous === undefined
      ? delete process.env.SCUFRIS_ROLE
      : (process.env.SCUFRIS_ROLE = previous),
  );
  const app = harness();
  app.context.sessionManager.getSessionFile = () => undefined as never;
  const result = await app.emit("message_end", {
    message: assistant(
      "The spoken result remains available.\n\n# Unsaved detail",
    ),
  });
  assert.equal(
    result.message.content[0].text,
    "The spoken result remains available.",
  );
  assert.match(app.notices.at(-1) ?? "", /detail was not saved/);
  assert.equal(
    app.entries.some((entry) => entry.customType === RESPONSE_ENTRY),
    false,
  );
});

test("prompt inspection composition is exact and direct splitting is deterministic", () => {
  const prompt = assembleScufrisPrompt("Pi base");
  assert.match(prompt, /^Pi base/);
  assert.match(prompt, /foreground pair-programming companion/);
  assert.match(prompt, /Use scufris_final_response/);
  const inspection = promptInspectionMarkdown("effective", {}, []);
  assert.match(inspection, /Embedded canonical Scufris identity policy/);
  assert.match(inspection, /## Embedded canonical identity policy/);
  assert.deepEqual(splitDirectResponse("A safe answer."), {
    spoken: "A safe answer.",
  });
  assert.deepEqual(splitDirectResponse("A safe answer.\n\n- More"), {
    spoken: "A safe answer.",
    detail: "- More",
  });
});
