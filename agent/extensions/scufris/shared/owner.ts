/** Who owns the jobs the foreground delegates. */

/** The token that stays with the conversation across every handoff. */
export const FOREGROUND_OWNER = "foreground";

/** Launcher environment that names the owner of the managed child's work. */
export const JOB_OWNER_VARIABLE = "SCUFRIS_JOB_OWNER";

/**
 * Event announcing who owns foreground work from now on.
 *
 * The service binding says when it changed; orchestration decides what to do
 * about it. Neither module imports the other, so the token travels here.
 */
export const OWNER_EVENT = "scufris:owner";

export interface OwnerSignal {
  /** The token to record as the owner of foreground work. */
  owner: string;
  /** True when the agent moved to another process rather than stopping. */
  handoff?: boolean;
}
