import { AsyncLocalStorage } from "node:async_hooks";
import {
  SubmissionConflictError,
  SubmissionRefusedError,
  SubmissionUncertainError,
  submissionDigest,
  type AcceptedSubmission,
} from "./server.ts";

/**
 * Session entry recording that one submission was handed to Pi.
 *
 * Written before the send, so it survives whatever happens next. It says only
 * that the words were dispatched, which is precisely what makes a crash between
 * dispatch and acceptance recognisable afterwards instead of invisible.
 */
export const DISPATCH_ENTRY = "scufris-desktop-dispatch-v1";

/**
 * Session entry committing that one submission entered the conversation.
 *
 * Written after Pi has persisted the prompt, and naming that entry by its own
 * identifier. Adjacency proves nothing - a session is a tree, a branch can be
 * taken at any entry, and a process can die between two appends - so the commit
 * binds the submission to the exact entry it accepted and reconciliation trusts
 * nothing else.
 */
export const ACCEPTED_ENTRY = "scufris-desktop-accepted-v1";

/**
 * The entry earlier builds wrote before a prompt landed.
 *
 * It cannot prove acceptance, because nothing bound it to the prompt that
 * followed it. A session that holds one is read as a dispatch: the words may be
 * in the conversation, and that is all it says.
 */
export const LEGACY_RECEIPT_ENTRY = "scufris-desktop-transcript-v1";

/** How long a submission may take to appear in the session. */
export const LANDING_TIMEOUT_MS = 10 * 60 * 1000;

/**
 * How often a pending delivery re-reads the session.
 *
 * Pi's event stream is the fast path, but it is not a guarantee: a provider
 * that stalls after the entry lands emits nothing further, and the
 * acknowledgment must not wait on an event that never comes.
 */
export const LANDING_POLL_MS = 250;

/** What one dispatch records about the submission it belongs to. */
export interface TranscriptDispatch {
  version: 1;
  id: string;
  /** Digest of the words the submission asked Pi to send. */
  digest: string;
}

/** What one commit records about the prompt it accepted. */
export interface TranscriptCommit extends TranscriptDispatch {
  /** Session entry identifier of the prompt these words landed as. */
  entry: string;
}

/** What a session says about one submission. */
export type SubmissionState =
  /** The conversation holds these words because this submission asked for it. */
  | "accepted"
  /** The words were dispatched, and whether they landed is not knowable. */
  | "uncertain"
  /** Nothing was ever dispatched under this identifier. */
  | "unsent"
  /** This identifier carries words the conversation does not hold. */
  | "conflict";

/** The parts of a Pi session this module reads and writes. */
export interface SessionView {
  /** Returns the current branch entries, oldest first. */
  branch(): readonly unknown[];
  /**
   * Returns the entry the next append becomes a child of, if the session has
   * one. Read while a prompt is landing, it names the place that prompt is
   * about to take.
   */
  leaf(): string | undefined;
  /**
   * Records that these words are about to be handed to Pi, durably, before any
   * of it happens. A dispatch nobody can see is a request nobody can be sure
   * about.
   */
  dispatch(id: string, digest: string): void;
  /**
   * Submits one transcript as a user prompt. The result is not observable; the
   * entries it produces are.
   */
  send(id: string, text: string, digest: string): void;
  /**
   * Records that `entry` is the prompt this submission became. Written only
   * once that entry exists, and naming it, so what is written can be checked
   * later against the entry it names.
   */
  commit(submission: TranscriptDispatch, entry: string): void;
}

/** One submission was abandoned because its session went away. */
export class SessionClosedError extends Error {
  constructor() {
    super("the Scufris session closed before the submission was accepted");
  }
}

/** Returns the custom data one session entry of `customType` carries. */
function customData(entry: unknown, customType: string): unknown {
  if (typeof entry !== "object" || entry === null) return undefined;
  const candidate = entry as {
    type?: unknown;
    customType?: unknown;
    data?: unknown;
  };
  if (candidate.type !== "custom" || candidate.customType !== customType) {
    return undefined;
  }
  return candidate.data;
}

/** Returns the identifier and digest a dispatch or commit records. */
function submissionOf(data: unknown): TranscriptDispatch | undefined {
  const record = data as Partial<TranscriptDispatch> | undefined;
  if (
    record?.version !== 1 ||
    typeof record.id !== "string" ||
    record.id.length === 0 ||
    typeof record.digest !== "string" ||
    record.digest.length === 0
  ) {
    return undefined;
  }
  return { version: 1, id: record.id, digest: record.digest };
}

/** Returns the dispatch one session entry records, if this daemon wrote it. */
export function transcriptDispatch(
  entry: unknown,
): TranscriptDispatch | undefined {
  return (
    submissionOf(customData(entry, DISPATCH_ENTRY)) ??
    // An entry an earlier build wrote before its prompt landed says only that
    // the words were dispatched. Reading it as anything more is what let a
    // branch or an orphan acknowledge words nobody had proved.
    submissionOf(customData(entry, LEGACY_RECEIPT_ENTRY))
  );
}

/** Returns the acceptance one session entry commits, if this daemon wrote it. */
export function transcriptCommit(entry: unknown): TranscriptCommit | undefined {
  const data = customData(entry, ACCEPTED_ENTRY);
  const submission = submissionOf(data);
  const record = data as Partial<TranscriptCommit> | undefined;
  if (
    !submission ||
    typeof record?.entry !== "string" ||
    record.entry.length === 0
  ) {
    return undefined;
  }
  return { ...submission, entry: record.entry };
}

/** Returns one session entry's own identifier. */
export function entryId(entry: unknown): string | undefined {
  if (typeof entry !== "object" || entry === null) return undefined;
  const id = (entry as { id?: unknown }).id;
  return typeof id === "string" && id.length > 0 ? id : undefined;
}

/** Returns the digest of one persisted user message, if the entry is one. */
export function userMessageDigest(entry: unknown): string | undefined {
  if (typeof entry !== "object" || entry === null) return undefined;
  const candidate = entry as {
    type?: unknown;
    message?: { role?: unknown; content?: unknown };
  };
  if (candidate.type !== "message" || candidate.message?.role !== "user") {
    return undefined;
  }
  const text = messageText(candidate.message.content);
  return text === undefined ? undefined : submissionDigest(text);
}

/** Returns the text of one message's content. */
export function messageText(content: unknown): string | undefined {
  if (typeof content === "string") return content;
  if (!Array.isArray(content)) return undefined;
  const parts = content.filter(
    (part): part is { type: "text"; text: string } =>
      typeof part === "object" &&
      part !== null &&
      (part as { type?: unknown }).type === "text" &&
      typeof (part as { text?: unknown }).text === "string",
  );
  return parts.length === 0
    ? undefined
    : parts.map((part) => part.text).join("");
}

/** How many announced prompts one ledger keeps waiting for their landing. */
const MAX_ANNOUNCED_PROMPTS = 64;

/** One prompt Pi has announced and not yet delivered. */
interface Announcement {
  digest: string;
  /** The submission this prompt is, when that is provable. */
  receipt?: TranscriptDispatch;
  /** True while this is the only prompt Pi has in flight. */
  alone: boolean;
}

/**
 * Decides which landed prompt belongs to which submission, and refuses to
 * decide when it cannot.
 *
 * Pi tells an input handler the source *class* of a prompt - `extension` for
 * every extension alike - so the class cannot say whose prompt it is. What can
 * is the asynchronous context: Pi announces a prompt from inside the
 * `sendUserMessage` call that started it, so an announcement that arrives
 * inside this daemon's own call is this daemon's own prompt and no other's.
 *
 * That proves whose the announcement is. It does not prove which landing is
 * that announcement's: a handler after this one may rewrite a prompt or answer
 * it outright, and neither is visible here. So a landing is credited only while
 * that announcement is the only prompt Pi has in flight. Anything else - a
 * prompt typed while this one waits, a second extension's prompt, a rewritten
 * prompt, a prompt that never lands - leaves the submission uncredited, and an
 * uncredited submission is retained by the pill rather than acknowledged.
 *
 * That is the safe direction, and it is deliberate: Pi's public API cannot tell
 * this daemon which landing is its own, so this daemon under-credits rather
 * than guess.
 */
export class ReceiptLedger {
  private readonly context = new AsyncLocalStorage<TranscriptDispatch>();
  private readonly announced: Announcement[] = [];

  /** Runs one send inside the context that identifies its announcement. */
  send(receipt: TranscriptDispatch, submit: () => void): void {
    this.context.run(receipt, submit);
  }

  /** Records one prompt Pi announced, whoever it belongs to. */
  announce(digest: string): void {
    const own = this.context.getStore();
    // A handler ahead of this one may have rewritten the words. The receipt
    // describes what the person said, so rewritten words are not it.
    const receipt = own?.digest === digest ? own : undefined;
    const alone = this.announced.length === 0;
    for (const entry of this.announced) entry.alone = false;
    this.announced.push({ digest, receipt, alone });
    // A prompt that a later handler answered by itself never lands. Nothing
    // here may grow without bound waiting for it.
    while (this.announced.length > MAX_ANNOUNCED_PROMPTS)
      this.announced.shift();
  }

  /** Claims one landed prompt, returning the submission it completes. */
  land(digest: string): TranscriptDispatch | undefined {
    const head = this.announced.shift();
    if (!head?.alone || head.digest !== digest) return undefined;
    return head.receipt;
  }

  clear(): void {
    this.announced.length = 0;
  }
}

/**
 * Returns every body this session holds, keyed by submission identifier.
 *
 * A commit names the prompt it accepted by that prompt's own entry identifier,
 * and is written only after Pi has persisted it. Acceptance is therefore read
 * back the way it was written: find the commit, find the entry it names on this
 * branch, and check that the entry is a prompt carrying exactly those words.
 *
 * Nothing weaker will do. A session is a tree, so a branch can be taken at any
 * entry and leave a record with a stranger behind it, and a process can die
 * between two appends and leave a record with nothing behind it at all. Both
 * shapes look identical to a rule that reads adjacency.
 *
 * One identifier can carry more than one body, because a branch can hold a
 * reused identifier over different words. Each of those bodies is in the
 * conversation, so each one is acknowledgeable and nothing else is.
 */
export function landings(branch: readonly unknown[]): Map<string, Set<string>> {
  /** Digest of every prompt on this branch, by its entry identifier. */
  const prompts = new Map<string, string>();
  const landed = new Map<string, Set<string>>();
  for (const entry of branch) {
    const digest = userMessageDigest(entry);
    if (digest !== undefined) {
      const id = entryId(entry);
      if (id !== undefined) prompts.set(id, digest);
      continue;
    }
    const commit = transcriptCommit(entry);
    if (!commit) continue;
    // The prompt this commit accepted must be on this branch, ahead of the
    // commit, and still carry the words the commit claims for it.
    if (prompts.get(commit.entry) !== commit.digest) continue;
    const digests = landed.get(commit.id) ?? new Set<string>();
    digests.add(commit.digest);
    landed.delete(commit.id);
    landed.set(commit.id, digests);
  }
  return landed;
}

/** Where the prompt that follows one entry is, when the branch holds one. */
type FollowingPrompt =
  /** The branch does not hold that entry, so no prompt on it follows one. */
  | "gone"
  /** The branch holds the entry and no prompt follows it yet. */
  | "waiting"
  /** The first prompt after the entry, by its own identifier. */
  | { entry: string; digest: string };

/**
 * Returns the first prompt appended after `anchor` on this branch.
 *
 * Pi appends a message as a child of the current leaf, so the leaf read while a
 * prompt is landing is the entry that prompt is about to follow. Everything
 * between them - another extension's own entry, this daemon's own records - is
 * skipped, and the first prompt after them is the one that landed.
 *
 * An `anchor` this branch does not hold is not this prompt's branch, and an
 * entry without an identifier can name nothing.
 */
function promptAfter(
  branch: readonly unknown[],
  anchor: string | undefined,
): FollowingPrompt {
  let index = 0;
  if (anchor !== undefined) {
    const at = branch.findIndex((entry) => entryId(entry) === anchor);
    if (at === -1) return "gone";
    index = at + 1;
  }
  for (; index < branch.length; index += 1) {
    const digest = userMessageDigest(branch[index]);
    if (digest === undefined) continue;
    const entry = entryId(branch[index]);
    return entry === undefined ? "gone" : { entry, digest };
  }
  return "waiting";
}

/** Returns every body this session dispatched, keyed by identifier. */
export function dispatches(
  branch: readonly unknown[],
): Map<string, Set<string>> {
  const sent = new Map<string, Set<string>>();
  for (const entry of branch) {
    const dispatch = transcriptDispatch(entry);
    if (!dispatch) continue;
    const digests = sent.get(dispatch.id) ?? new Set<string>();
    digests.add(dispatch.digest);
    sent.delete(dispatch.id);
    sent.set(dispatch.id, digests);
  }
  return sent;
}

/**
 * Returns what this session says about one submission.
 *
 * There are three answers and not two. A request that was dispatched and never
 * committed may have entered the conversation and run whatever it asked for, or
 * may have died before it did, and this daemon cannot tell which. Calling that
 * "not delivered" is what would let the same request run twice.
 */
export function submissionState(
  branch: readonly unknown[],
  id: string,
  digest: string,
): SubmissionState {
  const accepted = landings(branch).get(id);
  if (accepted?.size) return accepted.has(digest) ? "accepted" : "conflict";
  const sent = dispatches(branch).get(id);
  if (sent?.size) return sent.has(digest) ? "uncertain" : "conflict";
  return "unsent";
}

interface Waiter {
  id: string;
  digest: string;
  settle: (error?: Error) => void;
}

/** One submission whose prompt is landing, and the place it will land in. */
interface PendingCommit {
  submission: TranscriptDispatch;
  /** The leaf Pi held while the prompt was being persisted. */
  anchor: string | undefined;
}

/**
 * Delivers companion submissions into one authoritative Pi session.
 *
 * Pi's send APIs report nothing back and its session manager is read-only, so
 * starting a send proves nothing. Acceptance is observed instead, and it is
 * committed to the session as it is observed, so a restart reads the answer
 * rather than inferring one.
 */
export class SessionAcceptance {
  private readonly session: SessionView;
  private readonly timeoutMs: number;
  private readonly pollMs: number;
  /** Submissions this process has already handed to Pi. */
  private readonly dispatched = new Set<string>();
  private readonly waiting = new Set<Waiter>();
  private poll?: ReturnType<typeof setInterval>;
  /** The one landing whose commit has not been written yet, if there is one. */
  private landing?: PendingCommit;

  constructor(
    session: SessionView,
    timeoutMs: number = LANDING_TIMEOUT_MS,
    pollMs: number = LANDING_POLL_MS,
  ) {
    this.session = session;
    this.timeoutMs = timeoutMs;
    this.pollMs = pollMs;
  }

  /** Returns what this session has accepted, read straight from the branch. */
  accepted(limit: number): AcceptedSubmission[] {
    const accepted = acceptedSubmissions(this.session.branch(), limit);
    this.dispatched.clear();
    return accepted;
  }

  /**
   * Records that one prompt is being persisted, and whose it is when the
   * ledger could prove that.
   *
   * Called while Pi is delivering a user message, before the entry exists: Pi
   * appends it only once every extension has seen the event, so what can be
   * captured now is the leaf the prompt is about to follow.
   *
   * Every landing is recorded, whoever sent it, and a landing supersedes the
   * one before it. A commit may therefore only be written for the prompt that
   * landed with it and while no other prompt has landed since - which is the
   * only window in which the entry after the anchor cannot be somebody else's.
   */
  landed(submission?: TranscriptDispatch): void {
    this.landing = submission
      ? { submission, anchor: this.session.leaf() }
      : undefined;
  }

  /** Re-reads the session for every pending delivery. */
  notify(): void {
    this.commit();
    if (this.waiting.size === 0) return;
    const landed = landings(this.session.branch());
    for (const waiter of [...this.waiting]) {
      if (landed.get(waiter.id)?.has(waiter.digest)) waiter.settle();
    }
  }

  /** Abandons every pending delivery, for example when the session is replaced. */
  reset(): void {
    this.dispatched.clear();
    // A commit this process has not written yet cannot be written against a
    // session it no longer has.
    this.landing = undefined;
    for (const waiter of [...this.waiting]) {
      waiter.settle(new SessionClosedError());
    }
  }

  /**
   * Writes the commit for the landing in hand, once its entry exists.
   *
   * The entry is the first prompt after the anchor, so the commit names the
   * entry this submission became rather than whichever entry happens to carry
   * the same words. A branch that no longer holds the anchor, and a prompt
   * carrying other words, both end the attempt: the submission stays uncertain,
   * which is the honest answer and the safe one.
   */
  private commit(): void {
    const landing = this.landing;
    if (!landing) return;
    const found = promptAfter(this.session.branch(), landing.anchor);
    // Pi appends the entry after every extension has seen the event, and an
    // extension of its own may be slow, so an empty place is waited on.
    if (found === "waiting") return;
    this.landing = undefined;
    if (found === "gone" || found.digest !== landing.submission.digest) return;
    this.session.commit(landing.submission, found.entry);
  }

  /**
   * Delivers one submission, or explains why it will not.
   *
   * `force` is the person's own decision to send words that may already be in
   * the conversation. Nothing else may take it: a timeout, a reset, a restart,
   * and an ordinary retry all leave an uncertain submission exactly as it is,
   * because sending it again could repeat everything it did the first time.
   */
  async deliver(
    id: string,
    text: string,
    digest: string,
    force = false,
  ): Promise<void> {
    const state = submissionState(this.session.branch(), id, digest);
    if (state === "accepted") return;
    if (state === "conflict") throw new SubmissionConflictError(id);
    if (force) {
      // The person's own decision, and the only thing that can send words that
      // may already be in the conversation. It always sends: refusing it would
      // leave nothing that ever could.
      this.dispatched.add(id);
      this.handOver(id, text, digest);
      return await this.awaitLanding(id, digest);
    }
    if (state === "uncertain" && !this.dispatched.has(id)) {
      // Dispatched by some earlier attempt this process knows nothing about.
      // It may have run. Only the person may decide to run it again.
      throw new SubmissionUncertainError(id);
    }
    // Sent once per process. A retry that arrives while the first send is
    // still waiting waits with it rather than sending the words twice.
    if (!this.dispatched.has(id)) {
      this.dispatched.add(id);
      this.handOver(id, text, digest);
    }
    // The submission stays dispatched whatever happens next: once the words
    // have left, this process no longer knows what became of them, and that is
    // exactly what must not be forgotten.
    return await this.awaitLanding(id, digest);
  }

  /**
   * Records one submission and hands it to Pi, in that order.
   *
   * A failure before the record exists is a refusal: nothing was handed over,
   * nothing was written, and the words are known to be where they started. A
   * failure after it may have left words behind, so it is not.
   */
  private handOver(id: string, text: string, digest: string): void {
    try {
      // Durable before anything can lose it. A send nobody recorded is a send
      // nobody can be uncertain about, and that certainty is what decides
      // whether it may ever be sent again.
      this.session.dispatch(id, digest);
    } catch (error) {
      this.dispatched.delete(id);
      throw new SubmissionRefusedError(
        id,
        error instanceof Error ? error.message : String(error),
      );
    }
    try {
      this.session.send(id, text, digest);
    } catch (error) {
      // The dispatch above is durable, so the next attempt reads uncertain
      // rather than sending words that may already have gone.
      this.dispatched.delete(id);
      throw error;
    }
  }

  private awaitLanding(id: string, digest: string): Promise<void> {
    if (landings(this.session.branch()).get(id)?.has(digest)) {
      return Promise.resolve();
    }
    return new Promise<void>((resolve, reject) => {
      const waiter: Waiter = {
        id,
        digest,
        settle: (error) => {
          if (!this.waiting.delete(waiter)) return;
          clearTimeout(timer);
          this.stopPolling();
          if (error) reject(error);
          else resolve();
        },
      };
      const timer = setTimeout(
        () => waiter.settle(new SubmissionUncertainError(id)),
        this.timeoutMs,
      );
      // A pending delivery must never hold the process open by itself.
      timer.unref?.();
      this.waiting.add(waiter);
      this.startPolling();
    });
  }

  private startPolling(): void {
    if (this.poll || this.pollMs <= 0) return;
    this.poll = setInterval(() => this.notify(), this.pollMs);
    this.poll.unref?.();
  }

  private stopPolling(): void {
    if (this.waiting.size > 0 || !this.poll) return;
    clearInterval(this.poll);
    this.poll = undefined;
  }
}

/**
 * Returns every submission this session already holds, oldest first, for the
 * newest `limit` identifiers.
 */
export function acceptedSubmissions(
  branch: readonly unknown[],
  limit: number,
): AcceptedSubmission[] {
  return [...landings(branch)]
    .slice(-limit)
    .flatMap(([id, digests]) => [...digests].map((digest) => ({ id, digest })));
}
