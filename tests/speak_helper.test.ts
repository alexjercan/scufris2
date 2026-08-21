import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { access, chmod, mkdtemp, readFile, writeFile } from "node:fs/promises";
import { constants } from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, join } from "node:path";
import test from "node:test";
import { OwnedSpeechPlayback } from "../extensions/scufris/speech.ts";

const helper = new URL("../scripts/scufris-speak", import.meta.url).pathname;
const wavBuilder = `function wav(audio = Buffer.from("fake-audio")) {
  const output = Buffer.alloc(44 + audio.length);
  output.write("RIFF", 0);
  output.writeUInt32LE(output.length - 8, 4);
  output.write("WAVEfmt ", 8);
  output.writeUInt32LE(16, 16);
  output.writeUInt16LE(1, 20);
  output.writeUInt16LE(1, 22);
  output.writeUInt32LE(22050, 24);
  output.writeUInt32LE(44100, 28);
  output.writeUInt16LE(2, 32);
  output.writeUInt16LE(16, 34);
  output.write("data", 36);
  output.writeUInt32LE(audio.length, 40);
  audio.copy(output, 44);
  return output;
}`;

interface ProcessResult {
  code: number | null;
  stderr: string;
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

async function fakeProgram(path: string, source: string) {
  await writeFile(path, `#!${process.execPath}\n${source}`, "utf8");
  await chmod(path, 0o755);
}

async function fixture() {
  const root = await mkdtemp(join(tmpdir(), "scufris-speak-"));
  const bin = join(root, "bin");
  await import("node:fs/promises").then(({ mkdir }) => mkdir(bin));
  const model = join(root, "model.onnx");
  const config = join(root, "model.json");
  const log = join(root, "log.jsonl");
  await writeFile(model, "model", "utf8");
  await writeFile(config, "config", "utf8");
  return {
    root,
    bin,
    model,
    config,
    log,
    env: {
      ...process.env,
      PATH: bin,
      SCUFRIS_PIPER_MODEL: model,
      SCUFRIS_PIPER_CONFIG: config,
      TEST_LOG: log,
    },
  };
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

test("private speech helper composes fixed Piper and pw-play arguments", async () => {
  const item = await fixture();
  await fakeProgram(
    join(item.bin, "piper"),
    `const fs = require("node:fs");
${wavBuilder}
const chunks = [];
process.stdin.on("data", chunk => chunks.push(chunk));
process.stdin.on("end", () => {
  const input = Buffer.concat(chunks).toString("utf8");
  fs.appendFileSync(process.env.TEST_LOG, JSON.stringify({ program: "piper", argv: process.argv.slice(2), input }) + "\\n");
  if (input.endsWith("\\n")) process.stdout.write(wav());
});
`,
  );
  await fakeProgram(
    join(item.bin, "pw-play"),
    `const fs = require("node:fs");
const chunks = [];
process.stdin.on("data", chunk => chunks.push(chunk));
process.stdin.on("end", () => {
  const audio = Buffer.concat(chunks);
  fs.appendFileSync(process.env.TEST_LOG, JSON.stringify({ program: "pw-play", argv: process.argv.slice(2), header: audio.subarray(0, 4).toString("ascii"), bytes: audio.length }) + "\\n");
});
`,
  );

  const result = await runHelper("A safe spoken response.", item.env);
  assert.equal(result.code, 0, result.stderr);
  const records = (await readFile(item.log, "utf8"))
    .trim()
    .split("\n")
    .map((line) => JSON.parse(line));
  assert.deepEqual(records, [
    {
      program: "piper",
      argv: [
        "--model",
        item.model,
        "--config",
        item.config,
        "--output_file",
        "-",
      ],
      input: "A safe spoken response.\n",
    },
    { program: "pw-play", argv: ["-"], header: "RIFF", bytes: 54 },
  ]);

  await writeFile(item.log, "", "utf8");
  const withoutConfig: NodeJS.ProcessEnv = { ...item.env };
  delete withoutConfig.SCUFRIS_PIPER_CONFIG;
  assert.equal(
    (await runHelper("Optional config stays optional.", withoutConfig)).code,
    0,
  );
  const noConfigRecord = JSON.parse(
    (await readFile(item.log, "utf8")).split("\n")[0] ?? "",
  );
  assert.deepEqual(noConfigRecord.argv, [
    "--model",
    item.model,
    "--output_file",
    "-",
  ]);
});

test("private speech helper rejects malformed input and fixed runtime failures", async () => {
  const item = await fixture();
  assert.equal((await runHelper(Buffer.from([0xff]), item.env)).code, 2);
  assert.equal((await runHelper("x".repeat(1_001), item.env)).code, 2);
  assert.equal((await runHelper("Safe response.", item.env)).code, 3);

  await fakeProgram(
    join(item.bin, "pw-play"),
    `require("node:fs").appendFileSync(process.env.TEST_LOG, "player-started\\n");`,
  );
  await fakeProgram(join(item.bin, "piper"), `process.stdin.resume();`);
  const empty = await runHelper("Safe response.", item.env);
  assert.equal(empty.code, 5);
  assert.match(empty.stderr, /^speech: Piper returned invalid WAV\n$/);
  await assert.rejects(readFile(item.log, "utf8"));

  await fakeProgram(
    join(item.bin, "piper"),
    `process.stdin.resume(); process.stdin.on("end", () => process.stdout.write("not-wave"));`,
  );
  const malformed = await runHelper("Safe response.", item.env);
  assert.equal(malformed.code, 5);
  assert.match(malformed.stderr, /^speech: Piper returned invalid WAV\n$/);
  await assert.rejects(readFile(item.log, "utf8"));

  await fakeProgram(
    join(item.bin, "piper"),
    `${wavBuilder}\nprocess.stdin.resume(); process.stdin.on("end", () => process.stdout.write(wav()));`,
  );
  await fakeProgram(join(item.bin, "pw-play"), `process.exit(8);`);
  const unavailableAudio = await runHelper("Safe response.", item.env);
  assert.equal(unavailableAudio.code, 6);

  const missingModel = {
    ...item.env,
    SCUFRIS_PIPER_MODEL: join(item.root, "missing"),
  };
  assert.equal((await runHelper("Safe response.", missingModel)).code, 2);
});

test("owned playback replacement cancels only the exact helper process", async () => {
  const item = await fixture();
  const ownedHelper = join(item.root, "owned-helper");
  await fakeProgram(
    ownedHelper,
    `const fs = require("node:fs");
process.stdin.resume();
process.on("SIGTERM", () => {
  fs.appendFileSync(process.env.TEST_LOG, JSON.stringify({ event: "term", pid: process.pid }) + "\\n");
  process.exit(143);
});
fs.appendFileSync(process.env.TEST_LOG, JSON.stringify({ event: "start", pid: process.pid }) + "\\n");
setInterval(() => {}, 1000);
`,
  );
  const originalLog = process.env.TEST_LOG;
  process.env.TEST_LOG = item.log;
  const playback = new OwnedSpeechPlayback(ownedHelper, 10_000);
  const unrelated = spawn(
    process.execPath,
    ["-e", "setInterval(() => {}, 1000)"],
    { stdio: "ignore" },
  );

  try {
    const first = playback.play("First safe response.");
    await waitFor(async () => {
      try {
        return (await readFile(item.log, "utf8")).includes('"event":"start"');
      } catch {
        return false;
      }
    });
    const second = playback.play("Second safe response.");
    await first;
    await waitFor(async () => {
      const records = (await readFile(item.log, "utf8")).split("\n");
      return (
        records.filter((line) => line.includes('"event":"start"')).length === 2
      );
    });
    await playback.cancel();
    await second;

    const records = (await readFile(item.log, "utf8"))
      .trim()
      .split("\n")
      .map((line) => JSON.parse(line) as { event: string; pid: number });
    assert.deepEqual(
      records
        .filter((record) => record.event === "term")
        .map((record) => record.pid),
      records
        .filter((record) => record.event === "start")
        .map((record) => record.pid),
    );
    assert.equal(unrelated.exitCode, null);
  } finally {
    await playback.cancel();
    unrelated.kill("SIGTERM");
    await new Promise((resolve) => unrelated.once("close", resolve));
    if (originalLog === undefined) delete process.env.TEST_LOG;
    else process.env.TEST_LOG = originalLog;
  }
});

test("owned playback enforces a bounded deadline", async () => {
  const item = await fixture();
  const ownedHelper = join(item.root, "timeout-helper");
  await fakeProgram(
    ownedHelper,
    `process.stdin.resume();
process.on("SIGTERM", () => process.exit(143));
setInterval(() => {}, 1000);
`,
  );
  const playback = new OwnedSpeechPlayback(ownedHelper, 25);
  await assert.rejects(
    playback.play("This response reaches the deadline."),
    /Speech playback timed out\./,
  );
  await playback.cancel();
});

test("helper cancellation signals only its exact synthesis and playback child", async () => {
  const item = await fixture();
  const hangingProgram = (name: string) => `const fs = require("node:fs");
process.on("SIGTERM", () => {
  fs.appendFileSync(process.env.TEST_LOG, JSON.stringify({ event: "term", program: "${name}", pid: process.pid }) + "\\n");
  process.exit(143);
});
fs.appendFileSync(process.env.TEST_LOG, JSON.stringify({ event: "start", program: "${name}", pid: process.pid }) + "\\n");
process.stdin.resume();
setInterval(() => {}, 1000);
`;
  await fakeProgram(join(item.bin, "piper"), hangingProgram("piper"));

  const unrelated = spawn(
    process.execPath,
    ["-e", "setInterval(() => {}, 1000)"],
    { stdio: "ignore" },
  );
  const python = await executable("python3");
  const helpers: ReturnType<typeof spawn>[] = [];
  const cancelDuring = async (program: string) => {
    const child = spawn(python, [helper], {
      env: item.env,
      stdio: ["pipe", "ignore", "ignore"],
    });
    helpers.push(child);
    child.stdin.end("Cancellation stays safe.");
    await waitFor(async () => {
      try {
        return (await readFile(item.log, "utf8")).includes(
          `"event":"start","program":"${program}"`,
        );
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
  };

  try {
    await cancelDuring("piper");
    await fakeProgram(
      join(item.bin, "piper"),
      `${wavBuilder}\nprocess.stdin.resume(); process.stdin.on("end", () => process.stdout.write(wav()));`,
    );
    await fakeProgram(join(item.bin, "pw-play"), hangingProgram("pw-play"));
    await cancelDuring("pw-play");

    const records = (await readFile(item.log, "utf8"))
      .trim()
      .split("\n")
      .map(
        (line) =>
          JSON.parse(line) as { event: string; program: string; pid: number },
      );
    const starts = new Map(
      records
        .filter((record) => record.event === "start")
        .map((record) => [record.program, record.pid]),
    );
    const terms = new Map(
      records
        .filter((record) => record.event === "term")
        .map((record) => [record.program, record.pid]),
    );
    assert.deepEqual(terms, starts);
    assert.equal(unrelated.exitCode, null);
  } finally {
    unrelated.kill("SIGTERM");
    await new Promise((resolve) => unrelated.once("close", resolve));
    for (const child of helpers) {
      if (child.exitCode !== null) continue;
      child.kill("SIGKILL");
      await new Promise((resolve) => child.once("close", resolve));
    }
  }
});
