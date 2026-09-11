import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { createServer } from "node:net";
import test from "node:test";
import {
  decodeAgentResponse,
  encodeAgentRequest,
  jobSummary,
  MAX_BADGE_BYTES,
  MAX_DETAILS_BYTES,
  MAX_JOB_ROWS,
  MAX_JOB_SUMMARY_BYTES,
  REFUSAL,
  surfacePrompt,
  takeLines,
} from "../agent/extensions/scufris/service/protocol.ts";
import {
  AGENT_RESPONSE_EVENT,
  AgentClient,
  UPDATE_TOGETHER,
  type AgentClientOptions,
  type AgentWake,
  type AtomicResponse,
} from "../agent/extensions/scufris/service/client.ts";
import {
  bindService,
  proactiveIdFromMessage,
  proactiveMessageDetails,
  resolveSocketPath,
} from "../agent/extensions/scufris/service/index.ts";
import type {
  ExtensionAPI,
  ExtensionContext,
} from "@earendil-works/pi-coding-agent";

const widget = {
  name: "summary",
  description: "Show a summary <safely>.",
  input_schema: { type: "object", properties: { passed: { type: "integer" } } },
};

test("the encoder holds the host's content rules so a violation is not a teardown", () => {
  // shared/control/src/service.rs `text()` refuses NUL, CR, and a value that
  // trims to empty. The host answers an invalid submission by closing the
  // agent connection without a refusal, so anything this side lets through
  // costs the answer in flight and says nothing. These have to match.
  const response = (details: string) =>
    encodeAgentRequest({
      v: 10,
      type: "agent.response",
      text: "Done.",
      details,
    });
  assert.throws(() => response("bad\rline"), /details is invalid/);
  assert.throws(() => response("bad\0line"), /details is invalid/);
  assert.throws(() => response("   "), /details is invalid/);
  assert.throws(() => response("x".repeat(MAX_DETAILS_BYTES + 1)), /invalid/);
  assert.throws(
    () =>
      encodeAgentRequest({
        v: 10,
        type: "agent.response",
        text: "one\rtwo",
      }),
    /text is invalid/,
  );
  // A row summary was never validated at all, and one carriage return from a
  // worker's captured output was enough to close the channel.
  const listed = (summary: string) =>
    encodeAgentRequest({
      v: 10,
      type: "agent.jobs",
      jobs: [
        {
          id: "01ccbac98b97",
          state: "failed",
          since: 1_757_000_000,
          summary,
        },
      ],
    });
  assert.throws(() => listed("job\rfailed"), /job_summary is invalid/);
  assert.throws(() => listed("x".repeat(MAX_JOB_SUMMARY_BYTES + 1)), /invalid/);
  // A job that has said nothing yet is a row with no summary, and that row is
  // worth drawing: it says the job started.
  assert.ok(listed(""));
});

test("a job row summary is clamped rather than lost", () => {
  assert.equal(jobSummary("job\rfailed\0here"), "job failed here");
  const long = jobSummary("x".repeat(MAX_JOB_SUMMARY_BYTES * 2));
  assert.equal(Buffer.byteLength(long, "utf8"), MAX_JOB_SUMMARY_BYTES);
  // A cut that splits a codepoint must not leave a replacement character
  // behind, or the clamped summary is refused for a different reason.
  const wide = jobSummary("é".repeat(MAX_JOB_SUMMARY_BYTES));
  assert.ok(Buffer.byteLength(wide, "utf8") <= MAX_JOB_SUMMARY_BYTES);
  assert.doesNotMatch(wide, /�/);
  assert.ok(
    encodeAgentRequest({
      v: 10,
      type: "agent.jobs",
      jobs: [
        {
          id: "01ccbac98b97",
          state: "failed",
          since: 1_757_000_000,
          summary: jobSummary(`${"x".repeat(MAX_JOB_SUMMARY_BYTES * 2)}\rmore`),
        },
      ],
    }),
  );
});

test("badges are grouped by job and an offer names one offer in the message", () => {
  const badge = { label: "landed", value: "yes", state: "measured" } as const;
  const cited = (job: string, offer: string) => ({
    job_id: job,
    badges: [badge],
    offers: [{ id: offer, label: "push master" }],
  });
  assert.ok(
    encodeAgentRequest({
      v: 10,
      type: "agent.response",
      text: "Both finished.",
      receipts: [
        cited("750a4de8a80d", "offer-a"),
        cited("01ccbac98b97", "offer-b"),
      ],
    }),
  );
  const refused = (receipts: ReturnType<typeof cited>[]) =>
    encodeAgentRequest({
      v: 10,
      type: "agent.response",
      text: "Both finished.",
      receipts,
    });
  // One group per job: two strips under one message with the same label on
  // both leaves no reading that says which badge belongs where.
  assert.throws(
    () =>
      refused([
        cited("750a4de8a80d", "offer-a"),
        cited("750a4de8a80d", "offer-b"),
      ]),
    /duplicate citation/,
  );
  // The identifier is all a surface sends back, so it names one offer in the
  // whole message and not one inside its own group.
  assert.throws(
    () =>
      refused([
        cited("750a4de8a80d", "offer-a"),
        cited("01ccbac98b97", "offer-a"),
      ]),
    /duplicate offer/,
  );
  assert.throws(
    () =>
      refused([
        {
          job_id: "750a4de8a80d",
          badges: [
            {
              ...badge,
              value: "x".repeat(MAX_BADGE_BYTES + 1) as unknown as "yes",
            },
          ],
          offers: [],
        },
      ]),
    /receipt_value is invalid/,
  );
});

test("a job list is bounded and every row names one job", () => {
  const row = (id: string) => ({
    id,
    project: "personal/scufris2",
    state: "working" as const,
    since: 1_757_000_000,
    summary: "reviewing G1",
  });
  // A project identifier is a relative path, so it keeps the slashes an
  // identifier never may.
  assert.ok(
    encodeAgentRequest({
      v: 10,
      type: "agent.jobs",
      jobs: [row("3f81c204b1e9")],
    }),
  );
  const refused = (jobs: ReturnType<typeof row>[]) =>
    encodeAgentRequest({ v: 10, type: "agent.jobs", jobs });
  assert.throws(
    () => refused([row("3f81c204b1e9"), row("3f81c204b1e9")]),
    /duplicate job row/,
  );
  assert.throws(
    () =>
      refused(
        Array.from({ length: MAX_JOB_ROWS + 1 }, (_value, index) =>
          row(`3f81c204b1e${index}`),
        ),
      ),
    /too many job rows/,
  );
});

test("agent v10 messages are bounded and channel-specific", () => {
  assert.equal(
    encodeAgentRequest({ v: 10, type: "agent.hello" }),
    '{"v":10,"type":"agent.hello"}\n',
  );
  assert.equal(
    encodeAgentRequest({
      v: 10,
      type: "agent.proactive_started",
      proactive_id: "briefing-generation-a-terminal",
    }),
    '{"v":10,"type":"agent.proactive_started","proactive_id":"briefing-generation-a-terminal"}\n',
  );
  assert.throws(() =>
    encodeAgentRequest({
      v: 10,
      type: "agent.proactive_started",
      proactive_id: "not an identifier",
    }),
  );
  assert.deepEqual(
    decodeAgentResponse(
      '{"v":10,"type":"agent.message","id":"m-1","text":"hello","widgets":[]}',
    ),
    {
      v: 10,
      type: "agent.message",
      id: "m-1",
      text: "hello",
      widgets: [],
      attachments: [],
    },
  );
  assert.throws(() => decodeAgentResponse('{"v":6,"type":"agent.ready"}'));
  assert.throws(() =>
    decodeAgentResponse('{"v":10,"type":"surface.ready","surface":"desk"}'),
  );
});

test("attachment descriptors are strict and reach the surface prompt", () => {
  const descriptor = {
    id: "att_0123456789",
    name: "diagram.png",
    media_type: "image/png",
    size: 184_223,
  };
  const message = decodeAgentResponse(
    JSON.stringify({
      v: 10,
      type: "agent.message",
      id: "m-1",
      text: "See it.",
      widgets: [],
      attachments: [descriptor],
    }),
  );
  assert.equal(message.type, "agent.message");
  if (message.type !== "agent.message") return;
  assert.deepEqual(message.attachments, [descriptor]);
  assert.match(
    surfacePrompt(message.text, [], message.attachments),
    /diagram\.png/,
  );
  for (const attachment of [
    { ...descriptor, name: "../secret" },
    { ...descriptor, media_type: "image png" },
    { ...descriptor, size: 16 * 1024 * 1024 + 1 },
  ]) {
    assert.throws(() =>
      decodeAgentResponse(
        JSON.stringify({
          v: 10,
          type: "agent.message",
          id: "m-1",
          text: "See it.",
          widgets: [],
          attachments: [attachment],
        }),
      ),
    );
  }
});

test("surface prompts are deterministic, self-contained, and XML-safe", () => {
  const first = surfacePrompt("Use </user_message> & continue.", [widget], []);
  const second = surfacePrompt("Use </user_message> & continue.", [widget], []);
  assert.equal(first, second);
  assert.match(first, /^<scufris_surface_message>/);
  assert.match(first, /<widgets>/);
  assert.match(first, /<attachments>/);
  assert.match(first, /<user_message>/);
  assert.doesNotMatch(first, /<\/user_message> & continue/);
  assert.match(first, /\\u003c\/user_message\\u003e \\u0026 continue/);
});

test("framing retains partial lines and rejects oversized input", () => {
  assert.deepEqual(takeLines("one\ntwo"), { lines: ["one"], rest: "two" });
  assert.throws(() => takeLines("x".repeat(64 * 1024)));
});

test("the agent client sends messages through sendUserMessage and steers while busy", async () => {
  const root = await mkdtemp(join(tmpdir(), "scufris-agent-v10-"));
  const socketPath = join(root, "agent.sock");
  const server = createServer((socket) => {
    socket.once("data", () => {
      socket.write('{"v":10,"type":"agent.ready"}\n');
      socket.write(
        '{"v":10,"type":"agent.message","id":"m-1","text":"hello","widgets":[]}\n',
      );
    });
  });
  await new Promise<void>((resolve) => server.listen(socketPath, resolve));
  const received = new Promise<{ text: string; busy: boolean }>((resolve) => {
    const client = new AgentClient({
      socketPath,
      busy: () => true,
      abort() {},
      wake() {},
      jobCommand() {},
      offerTake() {},
      sendUserMessage: (text, busy) => {
        resolve({ text, busy });
        client.stop();
      },
    });
    client.start();
  });
  const message = await received;
  assert.equal(message.busy, true);
  assert.match(message.text, /"hello"/);
  await new Promise<void>((resolve) => server.close(() => resolve()));
  await rm(root, { recursive: true, force: true });
});

test("handshake EOF produces the local update-together message", async () => {
  const root = await mkdtemp(join(tmpdir(), "scufris-agent-eof-"));
  const socketPath = join(root, "agent.sock");
  const server = createServer((socket) => socket.destroy());
  await new Promise<void>((resolve) => server.listen(socketPath, resolve));
  const seen = new Promise<string>((resolve) => {
    const client = new AgentClient({
      socketPath,
      busy: () => false,
      abort() {},
      wake() {},
      jobCommand() {},
      offerTake() {},
      sendUserMessage() {},
      log(message) {
        resolve(message);
        client.stop();
      },
    });
    client.start();
  });
  assert.equal(await seen, UPDATE_TOGETHER);
  await new Promise<void>((resolve) => server.close(() => resolve()));
  await rm(root, { recursive: true, force: true });
});

test("a wake is decoded under its own kind with bounded details", () => {
  const wake = (fields: Record<string, unknown>) =>
    decodeAgentResponse(
      JSON.stringify({ v: 10, type: "agent.wake", ...fields }),
    );
  assert.deepEqual(
    wake({
      custom_type: "scufris-briefing",
      text: "The briefing is collected.",
      details: { profile: "morning" },
    }),
    {
      v: 10,
      type: "agent.wake",
      custom_type: "scufris-briefing",
      text: "The briefing is collected.",
      details: { profile: "morning" },
    },
  );
  assert.deepEqual(wake({ custom_type: "scufris-wake", text: "Wake up." }), {
    v: 10,
    type: "agent.wake",
    custom_type: "scufris-wake",
    text: "Wake up.",
  });
  for (const invalid of [
    { custom_type: "not an identifier", text: "Wake up." },
    { custom_type: "scufris-wake", text: "" },
    { custom_type: "scufris-wake", text: "x".repeat(8 * 1024 + 1) },
    { custom_type: "scufris-wake", text: "Wake up.", details: "a string" },
    { custom_type: "scufris-wake", text: "Wake up.", details: [1, 2] },
    {
      custom_type: "scufris-wake",
      text: "Wake up.",
      details: { note: "x".repeat(32 * 1024) },
    },
  ])
    assert.throws(() => wake(invalid));
});

test("proactive correlation stays on its exact queued custom message", () => {
  const details = proactiveMessageDetails("briefing-generation-a-terminal", {
    profile: "morning",
    generation: "generation-a",
  });
  assert.equal(details.profile, "morning");
  assert.equal(details.generation, "generation-a");
  const queued = {
    role: "custom",
    customType: "scufris-briefing",
    content: "Wake up.",
    details,
  };
  const unrelated = {
    role: "custom",
    customType: "scufris-workflow",
    content: "Another queued follow-up.",
    details: { generation: "generation-b" },
  };
  assert.equal(proactiveIdFromMessage(unrelated), undefined);
  assert.equal(
    proactiveIdFromMessage(queued),
    "briefing-generation-a-terminal",
  );
  assert.equal(
    proactiveIdFromMessage({ ...queued, role: "assistant" }),
    undefined,
  );
});

test("the service binding correlates and settles the exact Pi follow-up", () => {
  const handlers = new Map<
    string,
    (event?: unknown, context?: unknown) => void
  >();
  const bus = new Map<string, (value: unknown) => void>();
  const sent: Array<{ message: unknown; options: unknown }> = [];
  const calls: string[] = [];
  let clientOptions: AgentClientOptions | undefined;
  const client = {
    start: () => calls.push("start"),
    stop: () => calls.push("stop"),
    jobs: () => calls.push("jobs"),
    response: (response: AtomicResponse, proactiveId?: string) =>
      calls.push(`response:${response.text}:${proactiveId ?? "none"}`),
    proactiveStarted: (proactiveId: string) =>
      calls.push(`started:${proactiveId}`),
    proactiveSettled: (proactiveId: string) =>
      calls.push(`settled:${proactiveId}`),
  };
  const pi = {
    registerTool() {},
    on(name: string, handler: (event?: unknown, context?: unknown) => void) {
      handlers.set(name, handler);
    },
    events: {
      on(name: string, handler: (value: unknown) => void) {
        bus.set(name, handler);
      },
      emit() {},
    },
    sendMessage(message: unknown, options: unknown) {
      sent.push({ message, options });
    },
    sendUserMessage() {},
  } as unknown as ExtensionAPI;
  bindService(pi, {
    role: "orchestrator",
    socketPath: "/tmp/scufris-test-agent.sock",
    createClient(options) {
      clientOptions = options;
      return client;
    },
  });
  const context = {
    hasUI: false,
    isIdle: () => true,
    abort() {},
  } as unknown as ExtensionContext;
  handlers.get("session_start")?.({}, context);
  assert.equal(calls[0], "start");
  assert.ok(clientOptions);

  const proactiveId = "briefing-generation-a-terminal";
  clientOptions.wake({
    proactiveId,
    customType: "scufris-briefing",
    content: "Wake up.",
  });
  // A reconnect can redeliver the host wake while the exact Pi message remains
  // queued. It must not make a second model turn.
  clientOptions.wake({
    proactiveId,
    customType: "scufris-briefing",
    content: "Wake up.",
  });
  assert.equal(sent.length, 1);
  assert.deepEqual(sent[0]?.options, {
    deliverAs: "followUp",
    triggerTurn: true,
  });
  const queued = sent[0]?.message as Record<string, unknown>;
  handlers.get("message_end")?.({ message: { role: "custom", ...queued } });
  bus.get(AGENT_RESPONSE_EVENT)?.({ text: "The briefing." });
  handlers.get("agent_settled")?.();
  assert.deepEqual(calls.slice(1), [
    `started:${proactiveId}`,
    `response:The briefing.:${proactiveId}`,
    `settled:${proactiveId}`,
  ]);
  handlers.get("session_shutdown")?.();
  assert.equal(calls.at(-1), "stop");
});

test("an undelivered Pi follow-up is settled by exact queued identity", () => {
  const handlers = new Map<
    string,
    (event?: unknown, context?: unknown) => void
  >();
  let clientOptions: AgentClientOptions | undefined;
  const settled: string[] = [];
  const pi = {
    registerTool() {},
    on(name: string, handler: (event?: unknown, context?: unknown) => void) {
      handlers.set(name, handler);
    },
    events: { on() {}, emit() {} },
    sendMessage() {},
    sendUserMessage() {},
  } as unknown as ExtensionAPI;
  bindService(pi, {
    role: "orchestrator",
    socketPath: "/tmp/scufris-test-agent.sock",
    createClient(options) {
      clientOptions = options;
      return {
        start() {},
        stop() {},
        jobs() {},
        response() {},
        proactiveStarted() {},
        proactiveSettled: (id: string) => settled.push(id),
      };
    },
  });
  handlers.get("session_start")?.(
    {},
    { hasUI: false, isIdle: () => true, abort() {} },
  );
  const proactiveId = "briefing-generation-a-terminal";
  assert.ok(clientOptions);
  clientOptions.wake({
    proactiveId,
    customType: "scufris-briefing",
    content: "Wake up.",
  });
  // No message_end arrived. Pi's settled boundary says the follow-up queue is
  // empty, so the host receives the marker that releases and retries its slot.
  handlers.get("agent_settled")?.();
  assert.deepEqual(settled, [proactiveId]);
});

test("a wake becomes a follow-up, never a user message", async () => {
  const root = await mkdtemp(join(tmpdir(), "scufris-agent-wake-"));
  const socketPath = join(root, "agent.sock");
  const server = createServer((socket) => {
    socket.once("data", () => {
      socket.write('{"v":10,"type":"agent.ready"}\n');
      socket.write(
        '{"v":10,"type":"agent.wake","custom_type":"scufris-briefing","text":"Wake up.","details":{"profile":"morning"}}\n',
      );
    });
  });
  await new Promise<void>((resolve) => server.listen(socketPath, resolve));
  let userMessages = 0;
  const received = await new Promise<AgentWake>((resolve) => {
    const client = new AgentClient({
      socketPath,
      busy: () => false,
      abort() {},
      jobCommand() {},
      offerTake() {},
      sendUserMessage: () => {
        userMessages += 1;
      },
      wake: (value) => {
        resolve(value);
        client.stop();
      },
    });
    client.start();
  });
  assert.deepEqual(received, {
    customType: "scufris-briefing",
    content: "Wake up.",
    details: { profile: "morning" },
  });
  assert.equal(userMessages, 0);
  await new Promise<void>((resolve) => server.close(() => resolve()));
  await rm(root, { recursive: true, force: true });
});

test("a durable wake is correlated only by the explicit turn identity", async () => {
  const root = await mkdtemp(join(tmpdir(), "scufris-agent-proactive-"));
  const socketPath = join(root, "agent.sock");
  const responses = new Promise<Record<string, unknown>[]>((resolve) => {
    const received: Record<string, unknown>[] = [];
    const server = createServer((socket) => {
      let buffer = "";
      socket.setEncoding("utf8");
      socket.on("data", (chunk: string) => {
        buffer += chunk;
        const lines = buffer.split("\n");
        buffer = lines.pop() ?? "";
        for (const line of lines) {
          const message = JSON.parse(line) as Record<string, unknown>;
          if (message.type === "agent.hello") {
            socket.write('{"v":10,"type":"agent.ready"}\n');
            socket.write(
              '{"v":10,"type":"agent.wake","proactive_id":"briefing-generation-a-terminal","custom_type":"scufris-briefing","text":"Wake up."}\n',
            );
          } else if (
            message.type === "agent.proactive_started" ||
            message.type === "agent.proactive_settled" ||
            message.type === "agent.response"
          ) {
            received.push(message);
            if (received.length === 4) resolve(received);
          }
        }
      });
    });
    void new Promise<void>((listening) =>
      server.listen(socketPath, listening),
    ).then(() => {
      const client = new AgentClient({
        socketPath,
        busy: () => false,
        abort() {},
        jobCommand() {},
        offerTake() {},
        sendUserMessage() {},
        wake(value) {
          assert.equal(value.proactiveId, "briefing-generation-a-terminal");
          client.proactiveStarted(value.proactiveId);
          client.response({ text: "An unrelated turn." });
          client.response({ text: "The briefing." }, value.proactiveId);
          client.proactiveSettled(value.proactiveId);
        },
      });
      client.start();
      void responses.finally(() => {
        client.stop();
        server.close();
      });
    });
  });
  const sent = await responses;
  assert.equal(sent[0]?.type, "agent.proactive_started");
  assert.equal(sent[0]?.proactive_id, "briefing-generation-a-terminal");
  assert.equal(sent[1]?.proactive_id, undefined);
  assert.equal(sent[1]?.text, "An unrelated turn.");
  assert.equal(sent[2]?.proactive_id, "briefing-generation-a-terminal");
  assert.equal(sent[2]?.text, "The briefing.");
  assert.equal(sent[3]?.type, "agent.proactive_settled");
  assert.equal(sent[3]?.proactive_id, "briefing-generation-a-terminal");
  await rm(root, { recursive: true, force: true });
});

test("agent socket resolution has no service socket fallback", () => {
  assert.equal(
    resolveSocketPath({ SCUFRIS_RUNTIME_DIR: "/run/scufris" }),
    "/run/scufris/agent.sock",
  );
  assert.equal(resolveSocketPath({}), undefined);
});

// The refusal vocabulary is implemented twice, and neither compiler sees the
// other. A code changed on one side only is a match that stops matching, on the
// one path nobody exercises by hand, so the two lists are read and compared.
test("every refusal code is named the same on both sides", () => {
  const module = readFileSync(
    resolve(
      new URL("..", import.meta.url).pathname,
      "shared/control/src/refusal.rs",
    ),
    "utf8",
  );
  const host = new Map<string, string>();
  for (const line of module.split("\n")) {
    const declared = /^pub const ([A-Z_]+): &str = "([a-z_]+)";$/.exec(line);
    if (declared) host.set(declared[1]!, declared[2]!);
  }
  assert.ok(host.size > 0, "no refusal codes were read from the Rust module");
  assert.deepEqual(
    Object.fromEntries([...host].sort()),
    Object.fromEntries(Object.entries(REFUSAL).sort()),
  );
});
