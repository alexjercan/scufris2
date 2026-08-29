/** Event carrying one unattended job's durable notice to the service link. */
export const ATTENTION_NOTICE_EVENT = "scufris:attention-notice";

/** One unattended job raising, replacing, or clearing its own tray notice. */
export interface AttentionNoticeSignal {
  /** Stable job identifier. A job may clear only the notice under this key. */
  id: string;
  /** What the tray should show, or that this job no longer needs it. */
  state: "attention" | "error" | "clear";
  /** Short human-readable reason, empty when clearing. */
  detail: string;
}
