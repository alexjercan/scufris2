import {
  MAX_BADGE_BYTES,
  MAX_RECEIPTS,
  type ReceiptBadge,
} from "../service/protocol.ts";

/** Measured git, remote, and CI facts for one job.
 *
 * `false` means measured and false. A fact that could not be measured is null
 * and its reason sits in `unavailable`, so a failed fetch never reads here as
 * "not pushed".
 */
export interface Receipt {
  job_id: string;
  measured_at: string;
  trigger: string;
  commit: string | null;
  facts: Record<string, unknown>;
  claims: Array<{
    claim: string;
    said: string;
    field: string;
    measured: unknown;
    reason: string | null;
    verdict: string;
  }>;
  sentences: string[];
  unavailable: Record<string, string>;
}

/** The facts that earn a badge on their own, before any claim is read.
 *
 * Only these two, because they are the two the helper states as sentences
 * without a worker having said anything (`receipt_sentences` in
 * `tools/jobs/scufris-jobs`). Every other field becomes a badge only when a
 * claim named it.
 */
const STANDING_FACTS = ["landed", "pushed"] as const;

/** The verdict string `claim_checks` writes when nothing backs the claim. */
export const UNVERIFIED = "claimed, not verified";

function short(value: string): string {
  const clean = value.replace(/\s+/g, " ").trim();
  const bytes = Buffer.from(clean, "utf8");
  if (bytes.length <= MAX_BADGE_BYTES) return clean;
  return bytes.subarray(0, MAX_BADGE_BYTES).toString("utf8").replace(/�+$/, "");
}

function factBadge(label: string, receipt: Receipt): ReceiptBadge | undefined {
  const reason = receipt.unavailable[label];
  if (typeof reason === "string" && reason.trim())
    return { label, value: short(reason), state: "unknown" };
  const measured = receipt.facts[label];
  if (measured === true) return { label, value: "yes", state: "measured" };
  if (measured === false) return { label, value: "no", state: "refuted" };
  return undefined;
}

function claimBadge(claim: Receipt["claims"][number]): ReceiptBadge {
  if (claim.verdict === "verified")
    return { label: claim.claim, value: "yes", state: "measured" };
  // An unmeasured field is not a no. Saying "not pushed" because a fetch
  // failed would invent the one fact the receipt was careful not to claim.
  if (typeof claim.reason === "string" && claim.reason.trim())
    return { label: claim.claim, value: short(claim.reason), state: "unknown" };
  if (claim.measured === false)
    return { label: claim.claim, value: "no", state: "refuted" };
  return { label: claim.claim, value: UNVERIFIED, state: "claimed" };
}

/**
 * Turns one measured receipt into the badges a surface draws.
 *
 * No model writes these. The helper measured the facts and derived the
 * claims, and this only chooses the four words the strip has for them, in the
 * vocabulary `receipt_sentences` already uses. A claim replaces the standing
 * badge for the same field, because a claim carries both the measurement and
 * what the worker said about it.
 */
export function receiptBadges(receipt: Receipt): ReceiptBadge[] {
  const badges = new Map<string, ReceiptBadge>();
  for (const label of STANDING_FACTS) {
    const badge = factBadge(label, receipt);
    if (badge) badges.set(label, badge);
  }
  for (const claim of receipt.claims)
    badges.set(claim.claim, claimBadge(claim));
  // Uncommitted work in the workspace refutes every other fact measured from
  // the commit, so it is worth a badge of its own whenever it is true.
  if (receipt.facts.dirty === true)
    badges.set("worktree", {
      label: "worktree",
      value: "dirty",
      state: "refuted",
    });
  return [...badges.values()].slice(0, MAX_RECEIPTS);
}
