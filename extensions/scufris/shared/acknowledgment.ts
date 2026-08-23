export const ACKNOWLEDGMENT_STATE_EVENT = "scufris:acknowledgment-state";

export interface AcknowledgmentState {
  pending: boolean;
  action?: string;
}
