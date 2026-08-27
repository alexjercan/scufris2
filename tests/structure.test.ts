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
    "./extensions/scufris/workflow/index.ts",
    "./extensions/scufris/voice/index.ts",
    "./extensions/scufris/calm.ts",
    "./extensions/scufris/service/index.ts",
    "./extensions/scufris/widgets/index.ts",
  ]);

  const files = await typeScriptFiles(join(root, "extensions", "scufris"));
  for (const file of files) {
    assert.doesNotMatch(await readFile(file, "utf8"), /scripts\/scufris-/);
  }

  const orchestration = await readFile(
    join(root, "extensions", "scufris", "workflow", "orchestration.ts"),
    "utf8",
  );
  assert.doesNotMatch(orchestration, /setInterval|setTimeout/);
  assert.doesNotMatch(orchestration, /while \(readingEvents\)/);
  assert.match(orchestration, /watch\(job\.status_file/);
  assert.match(orchestration, /eventReadController\?\.abort\(\)/);
  await access(
    join(root, "extensions", "scufris", "workflow", "worker-report.ts"),
  );
  await access(join(root, "tools", "jobs", "scufris-report"));
  await access(join(root, "tools", "voice", "scufris-speak"));
  assert.deepEqual((await readdir(join(root, "scripts"))).sort(), [
    "scufris-artifacts-prune",
    "scufris-dev",
    "scufris-jobs",
  ]);
});
