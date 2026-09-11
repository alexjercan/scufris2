import assert from "node:assert/strict";
import { access, readFile, readdir } from "node:fs/promises";
import { join, resolve } from "node:path";
import test from "node:test";

const root = resolve(new URL("..", import.meta.url).pathname);

async function typeScriptFiles(directory: string): Promise<string[]> {
  const files: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) files.push(...(await typeScriptFiles(path)));
    else if (entry.name.endsWith(".ts")) files.push(path);
  }
  return files;
}

test("package loads only capability-owned Scufris extensions", async () => {
  const manifest = JSON.parse(
    await readFile(join(root, "package.json"), "utf8"),
  );
  assert.deepEqual(manifest.pi.extensions, [
    "./agent/extensions/scufris/workflow/index.ts",
    "./agent/extensions/scufris/briefing/index.ts",
    "./agent/extensions/scufris/response.ts",
    "./agent/extensions/scufris/calm.ts",
    "./agent/extensions/scufris/service/index.ts",
  ]);
  assert.deepEqual(manifest.pi.skills, ["./agent/skills"]);

  await Promise.all([
    access(join(root, "host", "service", "Cargo.toml")),
    access(join(root, "shared", "control", "Cargo.toml")),
    access(join(root, "surfaces", "desktop", "Cargo.toml")),
    access(join(root, "surfaces", "desktop", "widgets", "widget.d.ts")),
    access(join(root, "surfaces", "ios", "project.yml")),
    access(join(root, "surfaces", "ios", "Sources", "ScufrisApp.swift")),
    access(join(root, "surfaces", "desktop", "backends", "den", "backend.py")),
  ]);
  await assert.rejects(access(join(root, "native")));
  await assert.rejects(access(join(root, "host", "gateway")));

  const files = await typeScriptFiles(
    join(root, "agent", "extensions", "scufris"),
  );
  for (const file of files) {
    assert.doesNotMatch(await readFile(file, "utf8"), /scripts\/scufris-/);
  }

  const orchestration = await readFile(
    join(
      root,
      "agent",
      "extensions",
      "scufris",
      "workflow",
      "orchestration.ts",
    ),
    "utf8",
  );
  assert.doesNotMatch(orchestration, /setInterval|setTimeout/);
  assert.doesNotMatch(orchestration, /while \(readingEvents\)/);
  assert.match(orchestration, /watch\(job\.status_file/);
  assert.match(orchestration, /eventReadController\?\.abort\(\)/);
  // A failed drain strands every event the pass had not read. The status
  // watcher never fires again for a worker that already finished, so the
  // retry is the settle that ends the turn the failure wakes. That is how a
  // retry exists here without a clock, and the assertion above still holds.
  assert.match(orchestration, /reportDrainFailure\(message\)/);
  assert.match(
    orchestration,
    /if \(eventStranded && !shuttingDown\) void readEvents\(\)/,
  );
  // `hasUI` is false under the service, so a notification alone reported a
  // stranded drain to nobody. It is published as a failed row instead, and
  // filing that row is what acknowledges it.
  assert.match(orchestration, /id: EVENT_DRAIN_ROW/);
  await access(
    join(
      root,
      "agent",
      "extensions",
      "scufris",
      "workflow",
      "worker-report.ts",
    ),
  );
  // The briefing holds no clock at all. A systemd timer decides when one
  // happens, and what is left in the extension is one file read at session
  // start for a run gathered while nothing was connected. The single timeout
  // is the bound on that helper run, not a schedule.
  const briefing = await readFile(
    join(root, "agent", "extensions", "scufris", "briefing", "briefing.ts"),
    "utf8",
  );
  assert.doesNotMatch(briefing, /setInterval/);
  assert.equal(briefing.match(/setTimeout\(/g)?.length, 1);
  assert.doesNotMatch(briefing, /untilTomorrow|parseSchedule|decide\(/);
  // The terminal is the one project-local extension. It is off unless asked
  // for by name, so a plain `pi` in this checkout gets nothing from it, and
  // it holds the gate and the composition only: the lifecycle is packaged.
  const terminal = await readFile(
    join(root, ".pi", "extensions", "scufris-terminal", "index.ts"),
    "utf8",
  );
  assert.match(terminal, /SCUFRIS_TERMINAL/);
  assert.match(terminal, /if \(!terminalEnabled\(\)\) return;/);
  assert.doesNotMatch(terminal, /pi\.on\("input"/);
  await access(join(root, "tools", "briefing", "cli.py"));
  await access(join(root, "tools", "briefing", "page.py"));
  await access(join(root, "tools", "jobs", "scufris-report"));
  await access(join(root, "tools", "voice", "scufris-speak"));
  assert.deepEqual((await readdir(join(root, "scripts"))).sort(), [
    // The launcher `scufris-dev` and `scufris-staging` both run. It is its own
    // script because the service starts an agent on a session directory of its
    // own choosing, which a runner that picks one cannot be.
    "scufris-agent",
    "scufris-dev",
    "scufris-jobs",
    "scufris-staging",
    "scufris-terminal",
  ]);
  // The terminal launcher chooses a session and nothing else. It composes no
  // extensions, because the composition is the checkout's own and two of them
  // in one process would be two agents.
  const launcher = await readFile(
    join(root, "scripts", "scufris-terminal"),
    "utf8",
  );
  assert.match(launcher, /export SCUFRIS_TERMINAL=1/);
  assert.match(launcher, /--session-dir/);
  assert.match(launcher, /--fork/);
  assert.doesNotMatch(launcher, /--extension|--skill/);
  // Jobs a conversation delegates outlive the session that started them, so
  // both launchers of the managed child name the conversation as the owner.
  for (const managed of [
    join(root, "scripts", "scufris-agent"),
    join(root, "nix", "launcher.nix"),
  ]) {
    assert.match(
      await readFile(managed, "utf8"),
      /SCUFRIS_JOB_OWNER=foreground/,
    );
  }
});
