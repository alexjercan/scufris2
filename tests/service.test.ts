import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createServer } from "node:net";
import test from "node:test";
import {
  decodeAgentResponse,
  encodeAgentRequest,
  MAX_DETAIL_BYTES,
  stateDetail,
  surfacePrompt,
  takeLines,
} from "../agent/extensions/scufris/service/protocol.ts";
import {
  AgentClient,
  UPDATE_TOGETHER,
  type AgentWake,
} from "../agent/extensions/scufris/service/client.ts";
import { resolveSocketPath } from "../agent/extensions/scufris/service/index.ts";

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
      v: 6,
      type: "agent.response",
      text: "Done.",
      details,
    });
  assert.throws(() => response("bad\rline"), /details is invalid/);
  assert.throws(() => response("bad\0line"), /details is invalid/);
  assert.throws(() => response("   "), /details is invalid/);
  assert.throws(
    () =>
      encodeAgentRequest({
        v: 6,
        type: "agent.response",
        text: "one\rtwo",
      }),
    /text is invalid/,
  );
  // A state detail was never validated at all, and one carriage return from a
  // worker's captured output was enough to close the channel.
  const state = (detail: string) =>
    encodeAgentRequest({
      v: 6,
      type: "agent.state",
      state: "failed",
      detail,
    });
  assert.throws(() => state("job\rfailed"), /detail is invalid/);
  assert.throws(() => state("x".repeat(MAX_DETAIL_BYTES + 1)), /invalid/);
  // Empty is the cleared state and stays legal.
  assert.ok(state(""));
});

test("a state detail is clamped rather than lost", () => {
  assert.equal(stateDetail("job\rfailed\0here"), "job failed here");
  const long = stateDetail("x".repeat(MAX_DETAIL_BYTES * 2));
  assert.equal(Buffer.byteLength(long, "utf8"), MAX_DETAIL_BYTES);
  // A cut that splits a codepoint must not leave a replacement character
  // behind, or the clamped detail is refused for a different reason.
  const wide = stateDetail("é".repeat(MAX_DETAIL_BYTES));
  assert.ok(Buffer.byteLength(wide, "utf8") <= MAX_DETAIL_BYTES);
  assert.doesNotMatch(wide, /�/);
  assert.ok(
    encodeAgentRequest({
      v: 6,
      type: "agent.state",
      state: "failed",
      detail: stateDetail(`${"x".repeat(MAX_DETAIL_BYTES * 2)}\rmore`),
    }),
  );
});

test("agent v6 messages are bounded and channel-specific", () => {
  assert.equal(
    encodeAgentRequest({ v: 6, type: "agent.hello" }),
    '{"v":6,"type":"agent.hello"}\n',
  );
  assert.deepEqual(
    decodeAgentResponse(
      '{"v":6,"type":"agent.message","id":"m-1","text":"hello","widgets":[]}',
    ),
    {
      v: 6,
      type: "agent.message",
      id: "m-1",
      text: "hello",
      widgets: [],
      attachments: [],
    },
  );
  assert.throws(() => decodeAgentResponse('{"v":5,"type":"agent.ready"}'));
  assert.throws(() =>
    decodeAgentResponse('{"v":6,"type":"surface.ready","surface":"desk"}'),
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
      v: 6,
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
          v: 6,
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
  const root = await mkdtemp(join(tmpdir(), "scufris-agent-v6-"));
  const socketPath = join(root, "agent.sock");
  const server = createServer((socket) => {
    socket.once("data", () => {
      socket.write('{"v":6,"type":"agent.ready"}\n');
      socket.write(
        '{"v":6,"type":"agent.message","id":"m-1","text":"hello","widgets":[]}\n',
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
      JSON.stringify({ v: 6, type: "agent.wake", ...fields }),
    );
  assert.deepEqual(
    wake({
      custom_type: "scufris-briefing",
      text: "The briefing is collected.",
      details: { profile: "morning" },
    }),
    {
      v: 6,
      type: "agent.wake",
      custom_type: "scufris-briefing",
      text: "The briefing is collected.",
      details: { profile: "morning" },
    },
  );
  assert.deepEqual(wake({ custom_type: "scufris-wake", text: "Wake up." }), {
    v: 6,
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

test("a wake becomes a follow-up, never a user message", async () => {
  const root = await mkdtemp(join(tmpdir(), "scufris-agent-wake-"));
  const socketPath = join(root, "agent.sock");
  const server = createServer((socket) => {
    socket.once("data", () => {
      socket.write('{"v":6,"type":"agent.ready"}\n');
      socket.write(
        '{"v":6,"type":"agent.wake","custom_type":"scufris-briefing","text":"Wake up.","details":{"profile":"morning"}}\n',
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

test("agent socket resolution has no service socket fallback", () => {
  assert.equal(
    resolveSocketPath({ SCUFRIS_RUNTIME_DIR: "/run/scufris" }),
    "/run/scufris/agent.sock",
  );
  assert.equal(resolveSocketPath({}), undefined);
});
