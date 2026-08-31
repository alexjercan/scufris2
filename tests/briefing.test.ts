import assert from "node:assert/strict";
import test from "node:test";
import { wakeMessage } from "../agent/extensions/scufris/briefing/briefing.ts";
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
