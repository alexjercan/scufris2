import assert from "node:assert/strict";
import {
  chmod,
  mkdir,
  mkdtemp,
  readFile,
  stat,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawn } from "node:child_process";
import test from "node:test";

const root = resolve(new URL("..", import.meta.url).pathname);
const helper = join(root, "scripts", "scufris-dev");

interface Result {
  code: number | null;
  stdout: string;
  stderr: string;
}

async function executable(path: string, source: string): Promise<void> {
  await writeFile(path, source, "utf8");
  await chmod(path, 0o755);
}

async function fixture() {
  const directory = await mkdtemp(join(tmpdir(), "scufris-dev-"));
  const bin = join(directory, "bin");
  const state = join(directory, "state");
  await mkdir(bin);
  await executable(
    join(bin, "pi"),
    `#!${process.execPath}\nconsole.log(JSON.stringify({ argv: process.argv.slice(2), role: process.env.SCUFRIS_ROLE ?? null, speech: process.env.SCUFRIS_SPEECH ?? null, calm: process.env.SCUFRIS_CALM ?? null, model: process.env.SCUFRIS_PIPER_MODEL ?? null, config: process.env.SCUFRIS_PIPER_CONFIG ?? null, roots: process.env.SCUFRIS_PROJECT_ROOTS ?? null, stt: process.env.PI_STT_CONFIG ?? null, endpoint: process.env.TEST_STT_ENDPOINT ?? null }));\n`,
  );
  const env: NodeJS.ProcessEnv = {
    ...process.env,
    HOME: join(directory, "home"),
    XDG_STATE_HOME: state,
    PATH: `${join(root, "node_modules", ".bin")}:${bin}:${process.env.PATH ?? ""}`,
    SCUFRIS_ROLE: "worker",
    PI_STT_CONFIG: "/global/pi-voice-stt.json",
    TEST_STT_ENDPOINT: "inherited-endpoint",
  };
  delete env.SCUFRIS_PROJECT_ROOTS;
  delete env.SCUFRIS_DEV_VOICE;
  delete env.SCUFRIS_PIPER_MODEL;
  delete env.SCUFRIS_PIPER_CONFIG;
  delete env.SCUFRIS_SPEECH;
  delete env.SCUFRIS_CALM;
  return { directory, bin, state, env };
}

async function run(
  args: string[],
  env: NodeJS.ProcessEnv,
  cwd: string,
): Promise<Result> {
  return await new Promise((resolveResult, reject) => {
    const child = spawn(helper, args, {
      cwd,
      env,
      stdio: ["ignore", "pipe", "pipe"],
    });
    const stdout: Buffer[] = [];
    const stderr: Buffer[] = [];
    child.stdout.on("data", (chunk: Buffer) => stdout.push(chunk));
    child.stderr.on("data", (chunk: Buffer) => stderr.push(chunk));
    child.on("error", reject);
    child.on("close", (code) =>
      resolveResult({
        code,
        stdout: Buffer.concat(stdout).toString("utf8"),
        stderr: Buffer.concat(stderr).toString("utf8"),
      }),
    );
  });
}

function expectedArgs(sessionDirectory: string, voice: boolean): string[] {
  const extensions = [
    "identity.ts",
    "response.ts",
    "calm.ts",
    ...(voice ? ["speech.ts"] : []),
    "agents.ts",
    "widgets.ts",
  ];
  return [
    ...extensions.flatMap((name) => [
      "--extension",
      join(root, "extensions", "scufris", name),
    ]),
    "--skill",
    join(root, "skills", "delegation"),
    "--skill",
    join(root, "skills", "widgets"),
    "--session-dir",
    sessionDirectory,
    "--continue",
    "user-argument",
  ];
}

test("development scripts use one working-tree runner and normal mode stays silent", async () => {
  const packageJson = JSON.parse(
    await readFile(join(root, "package.json"), "utf8"),
  );
  assert.equal(packageJson.scripts.dev, "./scripts/scufris-dev");
  assert.equal(
    packageJson.scripts["dev:voice"],
    "./scripts/scufris-dev --voice",
  );
  assert.equal(packageJson.scripts.pi, undefined);

  const item = await fixture();
  const env = {
    ...item.env,
    SCUFRIS_SPEECH: "1",
    SCUFRIS_CALM: "1",
    SCUFRIS_PIPER_MODEL: "/untrusted/model",
    SCUFRIS_PIPER_CONFIG: "/untrusted/config",
  };
  const result = await run(["user-argument"], env, item.directory);
  assert.equal(result.code, 0, result.stderr);
  const sessionDirectory = join(item.state, "scufris", "dev-sessions");
  assert.deepEqual(JSON.parse(result.stdout), {
    argv: expectedArgs(sessionDirectory, false),
    role: "orchestrator",
    speech: null,
    calm: null,
    model: null,
    config: null,
    roots: '["~/personal","~/work","~/third-party"]',
    stt: "/global/pi-voice-stt.json",
    endpoint: "inherited-endpoint",
  });
  assert.equal((await stat(sessionDirectory)).mode & 0o777, 0o700);
});

test("voice development requires nix develop and composes exact trusted runtime", async () => {
  const item = await fixture();
  const unavailable = await run(
    ["--voice", "user-argument"],
    item.env,
    item.directory,
  );
  assert.equal(unavailable.code, 2);
  assert.equal(
    unavailable.stderr,
    "scufris-dev: voice requires the repository nix develop shell\n",
  );
  assert.equal(unavailable.stdout, "");

  const model = join(item.directory, "model.onnx");
  const config = `${model}.json`;
  await writeFile(model, "model", "utf8");
  await writeFile(config, "config", "utf8");
  await executable(join(item.bin, "piper"), "#!/usr/bin/env bash\nexit 0\n");
  await executable(join(item.bin, "pw-play"), "#!/usr/bin/env bash\nexit 0\n");
  const env = {
    ...item.env,
    SCUFRIS_DEV_VOICE: "1",
    SCUFRIS_PIPER_MODEL: model,
    SCUFRIS_PIPER_CONFIG: config,
    SCUFRIS_PROJECT_ROOTS: '["/trusted/projects"]',
  };
  const result = await run(["--voice", "user-argument"], env, item.directory);
  assert.equal(result.code, 0, result.stderr);
  const sessionDirectory = join(item.state, "scufris", "dev-sessions");
  assert.deepEqual(JSON.parse(result.stdout), {
    argv: expectedArgs(sessionDirectory, true),
    role: "orchestrator",
    speech: "1",
    calm: "1",
    model,
    config,
    roots: '["/trusted/projects"]',
    stt: "/global/pi-voice-stt.json",
    endpoint: "inherited-endpoint",
  });

  const source = await readFile(helper, "utf8");
  assert.doesNotMatch(source, /PI_STT_CONFIG|whisper|endpoint/i);
});
