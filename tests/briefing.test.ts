import assert from "node:assert/strict";
import {
  chmodSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { pathToFileURL } from "node:url";
import briefing from "../agent/extensions/scufris/briefing/briefing.ts";
import { toolPath } from "../agent/extensions/scufris/shared/runtime.ts";
import {
  DEFAULT_PROFILE,
  localDate,
} from "../agent/extensions/scufris/briefing/run.ts";

const on = (text: string): Date => new Date(text);

interface Sent {
  customType: string;
  content: string;
  details?: Record<string, unknown>;
}

/** How long the real helper is given to answer a file read. */
const ANSWER_WITHIN = 15_000;
/** How long a session that should ask for nothing is watched. */
const SILENCE_FOR = 1_000;

/** Load the extension against a state directory this test owns.
 *
 * The extension holds no clock any more, so what it does at session start is
 * decided by what the helper finds on disk. Every run here is the real helper
 * against a temporary state directory.
 */
function session(state: string): {
  start: (expected: number) => Promise<void>;
  shutdown: () => void;
  sent: Sent[];
  updates: () => Array<Record<string, unknown>>;
} {
  const handlers = new Map<string, (event: unknown, ctx: unknown) => unknown>();
  const sent: Sent[] = [];
  const pi = {
    registerTool() {},
    sendMessage(message: Sent) {
      sent.push(message);
    },
    on(name: string, handler: (event: unknown, ctx: unknown) => unknown) {
      handlers.set(name, handler);
    },
  };
  const ctl = join(state, "scufris-ctl");
  const log = join(state, "control.jsonl");
  writeFileSync(
    ctl,
    '#!/bin/sh\nprintf "%s\\n" "$2" >> "$SCUFRIS_TEST_CTL_LOG"\n',
  );
  chmodSync(ctl, 0o755);
  const role = process.env.SCUFRIS_ROLE;
  const home = process.env.XDG_STATE_HOME;
  process.env.SCUFRIS_ROLE = "orchestrator";
  process.env.XDG_STATE_HOME = state;
  try {
    briefing(pi as never);
  } finally {
    if (role === undefined) delete process.env.SCUFRIS_ROLE;
    else process.env.SCUFRIS_ROLE = role;
    if (home === undefined) delete process.env.XDG_STATE_HOME;
    else process.env.XDG_STATE_HOME = home;
  }
  return {
    async start(expected: number) {
      const previous = process.env.XDG_STATE_HOME;
      const previousCtl = process.env.SCUFRIS_CTL;
      const previousLog = process.env.SCUFRIS_TEST_CTL_LOG;
      process.env.XDG_STATE_HOME = state;
      process.env.SCUFRIS_CTL = ctl;
      process.env.SCUFRIS_TEST_CTL_LOG = log;
      try {
        const answer = handlers.get("session_start")?.(
          { reason: "startup" },
          { hasUI: false },
        );
        // The handler must hand the read off rather than hold the session
        // open: pi awaits the session_start listeners one after another, and
        // this extension is loaded before the one the surfaces speak through.
        // The helper is spawned before it returns, so the state directory this
        // test owns is the one the child was given.
        assert.equal(answer, undefined);
      } finally {
        if (previous === undefined) delete process.env.XDG_STATE_HOME;
        else process.env.XDG_STATE_HOME = previous;
        if (previousCtl === undefined) delete process.env.SCUFRIS_CTL;
        else process.env.SCUFRIS_CTL = previousCtl;
        if (previousLog === undefined) delete process.env.SCUFRIS_TEST_CTL_LOG;
        else process.env.SCUFRIS_TEST_CTL_LOG = previousLog;
      }
      // A session expected to ask for nothing is watched for the whole
      // silence: the read has to have happened for the absence to mean
      // anything.
      const until = Date.now() + (expected ? ANSWER_WITHIN : SILENCE_FOR);
      const count = () =>
        existsSync(log)
          ? readFileSync(log, "utf8").split("\n").filter(Boolean).length
          : 0;
      while (Date.now() < until && (expected === 0 || count() < expected))
        await new Promise((resolve) => setTimeout(resolve, 25));
    },
    shutdown() {
      handlers.get("session_shutdown")?.(undefined, undefined);
    },
    sent,
    updates: () =>
      existsSync(log)
        ? readFileSync(log, "utf8")
            .split("\n")
            .filter(Boolean)
            .map((line) => JSON.parse(line) as Record<string, unknown>)
        : [],
  };
}

/** Write a run the way a collection leaves one. */
function run(
  state: string,
  date: string,
  profile: string,
  manifest: Record<string, unknown>,
): void {
  const directory = join(state, "scufris", "briefings", date, profile);
  mkdirSync(directory, { recursive: true });
  writeFileSync(
    join(directory, "manifest.json"),
    JSON.stringify({
      version: 2,
      generation: `${date}-${profile}`,
      date,
      profile,
      delivery: "pending",
      started: `${date}T08:00:00+00:00`,
      finished: `${date}T08:02:00+00:00`,
      sources: [
        {
          project: "personal/the-den",
          slug: "personal-the-den",
          title: "The Den",
          status: "ok",
          headline: "clear",
          facts: [],
          harness: "pi",
          model: "",
          seconds: 1,
        },
      ],
      diagnostics: [],
      events: [],
      ...manifest,
    }),
  );
}

function room(): string {
  return mkdtempSync(join(tmpdir(), "scufris-briefing-"));
}

test("a run is named for the local date", () => {
  assert.equal(localDate(on("2026-08-31T23:30:00")), "2026-08-31");
  assert.equal(localDate(on("2026-01-05T00:01:00")), "2026-01-05");
  assert.equal(DEFAULT_PROFILE, "morning");
});

test("startup reconciles a gathered run without injecting a model turn", async () => {
  const state = room();
  const today = localDate(new Date());
  run(state, today, "morning", { state: "collected" });
  const opened = session(state);
  try {
    await opened.start(1);
    assert.deepEqual(opened.sent, []);
    const update = opened.updates()[0]!;
    const row = update.briefing as Record<string, unknown>;
    const wake = update.wake as Record<string, unknown>;
    assert.equal(row.profile, "morning");
    assert.equal(row.collection, "collected");
    assert.equal(row.delivery, "pending");
    assert.match(String(wake.text), /morning briefing for /);
    assert.match(String(wake.text), /scufris_briefing_publish/);
  } finally {
    opened.shutdown();
    rmSync(state, { recursive: true, force: true });
  }
});

test("a nightly gathered before midnight is still asked for in the morning", async () => {
  // The nightly timer collects at 23:00. If nothing was connected, its run is
  // pending under yesterday's date by the time the next session opens, and
  // reading only today lost it for good: `collected_runs` is per-date, so
  // nothing else would ever have found it either.
  const state = room();
  const now = new Date();
  const earlier = new Date(now);
  earlier.setDate(earlier.getDate() - 1);
  const yesterday = localDate(earlier);
  run(state, yesterday, "nightly", { state: "collected" });
  const opened = session(state);
  try {
    await opened.start(1);
    assert.deepEqual(opened.sent, []);
    const row = opened.updates()[0]!.briefing as Record<string, unknown>;
    assert.equal(row.date, yesterday);
    assert.equal(row.profile, "nightly");
  } finally {
    opened.shutdown();
    rmSync(state, { recursive: true, force: true });
  }
});

test("a session that opens on a delivered run asks for nothing", async () => {
  const state = room();
  const today = localDate(new Date());
  run(state, today, "morning", {
    version: 1,
    state: "delivered",
    delivery: undefined,
    generation: undefined,
  });
  const opened = session(state);
  try {
    await opened.start(1);
    assert.deepEqual(opened.sent, []);
    const update = opened.updates()[0]!;
    assert.equal(update.wake, undefined);
    assert.equal(
      (update.briefing as Record<string, unknown>).delivery,
      "delivered",
    );
  } finally {
    opened.shutdown();
    rmSync(state, { recursive: true, force: true });
  }
});

test("two profiles gathered on one date are two askings, each naming its own", async () => {
  const state = room();
  const today = localDate(new Date());
  run(state, today, "morning", { state: "collected" });
  run(state, today, "evening", {
    state: "collected",
    finished: `${today}T19:00:00+00:00`,
  });
  const opened = session(state);
  try {
    await opened.start(2);
    assert.deepEqual(opened.sent, []);
    const updates = opened.updates();
    assert.equal(updates.length, 2);
    const profiles = updates.map(
      (update) => (update.briefing as Record<string, unknown>).profile,
    );
    assert.deepEqual([...profiles].sort(), ["evening", "morning"]);
    for (const update of updates) {
      const profile = (update.briefing as Record<string, unknown>).profile;
      assert.match(
        String((update.wake as Record<string, unknown>).text),
        new RegExp(`profile ${String(profile)}`),
      );
    }
  } finally {
    opened.shutdown();
    rmSync(state, { recursive: true, force: true });
  }
});

test("a session that opens on a briefing nothing declared asks for nothing", async () => {
  const state = room();
  const today = localDate(new Date());
  run(state, today, "morning", { state: "collected", sources: [] });
  const opened = session(state);
  try {
    await opened.start(1);
    assert.deepEqual(opened.sent, []);
    const update = opened.updates()[0]!;
    assert.equal(update.wake, undefined);
    assert.equal(
      (update.briefing as Record<string, unknown>).delivery,
      "delivered",
    );
  } finally {
    opened.shutdown();
    rmSync(state, { recursive: true, force: true });
  }
});

test("the helper is found from the source tree and from a package", () => {
  // Extensions sit three levels under `share/scufris` in a package and four
  // under the repository root in the working tree. Staging runs the working
  // tree, so a path written for one layout only is a helper nobody can spawn.
  const source = new URL(
    "../agent/extensions/scufris/briefing/briefing.ts",
    import.meta.url,
  ).href;
  assert.ok(existsSync(toolPath("briefing/cli.py", source)));

  const packaged = room();
  const share = join(packaged, "share", "scufris");
  mkdirSync(join(share, "tools", "briefing"), { recursive: true });
  mkdirSync(join(share, "extensions", "scufris", "briefing"), {
    recursive: true,
  });
  writeFileSync(join(share, "tools", "briefing", "cli.py"), "");
  const url = pathToFileURL(
    join(share, "extensions", "scufris", "briefing", "briefing.ts"),
  ).href;
  assert.equal(
    toolPath("briefing/cli.py", url),
    join(share, "tools", "briefing", "cli.py"),
  );
  rmSync(packaged, { recursive: true, force: true });
});
