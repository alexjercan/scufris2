import assert from "node:assert/strict";
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { pathToFileURL } from "node:url";
import briefing, {
  wakeMessage,
} from "../agent/extensions/scufris/briefing/briefing.ts";
import { toolPath } from "../agent/extensions/scufris/shared/runtime.ts";
import {
  DEFAULT_BRIEFING_TIME,
  decide,
  localDate,
  parseSchedule,
  type RunState,
} from "../agent/extensions/scufris/briefing/schedule.ts";

const morning = { hour: 8, minute: 0 };
const on = (text: string): Date => new Date(text);

test("a briefing time is a time of day, a refusal, or a mistake said out loud", () => {
  assert.deepEqual(parseSchedule("07:30"), { kind: "at", hour: 7, minute: 30 });
  assert.deepEqual(parseSchedule(" 9:05 "), { kind: "at", hour: 9, minute: 5 });
  assert.deepEqual(parseSchedule("23:59"), {
    kind: "at",
    hour: 23,
    minute: 59,
  });
  assert.deepEqual(parseSchedule("off"), { kind: "off" });
  assert.deepEqual(parseSchedule(undefined), {
    kind: "at",
    hour: 8,
    minute: 0,
  });
  assert.equal(DEFAULT_BRIEFING_TIME, "08:00");
  // Set to nothing is the same as never set: the default morning.
  assert.deepEqual(parseSchedule(""), { kind: "at", hour: 8, minute: 0 });
  // A time nobody can act on is reported rather than treated as no briefing.
  for (const raw of ["24:00", "8", "morning", "07:60"])
    assert.equal(parseSchedule(raw).kind, "invalid", raw);
});

test("a run is named for the local date", () => {
  assert.equal(localDate(on("2026-08-31T23:30:00")), "2026-08-31");
  assert.equal(localDate(on("2026-01-05T00:01:00")), "2026-01-05");
});

test("a session before the morning waits for it", () => {
  const next = decide(on("2026-08-31T06:15:00"), morning, "none");
  assert.deepEqual(next, { do: "wait", delayMs: 105 * 60 * 1000 });
});

test("a late session catches the morning up rather than skipping it", () => {
  assert.deepEqual(decide(on("2026-08-31T14:00:00"), morning, "none"), {
    do: "collect",
  });
  // A run left half-made by a crash owns no process any more, so it is made
  // again rather than counted as today's.
  assert.deepEqual(decide(on("2026-08-31T14:00:00"), morning, "collecting"), {
    do: "collect",
  });
});

test("a delivered morning is never delivered twice", () => {
  const next = decide(on("2026-08-31T14:00:00"), morning, "delivered");
  assert.deepEqual(next, { do: "wait", delayMs: 18 * 60 * 60 * 1000 });
});

test("a gathered morning that lost its prose is written, not gathered again", () => {
  assert.deepEqual(decide(on("2026-08-31T09:00:00"), morning, "collected"), {
    do: "publish",
  });
});

test("a morning every source failed is not retried by itself", () => {
  const next = decide(on("2026-08-31T09:00:00"), morning, "failed");
  assert.equal(next.do, "wait");
});

test("every run state decides something", () => {
  const states: RunState[] = [
    "none",
    "collecting",
    "collected",
    "delivered",
    "failed",
  ];
  for (const state of states) {
    const next = decide(on("2026-08-31T09:00:00"), morning, state);
    assert.ok(["collect", "publish", "wait"].includes(next.do), state);
    if (next.do === "wait") assert.ok(next.delayMs > 0, state);
  }
});

test("the wake asks for one briefing and names what could not answer", () => {
  const message = wakeMessage({
    date: "2026-08-31",
    profile: "morning",
    state: "collected",
    sources: [
      { project: "personal/the-den", status: "ok", headline: "clear" },
      { project: "personal/seedzero", status: "failed", headline: "no answer" },
    ],
    diagnostics: [],
  });
  assert.match(message, /1 source answered, 1 could not answer/);
  assert.match(message, /scufris_briefing_show/);
  assert.match(message, /scufris_briefing_publish/);
  assert.match(message, /Do not read the sources out one after another/);
  assert.match(message, /Name any source that could not answer/);

  const quiet = wakeMessage({
    date: "2026-08-31",
    profile: "morning",
    state: "collected",
    sources: [
      { project: "personal/the-den", status: "ok", headline: "clear" },
      { project: "personal/seedzero", status: "attention", headline: "late" },
    ],
    diagnostics: [],
  });
  assert.match(quiet, /2 sources answered\./);
  assert.doesNotMatch(quiet, /, \d+ could not answer/);
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

  const room = mkdtempSync(join(tmpdir(), "scufris-tool-path-"));
  const share = join(room, "share", "scufris");
  mkdirSync(join(share, "tools", "briefing"), { recursive: true });
  mkdirSync(join(share, "extensions", "scufris", "briefing"), {
    recursive: true,
  });
  writeFileSync(join(share, "tools", "briefing", "cli.py"), "");
  const packaged = pathToFileURL(
    join(share, "extensions", "scufris", "briefing", "briefing.ts"),
  ).href;
  assert.equal(
    toolPath("briefing/cli.py", packaged),
    join(share, "tools", "briefing", "cli.py"),
  );
  rmSync(room, { recursive: true, force: true });
});

test("the session start handler hands the morning off instead of holding the session open", () => {
  // pi awaits the session_start listeners one after another, in the order the
  // extensions were loaded. This one is loaded before the extension that
  // connects to the service, so a handler that awaited the collection would
  // leave every surface unable to reach the agent for as long as the sources
  // took to answer. It must return something pi has nothing to wait for.
  const role = process.env.SCUFRIS_ROLE;
  const time = process.env.SCUFRIS_BRIEFING_TIME;
  process.env.SCUFRIS_ROLE = "orchestrator";
  process.env.SCUFRIS_BRIEFING_TIME = "off";
  const handlers = new Map<string, (event: unknown, ctx: unknown) => unknown>();
  const pi = {
    registerTool() {},
    sendMessage() {},
    on(name: string, handler: (event: unknown, ctx: unknown) => unknown) {
      handlers.set(name, handler);
    },
  };
  try {
    briefing(pi as never);
    const start = handlers.get("session_start");
    assert.ok(start);
    const answer = start({ reason: "startup" }, { hasUI: false });
    assert.equal(answer, undefined);
    handlers.get("session_shutdown")?.(undefined, undefined);
  } finally {
    if (role === undefined) delete process.env.SCUFRIS_ROLE;
    else process.env.SCUFRIS_ROLE = role;
    if (time === undefined) delete process.env.SCUFRIS_BRIEFING_TIME;
    else process.env.SCUFRIS_BRIEFING_TIME = time;
  }
});
