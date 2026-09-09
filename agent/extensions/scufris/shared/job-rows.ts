import type { JobAction, JobRow } from "../service/protocol.ts";

/** Event carrying every drawn job row from orchestration to the service. */
export const JOB_ROWS_EVENT = "scufris:job-rows";

/** Event carrying one row control a surface pressed back to orchestration. */
export const JOB_COMMAND_EVENT = "scufris:job-command";

/** One row control press. `cancel` stops the job; `archive` only files it. */
export interface JobCommandSignal {
  id: string;
  action: JobAction;
}

export type { JobRow };
