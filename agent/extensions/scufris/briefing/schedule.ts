/** When the morning briefing happens, and what to do when a session opens.
 *
 * All of it is arithmetic on a clock and one run state, so all of it is
 * testable without a timer, a model, or a project. The extension owns the
 * timer; this owns the decision.
 */

/** The default morning, when nothing configures one. */
export const DEFAULT_BRIEFING_TIME = "08:00";

const TIME = /^([01]?\d|2[0-3]):([0-5]\d)$/;

export type ScheduleSetting =
  | { kind: "at"; hour: number; minute: number }
  | { kind: "off" }
  | { kind: "invalid"; raw: string };

/** A run directory's state, as the helper reports it. */
export type RunState =
  | "none"
  | "collecting"
  | "collected"
  | "delivered"
  | "failed";

/** What a session should do about today, right now. */
export type Next =
  | { do: "collect" }
  | { do: "publish" }
  | { do: "wait"; delayMs: number };

export function parseSchedule(raw: string | undefined): ScheduleSetting {
  // An unset variable and one set to nothing are the same thing in a shell, so
  // both mean the default morning rather than a mistake or a refusal.
  const value = (raw ?? "").trim().toLowerCase() || DEFAULT_BRIEFING_TIME;
  if (value === "off" || value === "none") return { kind: "off" };
  const match = TIME.exec(value);
  if (!match) return { kind: "invalid", raw: value };
  return { kind: "at", hour: Number(match[1]), minute: Number(match[2]) };
}

/** The local date, which is the name of a run. */
export function localDate(now: Date): string {
  const month = `${now.getMonth() + 1}`.padStart(2, "0");
  const day = `${now.getDate()}`.padStart(2, "0");
  return `${now.getFullYear()}-${month}-${day}`;
}

function at(now: Date, hour: number, minute: number, dayOffset = 0): Date {
  const when = new Date(now);
  when.setDate(when.getDate() + dayOffset);
  when.setHours(hour, minute, 0, 0);
  return when;
}

/** Milliseconds until the next morning after this one.
 *
 * Used after a run is finished with, where the answer is always tomorrow and
 * the decision below would have to be narrowed to say so.
 */
export function untilTomorrow(
  now: Date,
  schedule: { hour: number; minute: number },
): number {
  return Math.max(
    0,
    at(now, schedule.hour, schedule.minute, 1).getTime() - now.getTime(),
  );
}

/** What to do about today, from the clock and what the run directory says.
 *
 * A briefing is caught up rather than skipped: a session that opens at two in
 * the afternoon with no run for today runs one then, because a morning nobody
 * was awake for is still a morning that was never delivered.
 *
 * The reason the decision reads the state and not a delivered flag is what
 * happens after a crash. A run left `collecting` is one no process owns any
 * more, so it is started again. A run that is `collected` was gathered and its
 * prose was lost, so it is published rather than gathered a second time - the
 * sources already answered, and asking them again would cost the morning and
 * could answer differently.
 */
export function decide(
  now: Date,
  schedule: { hour: number; minute: number },
  state: RunState,
): Next {
  if (state === "collected") return { do: "publish" };
  const today = at(now, schedule.hour, schedule.minute);
  const pending = state === "none" || state === "collecting";
  if (pending && now.getTime() >= today.getTime()) return { do: "collect" };
  const when = pending ? today : at(now, schedule.hour, schedule.minute, 1);
  return { do: "wait", delayMs: Math.max(0, when.getTime() - now.getTime()) };
}
