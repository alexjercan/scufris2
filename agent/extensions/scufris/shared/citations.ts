import type { ReceiptBadge } from "../service/protocol.ts";

/** Event carrying one job's measured badges to the final-response tool. */
export const JOB_CITATION_EVENT = "scufris:job-citation";

/** Event carrying one offer a surface took back to the final-response tool. */
export const OFFER_TAKE_EVENT = "scufris:offer-take";

/** Every badge one wake measured about one job. */
export interface JobCitationSignal {
  job_id: string;
  badges: ReceiptBadge[];
}

/** One offer identifier. The words behind it never left this process. */
export interface OfferTakeSignal {
  id: string;
}

export type { ReceiptBadge };
