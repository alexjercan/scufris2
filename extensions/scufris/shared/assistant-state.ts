export const ATTENTION_STATE_EVENT = "scufris:attention-state";

/**
 * One unattended job asking for the person, or saying it no longer needs them.
 *
 * The workflow extension raises this. Nothing shows it yet: version 3 of the
 * protocol has no `attention` state, because routing a dialog to a surface is
 * work the textbox increment does. The signal stays because it is the workflow
 * extension's own, and losing it would mean building it again.
 */
export interface AttentionStateSignal {
  state: "attention" | "error" | "clear";
  detail: string;
}
