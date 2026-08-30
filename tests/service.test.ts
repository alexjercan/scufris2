import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createServer } from "node:net";
import test from "node:test";
import {
  decodeAgentResponse,
  encodeAgentRequest,
  surfacePrompt,
  takeLines,
} from "../agent/extensions/scufris/service/protocol.ts";
import {
  AgentClient,
  UPDATE_TOGETHER,
} from "../agent/extensions/scufris/service/client.ts";
import { resolveSocketPath } from "../agent/extensions/scufris/service/index.ts";

const widget = {
  name: "summary",
  description: "Show a summary <safely>.",
  input_schema: { type: "object", properties: { passed: { type: "integer" } } },
};

test("agent v5 messages are bounded and channel-specific", () => {
  assert.equal(
    encodeAgentRequest({ v: 5, type: "agent.hello" }),
    '{"v":5,"type":"agent.hello"}\n',
  );
  assert.deepEqual(
    decodeAgentResponse(
      '{"v":5,"type":"agent.message","id":"m-1","text":"hello","widgets":[]}',
    ),
    {
      v: 5,
      type: "agent.message",
      id: "m-1",
      text: "hello",
      widgets: [],
      attachments: [],
    },
  );
  assert.throws(() => decodeAgentResponse('{"v":4,"type":"agent.ready"}'));
  assert.throws(() =>
    decodeAgentResponse('{"v":5,"type":"surface.ready","surface":"desk"}'),
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
      v: 5,
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
          v: 5,
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
  const root = await mkdtemp(join(tmpdir(), "scufris-agent-v5-"));
  const socketPath = join(root, "agent.sock");
  const server = createServer((socket) => {
    socket.once("data", () => {
      socket.write('{"v":5,"type":"agent.ready"}\n');
      socket.write(
        '{"v":5,"type":"agent.message","id":"m-1","text":"hello","widgets":[]}\n',
      );
    });
  });
  await new Promise<void>((resolve) => server.listen(socketPath, resolve));
  const received = new Promise<{ text: string; busy: boolean }>((resolve) => {
    const client = new AgentClient({
      socketPath,
      busy: () => true,
      abort() {},
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

test("agent socket resolution has no service socket fallback", () => {
  assert.equal(
    resolveSocketPath({ SCUFRIS_RUNTIME_DIR: "/run/scufris" }),
    "/run/scufris/agent.sock",
  );
  assert.equal(resolveSocketPath({}), undefined);
});
