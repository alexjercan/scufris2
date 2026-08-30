import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { constants } from "node:fs";
import { access, chmod, mkdtemp, readFile, writeFile } from "node:fs/promises";
import {
  createServer,
  type IncomingMessage,
  type ServerResponse,
} from "node:http";
import { tmpdir } from "node:os";
import { delimiter, join } from "node:path";
import test from "node:test";

const helper = new URL("../tools/voice/scufris-speak", import.meta.url)
  .pathname;

interface ProcessResult {
  code: number | null;
  stderr: string;
}

function wav(audio = Buffer.from("fake-audio")): Buffer {
  const output = Buffer.alloc(44 + audio.length);
  output.write("RIFF", 0);
  output.writeUInt32LE(output.length - 8, 4);
  output.write("WAVEfmt ", 8);
  output.writeUInt32LE(16, 16);
  output.writeUInt16LE(1, 20);
  output.writeUInt16LE(1, 22);
  output.writeUInt32LE(22_050, 24);
  output.writeUInt32LE(44_100, 28);
  output.writeUInt16LE(2, 32);
  output.writeUInt16LE(16, 34);
  output.write("data", 36);
  output.writeUInt32LE(audio.length, 40);
  audio.copy(output, 44);
  return output;
}

async function executable(name: string): Promise<string> {
  for (const directory of (process.env.PATH ?? "").split(delimiter)) {
    const candidate = join(directory, name);
    try {
      await access(candidate, constants.X_OK);
      return candidate;
    } catch {
      // Continue through PATH.
    }
  }
  throw new Error(`${name} is unavailable`);
}

async function fakeProgram(path: string, source: string): Promise<void> {
  await writeFile(path, `#!${process.execPath}\n${source}`, "utf8");
  await chmod(path, 0o755);
}

async function fixture() {
  const root = await mkdtemp(join(tmpdir(), "scufris-speak-"));
  const bin = join(root, "bin");
  await import("node:fs/promises").then(({ mkdir }) => mkdir(bin));
  return { root, bin, log: join(root, "log.jsonl") };
}

async function api(
  handler: (request: IncomingMessage, response: ServerResponse) => void,
): Promise<{ endpoint: string; close: () => Promise<void> }> {
  const server = createServer(handler);
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  assert(address && typeof address !== "string");
  return {
    endpoint: `http://127.0.0.1:${address.port}/v1/audio/speech`,
    close: () => new Promise<void>((resolve) => server.close(() => resolve())),
  };
}

async function runHelper(
  input: Buffer | string,
  env: NodeJS.ProcessEnv,
): Promise<ProcessResult> {
  const python = await executable("python3");
  return await new Promise((resolve, reject) => {
    const child = spawn(python, [helper], {
      env,
      stdio: ["pipe", "ignore", "pipe"],
    });
    const stderr: Buffer[] = [];
    child.stderr.on("data", (chunk: Buffer) => stderr.push(chunk));
    child.on("error", reject);
    child.on("close", (code) =>
      resolve({ code, stderr: Buffer.concat(stderr).toString("utf8") }),
    );
    child.stdin.end(input);
  });
}

async function waitFor(
  predicate: () => Promise<boolean>,
  timeoutMs = 3_000,
): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (await predicate()) return;
    await new Promise((resolve) => setTimeout(resolve, 20));
  }
  throw new Error("condition timed out");
}

test("speech helper sends the fixed API request and plays returned WAV", async () => {
  const item = await fixture();
  await fakeProgram(
    join(item.bin, "pw-play"),
    `const fs = require("node:fs");
const chunks = [];
process.stdin.on("data", chunk => chunks.push(chunk));
process.stdin.on("end", () => {
  const audio = Buffer.concat(chunks);
  fs.writeFileSync(process.env.TEST_LOG, JSON.stringify({ argv: process.argv.slice(2), header: audio.subarray(0, 4).toString("ascii"), bytes: audio.length }));
});`,
  );
  let received: {
    method?: string;
    url?: string;
    contentType?: string;
    body?: unknown;
  } = {};
  const server = await api((request, response) => {
    const chunks: Buffer[] = [];
    request.on("data", (chunk: Buffer) => chunks.push(chunk));
    request.on("end", () => {
      received = {
        method: request.method,
        url: request.url,
        contentType: request.headers["content-type"],
        body: JSON.parse(Buffer.concat(chunks).toString("utf8")),
      };
      const audio = wav();
      response.writeHead(200, {
        "content-type": "audio/wav",
        "content-length": audio.length,
      });
      response.end(audio);
    });
  });
  try {
    const result = await runHelper("A safe spoken response.", {
      ...process.env,
      PATH: item.bin,
      TEST_LOG: item.log,
      SCUFRIS_TTS_ENDPOINT: server.endpoint,
      SCUFRIS_TTS_MODEL: "custom-piper",
      SCUFRIS_TTS_VOICE: "custom-voice",
    });
    assert.equal(result.code, 0, result.stderr);
    assert.deepEqual(received, {
      method: "POST",
      url: "/v1/audio/speech",
      contentType: "application/json",
      body: {
        model: "custom-piper",
        voice: "custom-voice",
        input: "A safe spoken response.",
        response_format: "wav",
      },
    });
    assert.deepEqual(JSON.parse(await readFile(item.log, "utf8")), {
      argv: ["-"],
      header: "RIFF",
      bytes: 54,
    });
  } finally {
    await server.close();
  }
});

test("speech helper rejects invalid input and bounded API failures", async () => {
  const item = await fixture();
  const base = { ...process.env, PATH: item.bin };
  assert.equal((await runHelper(Buffer.from([0xff]), base)).code, 2);
  assert.equal((await runHelper("x".repeat(1_001), base)).code, 2);
  assert.equal((await runHelper("Safe response.", base)).code, 2);
  const invalidSetting = await runHelper("Safe response.", {
    ...base,
    SCUFRIS_TTS_ENDPOINT: "http://127.0.0.1:10300/v1/audio/speech",
    SCUFRIS_TTS_MODEL: "not a model",
  });
  assert.equal(invalidSetting.code, 2);
  assert.match(invalidSetting.stderr, /SCUFRIS_TTS_MODEL/);

  let mode = "status";
  const server = await api((_request, response) => {
    if (mode === "status") {
      response.writeHead(503, { "content-type": "application/json" });
      response.end('{"error":{"code":"overloaded"}}');
    } else if (mode === "type") {
      response.writeHead(200, { "content-type": "application/json" });
      response.end("{}");
    } else {
      response.writeHead(200, { "content-type": "audio/wav" });
      response.end("not-wave");
    }
  });
  const env = { ...base, SCUFRIS_TTS_ENDPOINT: server.endpoint };
  try {
    const status = await runHelper("Safe response.", env);
    assert.equal(status.code, 5);
    assert.match(status.stderr, /status 503/);
    mode = "type";
    assert.equal((await runHelper("Safe response.", env)).code, 5);
    mode = "wave";
    const malformed = await runHelper("Safe response.", env);
    assert.equal(malformed.code, 5);
    assert.match(malformed.stderr, /invalid WAV/);
  } finally {
    await server.close();
  }
});

test("helper cancellation stops only its exact playback child", async () => {
  const item = await fixture();
  await fakeProgram(
    join(item.bin, "pw-play"),
    `const fs = require("node:fs");
process.on("SIGTERM", () => {
  fs.appendFileSync(process.env.TEST_LOG, JSON.stringify({ event: "term", pid: process.pid }) + "\\n");
  process.exit(143);
});
fs.appendFileSync(process.env.TEST_LOG, JSON.stringify({ event: "start", pid: process.pid }) + "\\n");
process.stdin.resume();
setInterval(() => {}, 1000);`,
  );
  const server = await api((_request, response) => {
    response.writeHead(200, { "content-type": "audio/wav" });
    response.end(wav());
  });
  const unrelated = spawn(
    process.execPath,
    ["-e", "setInterval(() => {}, 1000)"],
    {
      stdio: "ignore",
    },
  );
  const python = await executable("python3");
  const child = spawn(python, [helper], {
    env: {
      ...process.env,
      PATH: item.bin,
      TEST_LOG: item.log,
      SCUFRIS_TTS_ENDPOINT: server.endpoint,
    },
    stdio: ["pipe", "ignore", "ignore"],
  });
  child.stdin.end("Cancellation stays safe.");
  try {
    await waitFor(async () => {
      try {
        return (await readFile(item.log, "utf8")).includes('"event":"start"');
      } catch {
        return false;
      }
    });
    child.kill("SIGTERM");
    assert.equal(
      await new Promise<number | null>((resolve) =>
        child.once("close", resolve),
      ),
      130,
    );
    const records = (await readFile(item.log, "utf8"))
      .trim()
      .split("\n")
      .map((line) => JSON.parse(line) as { event: string; pid: number });
    assert.deepEqual(
      records.map(({ event }) => event),
      ["start", "term"],
    );
    assert.equal(records[0]?.pid, records[1]?.pid);
    assert.equal(unrelated.exitCode, null);
  } finally {
    if (child.exitCode === null) child.kill("SIGKILL");
    unrelated.kill("SIGTERM");
    await new Promise((resolve) => unrelated.once("close", resolve));
    await server.close();
  }
});
