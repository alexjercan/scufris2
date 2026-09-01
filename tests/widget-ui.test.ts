import assert from "node:assert/strict";
import { readFile, readdir } from "node:fs/promises";
import { join, resolve } from "node:path";
import test from "node:test";

const root = resolve(new URL("..", import.meta.url).pathname);
const desktop = join(root, "surfaces", "desktop");

function pixels(source: string, token: string): number {
  const found = new RegExp(`--${token}:\\s*(\\d+)px`).exec(source);
  assert.ok(found !== null, `tokens.css does not define --${token} in pixels`);
  return Number(found[1]);
}

test("widget scroll lists reserve a lane wider than their scrollbar", async () => {
  const [shell, tokens] = await Promise.all([
    readFile(join(desktop, "shell", "shell.css"), "utf8"),
    readFile(join(desktop, "shell", "tokens.css"), "utf8"),
  ]);

  assert.match(
    shell,
    /\.scroll-list\s*\{[^}]*padding-inline-end:\s*var\(--sw-scroll-clearance\)/s,
  );
  assert.match(
    shell,
    /\.scroll-list::-webkit-scrollbar\s*\{[^}]*width:\s*var\(--sw-scroll-width\)/s,
  );
  assert.ok(
    pixels(tokens, "sw-scroll-clearance") > pixels(tokens, "sw-scroll-width"),
    "the scrollbar lane leaves no clearance before row actions",
  );
});

test("every scrolling widget list uses the shared scrollbar-safe class", async () => {
  const directory = join(desktop, "widgets");
  const entries = await readdir(directory, { withFileTypes: true });
  const covered: string[] = [];

  for (const entry of entries) {
    if (!entry.isDirectory()) continue;
    const files = await readdir(join(directory, entry.name), {
      withFileTypes: true,
    });
    if (!files.some((file) => file.isFile() && file.name === "widget.toml")) {
      continue;
    }
    const relative = join(entry.name, "widget.ts");
    const source = await readFile(join(directory, relative), "utf8");
    const scrolls = [...source.matchAll(/(\w+)\.style\.overflowY = "auto";/g)];
    if (scrolls.length > 0) covered.push(relative);

    for (const scroll of scrolls) {
      const name = scroll[1];
      assert.ok(name !== undefined);
      assert.match(
        source,
        new RegExp(`${name}\\.className = "scroll-list";`),
        `${relative} leaves ${name} outside the shared scrollbar-safe class`,
      );
    }
  }

  assert.deepEqual(covered.sort(), [
    join("agenda", "widget.ts"),
    join("macros", "widget.ts"),
    join("notes", "widget.ts"),
  ]);
});
