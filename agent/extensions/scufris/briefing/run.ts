/** What names a briefing run, and what its record says about it.
 *
 * The clock arithmetic that used to live here is systemd's now: a timer per
 * profile decides when a briefing happens, and `Persistent=true` catches up
 * one the machine was off for. What is left is the two things a session still
 * has to know without asking a model or a timer.
 */

/** A run directory's state, as the helper reports it. */
export type RunState =
  | "none"
  | "collecting"
  | "collected"
  | "delivered"
  | "failed";

/** The profile a caller that names none is asking about. */
export const DEFAULT_PROFILE = "morning";

/** The local date, which is half the name of a run. */
export function localDate(now: Date): string {
  const month = `${now.getMonth() + 1}`.padStart(2, "0");
  const day = `${now.getDate()}`.padStart(2, "0");
  return `${now.getFullYear()}-${month}-${day}`;
}
