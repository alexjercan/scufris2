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
  // stranded drain to nobody.
  assert.match(orchestration, /id: EVENT_DRAIN_NOTICE/);
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
  ]);
});
