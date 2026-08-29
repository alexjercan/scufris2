import assert from "node:assert/strict";
import {
  chmod,
  copyFile,
  mkdir,
  mkdtemp,
  readFile,
  realpath,
  stat,
  symlink,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { delimiter, join, relative, resolve } from "node:path";
import { spawn } from "node:child_process";
import test from "node:test";

const root = resolve(new URL("..", import.meta.url).pathname);
const helper = join(root, "scripts", "scufris-dev");
// The runner picks a session directory and hands the rest to the launcher, so
// the fixture is only a runner where both are beside each other.
const launcher = join(root, "scripts", "scufris-agent");

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
  const bin = join(directory, "system-bin");
  const state = join(directory, "state");
  const fixtureHelper = join(directory, "scripts", "scufris-dev");
  const repositoryNpmBin = join(directory, "node_modules", ".bin");
  await mkdir(bin);
  await mkdir(join(directory, "scripts"));
  await mkdir(repositoryNpmBin, { recursive: true });
  await copyFile(helper, fixtureHelper);
  await chmod(fixtureHelper, 0o755);
  const fixtureLauncher = join(directory, "scripts", "scufris-agent");
  await copyFile(launcher, fixtureLauncher);
  await chmod(fixtureLauncher, 0o755);
  await executable(
    join(repositoryNpmBin, "pi"),
    "#!/usr/bin/env bash\nexit 99\n",
  );
  await executable(
    join(bin, "pi"),
    `#!${process.execPath}\nconst { accessSync, constants, realpathSync } = require("node:fs");\nconst { delimiter, join } = require("node:path");\nconst ambientPi = (process.env.PATH ?? "").split(delimiter).map((entry) => join(entry || process.cwd(), "pi")).find((candidate) => { try { accessSync(candidate, constants.X_OK); return true; } catch { return false; } });\nconsole.log(JSON.stringify({ argv: process.argv.slice(2), foregroundPi: realpathSync(process.argv[1]), ambientPi: ambientPi ? realpathSync(ambientPi) : null, path: process.env.PATH ?? null, role: process.env.SCUFRIS_ROLE ?? null, model: process.env.SCUFRIS_PIPER_MODEL ?? null, config: process.env.SCUFRIS_PIPER_CONFIG ?? null, roots: process.env.SCUFRIS_PROJECT_ROOTS ?? null, stt: process.env.PI_STT_CONFIG ?? null, endpoint: process.env.TEST_STT_ENDPOINT ?? null }));\n`,
  );
  const npmBinAlias = join(directory, "npm-bin-alias");
  await symlink(repositoryNpmBin, npmBinAlias, "dir");
  const pathEntries = [
    repositoryNpmBin,
    `${repositoryNpmBin}/`,
    npmBinAlias,
    relative(directory, repositoryNpmBin),
    bin,
    "",
    bin,
    ...(process.env.PATH ?? "").split(delimiter),
    "",
  ];
  const repositoryNpmBinPhysical = await realpath(repositoryNpmBin);
  const systemPathEntries: string[] = [];
  for (const entry of pathEntries) {
    if (entry === "") {
      systemPathEntries.push(entry);
      continue;
    }
    try {
      if (
        (await realpath(resolve(directory, entry))) === repositoryNpmBinPhysical
      ) {
        continue;
      }
    } catch {}
    systemPathEntries.push(entry);
  }
  const systemPath = systemPathEntries.join(delimiter);
  const env: NodeJS.ProcessEnv = {
    ...process.env,
    HOME: join(directory, "home"),
    XDG_STATE_HOME: state,
    PATH: pathEntries.join(delimiter),
    SCUFRIS_ROLE: "worker",
    PI_STT_CONFIG: "/global/pi-voice-stt.json",
    TEST_STT_ENDPOINT: "inherited-endpoint",
  };
  delete env.SCUFRIS_PROJECT_ROOTS;
  delete env.SCUFRIS_PIPER_MODEL;
  delete env.SCUFRIS_PIPER_CONFIG;
  return { directory, bin, state, helper: fixtureHelper, systemPath, env };
}

function assertManagedPath(
  output: Record<string, unknown>,
  systemPath: string,
): void {
  assert.equal(output.foregroundPi, output.ambientPi);
  assert.equal(output.path, systemPath);
  delete output.foregroundPi;
  delete output.ambientPi;
  delete output.path;
}

async function run(
  args: string[],
  env: NodeJS.ProcessEnv,
  cwd: string,
  executableHelper: string,
): Promise<Result> {
  return await new Promise((resolveResult, reject) => {
    const child = spawn(executableHelper, args, {
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

function expectedArgs(projectRoot: string, sessionDirectory: string): string[] {
  const extensions = [
    join("workflow", "index.ts"),
    "response.ts",
    "calm.ts",
    // The service extension is how a working-tree agent reaches the service
    // at all. Without it the conversation has no client in this process.
    join("service", "index.ts"),
    join("widgets", "index.ts"),
    "conversation.ts",
  ];
  return [
    ...extensions.flatMap((name) => [
      "--extension",
      join(projectRoot, "agent", "extensions", "scufris", name),
    ]),
    "--skill",
    join(projectRoot, "agent", "skills", "workflow"),
    "--skill",
    join(projectRoot, "agent", "skills", "widgets"),
    "--session-dir",
    sessionDirectory,
    "--continue",
    "user-argument",
  ];
}

test("development runner keeps foreground and ambient Pi on the managed PATH", async () => {
  const packageJson = JSON.parse(
    await readFile(join(root, "package.json"), "utf8"),
  );
  assert.equal(packageJson.scripts.dev, "./scripts/scufris-dev");
  // There is no voice mode to run. Nothing in this process tree makes sound,
  // so a second script that turned speech on has nothing left to turn on.
  assert.equal(packageJson.scripts["dev:voice"], undefined);
  assert.equal(packageJson.scripts.pi, undefined);

  const item = await fixture();
  // Piper paths from the ambient environment are dropped rather than passed
  // on: the trusted pair is bound inside `scufris-speak`, and the agent runs
  // no synthesiser to hand them to.
  const env = {
    ...item.env,
    SCUFRIS_PIPER_MODEL: "/untrusted/model",
    SCUFRIS_PIPER_CONFIG: "/untrusted/config",
  };
  const result = await run(["user-argument"], env, item.directory, item.helper);
  assert.equal(result.code, 0, result.stderr);
  const sessionDirectory = join(item.state, "scufris", "dev-sessions");
  const output = JSON.parse(result.stdout);
  assertManagedPath(output, item.systemPath);
  assert.deepEqual(output, {
    argv: expectedArgs(item.directory, sessionDirectory),
    role: "orchestrator",
    model: null,
    config: null,
    roots: '["~/personal","~/work","~/third-party"]',
    stt: "/global/pi-voice-stt.json",
    endpoint: "inherited-endpoint",
  });
  assert.equal((await stat(sessionDirectory)).mode & 0o777, 0o700);
});
