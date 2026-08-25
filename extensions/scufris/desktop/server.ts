import { createServer, type Server, type Socket } from "node:net";
import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import { createHash, randomBytes } from "node:crypto";
import { chmodSync, mkdirSync, realpathSync, rmSync, statSync } from "node:fs";
import { basename, dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import type { AssistantStateReport } from "../shared/assistant-state.ts";
import {
  PROTOCOL_VERSION,
  decodeClientMessage,
  encodeDaemonMessage,
  takeLines,
  type ClientMessage,
  type DaemonMessage,
} from "./protocol.ts";

/** How long a starter waits for another starter's ownership lock. */
export const OWNERSHIP_LOCK_TIMEOUT_MS = 5_000;

/**
 * How long a shutdown waits for the ownership lock before leaving its socket
 * pathname behind.
 *
 * Shutdown must not hang behind a starter. A pathname left behind is harmless:
 * the listener is already closed, so the next starter probes it, finds it dead,
 * and removes it under the same lock.
 */
export const OWNERSHIP_RELEASE_TIMEOUT_MS = 1_000;

/**
 * Submission identifiers remembered for idempotent acknowledgment. Bounded so a
 * long-lived daemon cannot grow this set without limit.
 */
export const MAX_REMEMBERED_SUBMISSIONS = 256;

/** One transcript the authoritative session has already accepted. */
export interface AcceptedSubmission {
  /** Companion-owned identifier. */
  id: string;
  /** Digest of the exact text that entered the conversation. */
  digest: string;
}

/** Returns the digest recorded for one accepted transcript. */
export function submissionDigest(text: string): string {
  return createHash("sha256").update(text, "utf8").digest("hex");
}

/**
 * One submission was refused before any of its words could leave the daemon.
 *
 * This is the definite half of a failure: the conversation never saw the
 * request, nothing it could have done was done, and the words are still only
 * the companion's. An ordinary retry of them is safe, and so is editing them,
 * which is exactly what separates this from [`SubmissionUncertainError`].
 */
export class SubmissionRefusedError extends Error {
  readonly id: string;

  constructor(id: string, reason: string) {
    super(`submission ${id} was not sent: ${reason}`);
    this.id = id;
  }
}

/** One identifier was reused for text the session did not accept. */
export class SubmissionConflictError extends SubmissionRefusedError {
  constructor(id: string) {
    super(id, "it was already accepted with different text");
  }
}

export interface ControlHost {
  /** Returns the authoritative session identity the daemon owns. */
  session(): string;
  /**
   * Delivers one accepted transcript and resolves only once it is an entry in
   * the authoritative session, durably recorded as accepted.
   *
   * Anything short of that must reject. The acknowledgment the companion
   * receives is a promise that the words are in the conversation, so it cannot
   * be sent on the strength of having merely started a send.
   *
   * `force` carries the person's own decision to send words that may already be
   * in the conversation. Nothing else may set it.
   */
  deliver(
    id: string,
    text: string,
    digest: string,
    force: boolean,
  ): Promise<void>;
  /**
   * Returns what this session has already accepted, oldest first. Read once
   * when the socket opens, so the suppression set survives a daemon restart.
   */
  accepted(): Iterable<AcceptedSubmission>;
}

/**
 * One submission may already be in the conversation, and may not be.
 *
 * Only the person can resolve that, because resolving it wrongly runs the
 * request a second time.
 */
export class SubmissionUncertainError extends Error {
  readonly id: string;

  constructor(id: string) {
    super(
      `submission ${id} was dispatched and its outcome is unknown; sending it again could repeat what it did`,
    );
    this.id = id;
  }
}

export class SocketBusyError extends Error {}

/** Identity of the exact socket file this server created. */
interface Owned {
  device: number;
  inode: number;
}

function identify(path: string): Owned | undefined {
  try {
    const stats = statSync(path);
    return { device: stats.dev, inode: stats.ino };
  } catch {
    return undefined;
  }
}

function sameFile(a: Owned | undefined, b: Owned | undefined): boolean {
  return (
    a !== undefined &&
    b !== undefined &&
    a.device === b.device &&
    a.inode === b.inode
  );
}

/** Helper that holds the kernel lock for as long as its caller wants it. */
const lockHelperPath = fileURLToPath(
  new URL("../../../tools/desktop/scufris-socket-lock", import.meta.url),
);

/**
 * Returns the one name this daemon uses for a socket pathname.
 *
 * Every name that reaches the same socket - a `.` detour, a `..` climb, a
 * symlinked parent - must produce the same lock and the same ownership checks,
 * or two daemons would guard one pathname under two different names. The parent
 * directory is resolved to its real path and the basename kept as given, which
 * is the name the socket itself is created under.
 */
export function canonicalSocketPath(socketPath: string): string {
  try {
    return join(realpathSync(dirname(socketPath)), basename(socketPath));
  } catch {
    // The directory does not exist yet. The caller creates it and asks again.
    return socketPath;
  }
}

/** Returns the lock file that guards one socket pathname. */
export function ownershipLockFile(socketPath: string): string {
  return `${canonicalSocketPath(socketPath)}.lock`;
}

/**
 * One exclusive kernel lock on a socket pathname, and the only thing that
 * changes it.
 *
 * The lock lives on the lock file's inode, so it is the same lock for every
 * name that reaches that file and it is not confined to one network namespace -
 * unlike an abstract socket, which two processes sharing this filesystem
 * through different network namespaces can both bind. Nothing is unlinked, so
 * no holder can remove a name a successor has taken.
 *
 * Node cannot take that lock, so a helper process holds it. It also does every
 * mutation of the socket pathname, and that is the point rather than a detail.
 * A caller that asked "is the helper still alive?" and then changed the
 * pathname itself would be acting on a belief the kernel may already have
 * withdrawn: the lock goes the instant the helper does, and Node learns of that
 * only when it next runs the event loop. Between those two moments a successor
 * can hold the lock while the old daemon still thinks it does. So the mutation
 * happens where the lock is, and a helper that is gone performs none.
 */
export class OwnershipLock {
  private readonly child: ChildProcessWithoutNullStreams;
  private readonly waiting: Array<(answer: string | Error) => void> = [];
  private answers = "";
  private ended?: Error;

  private constructor(child: ChildProcessWithoutNullStreams) {
    this.child = child;
    child.stdout.on("data", (chunk) => this.take(String(chunk)));
    child.once("exit", (code) =>
      this.finish(
        new SocketBusyError(
          `the desktop control socket lock was released before this daemon finished with it (helper exited with ${code})`,
        ),
      ),
    );
    child.once("error", (error) =>
      this.finish(new Error(`the socket lock helper failed: ${error}`)),
    );
  }

  /** Takes the lock, or fails once `timeoutMs` has passed. */
  static acquire(lockFile: string, timeoutMs: number): Promise<OwnershipLock> {
    return new Promise<OwnershipLock>((resolve, reject) => {
      const child = spawn(
        lockHelperPath,
        [lockFile, String(Math.max(0, Math.round(timeoutMs)))],
        { stdio: ["pipe", "pipe", "pipe"] },
      );
      let answer = "";
      let details = "";
      let settled = false;
      const settle = (error?: Error, lock?: OwnershipLock) => {
        if (settled) return;
        settled = true;
        if (error) reject(error);
        else resolve(lock as OwnershipLock);
      };
      const started = (chunk: unknown) => {
        answer += chunk;
        if (!answer.includes("\n")) return;
        child.stdout.removeListener("data", started);
        if (answer.startsWith("locked")) {
          settle(undefined, new OwnershipLock(child));
        } else if (answer.startsWith("busy")) {
          settle(
            new SocketBusyError(
              "another Scufris daemon is holding the desktop control socket lock",
            ),
          );
        }
      };
      child.stderr.on("data", (chunk) => (details += chunk));
      child.stdout.on("data", started);
      child.once("error", (error) =>
        settle(new Error(`the socket lock helper could not run: ${error}`)),
      );
      child.once("exit", (code) =>
        settle(
          code === 3
            ? new SocketBusyError(
                "another Scufris daemon is holding the desktop control socket lock",
              )
            : new Error(
                `the socket lock helper exited with ${code}${details ? `: ${details.trim()}` : ""}`,
              ),
        ),
      );
    });
  }

  /**
   * The process that holds the lock.
   *
   * Exposed because it is the honest answer to "who owns this pathname right
   * now", which logs and tests both have to ask.
   */
  get holder(): number | undefined {
    return this.child.pid;
  }

  /**
   * Links `isolated` onto `publicPath`, replacing a socket nobody is serving.
   *
   * The probe, the removal, and the link all happen in the helper, so nothing
   * between them can be a moment in which this daemon no longer holds the lock.
   */
  async claim(publicPath: string, isolated: string): Promise<void> {
    const answer = await this.command({
      command: "claim",
      public: publicPath,
      isolated,
    });
    if (answer === "claimed") return;
    if (answer === "busy") {
      throw new SocketBusyError(
        "another Scufris daemon already owns the desktop control socket",
      );
    }
    throw new SocketBusyError(
      `the desktop control socket could not be claimed: ${answer}`,
    );
  }

  /**
   * Removes `publicPath`, but only while it is still the socket `owned`
   * describes. Returns whether it was removed.
   */
  async release(publicPath: string, owned: Owned): Promise<boolean> {
    const answer = await this.command({
      command: "release",
      public: publicPath,
      device: owned.device,
      inode: owned.inode,
    });
    if (answer === "released") return true;
    if (answer === "kept") return false;
    throw new Error(
      `the desktop control socket could not be given back: ${answer}`,
    );
  }

  /** Gives the lock back and waits for the kernel to have released it. */
  async close(): Promise<void> {
    if (this.ended) return;
    this.child.stdin.end();
    await new Promise<void>((resolve) => {
      if (this.ended) return resolve();
      this.child.once("exit", () => resolve());
    });
  }

  /**
   * Sends one command and waits for the single line that answers it.
   *
   * The command is JSON, not a line of space-separated fields. A socket path is
   * whatever the person configured, and `XDG_RUNTIME_DIR` is whatever the
   * session set: both may hold spaces, quotes, or newlines, and a field
   * delimiter that any of those can be is a delimiter that loses the path.
   */
  private command(request: Record<string, unknown>): Promise<string> {
    const line = JSON.stringify(request);
    return new Promise<string>((resolve, reject) => {
      if (this.ended) return reject(this.ended);
      this.waiting.push((answer) =>
        answer instanceof Error ? reject(answer) : resolve(answer),
      );
      this.child.stdin.write(`${line}\n`, (error) => {
        if (error)
          this.finish(new Error(`the socket lock helper failed: ${error}`));
      });
    });
  }

  private take(chunk: string): void {
    this.answers += chunk;
    for (;;) {
      const index = this.answers.indexOf("\n");
      if (index === -1) break;
      const line = this.answers.slice(0, index);
      this.answers = this.answers.slice(index + 1);
      this.waiting.shift()?.(line);
    }
  }

  private finish(error: Error): void {
    if (this.ended) return;
    this.ended = error;
    for (const waiter of this.waiting.splice(0)) waiter(error);
  }
}

/** Test seams and timings. Production uses every default. */
export interface ControlServerOptions {
  /** Restricts the private socket before it is claimed. */
  harden?: (path: string) => void;
  /** How long a starter waits for the ownership lock. */
  lockTimeoutMs?: number;
  /** How long a shutdown waits for the ownership lock. */
  releaseTimeoutMs?: number;
  /**
   * Runs after the last ownership check and before each mutation of the socket
   * pathname, which is where a lock that could be lost would do damage.
   */
  beforeMutate?: () => void | Promise<void>;
}

/**
 * The daemon half of the desktop control protocol. It owns one same-user socket
 * and never trusts the peer: every line is bounded, versioned, and typed before
 * it reaches the conversation.
 */
export class ControlServer {
  private server?: Server;
  private owned?: Owned;
  private readonly clients = new Set<Socket>();
  private last: AssistantStateReport = { state: "idle", detail: "" };
  /**
   * Digests accepted under each identifier, oldest identifier first, bounded.
   *
   * One identifier can carry more than one body: a session branch can hold a
   * reused identifier over different words. Every body the conversation holds
   * is acknowledgeable, and nothing else is.
   */
  private readonly accepted = new Map<string, Set<string>>();
  /** Deliveries in progress, paired with the body each one is delivering. */
  private readonly inFlight = new Map<
    string,
    { digest: string; promise: Promise<void> }
  >();
  private readonly socketPath: string;
  private readonly host: ControlHost;
  private readonly log: (message: string, level: "info" | "error") => void;
  private readonly harden: (path: string) => void;
  private readonly lockTimeoutMs: number;
  private readonly releaseTimeoutMs: number;
  private readonly beforeMutate: () => void | Promise<void>;
  /** The one name this daemon uses for the socket, whatever name it was given. */
  private resolved: string;

  constructor(
    socketPath: string,
    host: ControlHost,
    log: (message: string, level: "info" | "error") => void,
    options: ControlServerOptions = {},
  ) {
    this.socketPath = socketPath;
    this.resolved = canonicalSocketPath(socketPath);
    this.host = host;
    this.log = log;
    this.harden = options.harden ?? ((path) => chmodSync(path, 0o600));
    this.lockTimeoutMs = options.lockTimeoutMs ?? OWNERSHIP_LOCK_TIMEOUT_MS;
    this.releaseTimeoutMs =
      options.releaseTimeoutMs ?? OWNERSHIP_RELEASE_TIMEOUT_MS;
    this.beforeMutate = options.beforeMutate ?? (() => {});
  }

  /**
   * Binds the socket and takes ownership of its pathname.
   *
   * The listener is created on a private path and hard-linked onto the public
   * one, which is atomic against an absent path. Replacing a stale socket is
   * not atomic on its own, so probe, removal, and claim all happen while
   * holding the kernel-exclusive ownership lock.
   */
  async start(): Promise<void> {
    if (this.server) return;
    const directory = dirname(this.socketPath);
    mkdirSync(directory, { recursive: true, mode: 0o700 });
    // The directory exists now, so the one name this daemon will use for the
    // socket can finally be resolved. Two daemons given different names for
    // the same pathname must agree on it, or neither would see the other.
    this.resolved = canonicalSocketPath(this.socketPath);
    // Unique per attempt: two daemons starting in the same millisecond must
    // not bind the same private path and then fight over its inode.
    const isolated = join(
      directory,
      `.daemon-${process.pid}-${randomBytes(12).toString("hex")}.sock`,
    );

    const server = createServer((socket) => this.accept(socket));
    server.on("error", (error) =>
      this.log(`control socket: ${error}`, "error"),
    );
    try {
      await new Promise<void>((resolve, reject) => {
        server.once("error", reject);
        server.listen(isolated, () => {
          server.removeListener("error", reject);
          resolve();
        });
      });
    } catch (error) {
      rmSync(isolated, { force: true });
      throw error;
    }

    // Every failure past this point must close the listener and give back the
    // pathname, or the process keeps a socket nobody can reach or stop.
    let claimed = false;
    try {
      this.harden(isolated);
      await this.withOwnershipLock((lock) => this.claim(lock, isolated));
      claimed = true;
      this.owned = identify(this.resolved);
      for (const submission of this.host.accepted()) {
        this.remember(submission.id, submission.digest);
      }
      this.server = server;
    } catch (error) {
      await new Promise<void>((resolve) => server.close(() => resolve()));
      if (claimed) await this.releaseOwnedSocket(this.owned);
      this.owned = undefined;
      throw error;
    } finally {
      rmSync(isolated, { force: true });
    }
  }

  /**
   * Runs `body` while holding the ownership lock for this socket pathname.
   *
   * The lock is the kernel's, taken on the lock file beside the socket, so it
   * covers every name that reaches that pathname and every process that shares
   * this filesystem. It is released when the holder ends, however it ends, so
   * there is no lock record to expire, to recover, or to unlink.
   */
  private async withOwnershipLock<T>(
    body: (lock: OwnershipLock) => Promise<T>,
    timeoutMs: number = this.lockTimeoutMs,
  ): Promise<T> {
    const lock = await OwnershipLock.acquire(
      ownershipLockFile(this.resolved),
      timeoutMs,
    );
    try {
      return await body(lock);
    } finally {
      await lock.close();
    }
  }

  /**
   * Gives back the socket pathname this server claimed.
   *
   * The removal happens in the process holding the lock, so no other daemon can
   * be between its own probe and its own link while this one decides, and no
   * belief about who holds the lock stands between recognising the socket and
   * removing it.
   */
  private async releaseOwnedSocket(owned: Owned | undefined): Promise<void> {
    if (!owned) return;
    try {
      await this.withOwnershipLock(async (lock) => {
        await this.beforeMutate();
        await lock.release(this.resolved, owned);
      }, this.releaseTimeoutMs);
    } catch (error) {
      // A starter holds the lock, or this daemon's own helper is gone. Either
      // way the listener is already closed, so the next starter probes the
      // pathname, finds it dead, and removes it under the same lock.
      if (!(error instanceof SocketBusyError)) throw error;
    }
  }

  private async claim(lock: OwnershipLock, isolated: string): Promise<void> {
    await this.beforeMutate();
    await lock.claim(this.resolved, isolated);
  }

  /** Closes every connection and removes the socket this server created. */
  async stop(): Promise<void> {
    const server = this.server;
    const owned = this.owned;
    this.server = undefined;
    this.owned = undefined;
    for (const client of this.clients) client.destroy();
    this.clients.clear();
    // Nothing may still be waiting to acknowledge into a socket that is gone.
    // The host settles the deliveries themselves; this drops the reservations
    // so a restarted server starts from the session, not from stale promises.
    for (const [, delivery] of this.inFlight) {
      delivery.promise.catch(() => undefined);
    }
    this.inFlight.clear();
    if (!server) return;
    await new Promise<void>((resolve) => server.close(() => resolve()));
    // Never remove a socket another daemon bound after this one started.
    await this.releaseOwnedSocket(owned);
  }

  /** Returns true while the socket is bound. */
  get listening(): boolean {
    return this.server !== undefined;
  }

  /** Sends one assistant state to every connected companion. */
  broadcast(report: AssistantStateReport): void {
    this.last = report;
    this.send({
      v: PROTOCOL_VERSION,
      type: "state",
      state: report.state,
      detail: report.detail,
    });
  }

  private send(message: DaemonMessage, only?: Socket): void {
    let line: string;
    try {
      line = encodeDaemonMessage(message);
    } catch (error) {
      this.log(`control message rejected: ${error}`, "error");
      return;
    }
    for (const client of only ? [only] : this.clients) {
      if (client.writable) client.write(line);
    }
  }

  private accept(socket: Socket): void {
    this.clients.add(socket);
    socket.setEncoding("utf8");
    let buffer = "";
    const reject = (error: unknown) => {
      const message = error instanceof Error ? error.message : String(error);
      this.log(`rejected companion message: ${message}`, "error");
      socket.destroy();
    };
    socket.on("data", (chunk: string) => {
      buffer += chunk;
      let lines: string[];
      try {
        const taken = takeLines(buffer);
        lines = taken.lines;
        buffer = taken.rest;
      } catch (error) {
        reject(error);
        return;
      }
      for (const line of lines) {
        let message: ClientMessage;
        try {
          message = decodeClientMessage(line);
        } catch (error) {
          reject(error);
          return;
        }
        // Dispatched without waiting for the previous message: a submission
        // can take as long as the agent does, and a liveness probe behind it
        // would look like a dead backend. Duplicate submissions are made safe
        // by the in-flight map, which is claimed before any await.
        void this.dispatch(socket, message).catch((error) => {
          this.log(`control dispatch failed: ${error}`, "error");
        });
      }
    });
    socket.on("error", () => socket.destroy());
    socket.on("close", () => this.clients.delete(socket));
  }

  private async dispatch(
    socket: Socket,
    message: ClientMessage,
  ): Promise<void> {
    if (message.type === "ping") {
      this.send({ v: PROTOCOL_VERSION, type: "pong" }, socket);
      return;
    }
    if (message.type === "hello") {
      this.send(
        { v: PROTOCOL_VERSION, type: "welcome", session: this.host.session() },
        socket,
      );
      this.send(
        {
          v: PROTOCOL_VERSION,
          type: "state",
          state: this.last.state,
          detail: this.last.detail,
        },
        socket,
      );
      return;
    }

    try {
      await this.deliver(
        message.id,
        message.text,
        submissionDigest(message.text),
        message.force === true,
      );
    } catch (error) {
      const detail = error instanceof Error ? error.message : String(error);
      this.log(`submission was not delivered: ${detail}`, "error");
      // Every answer carries the identifier it answers. A companion holds one
      // transcript per submission and may have started another by the time a
      // slow answer arrives, so an answer nobody can match is an answer that
      // can freeze the wrong words.
      if (error instanceof SubmissionRefusedError) {
        // Definitely unsent: the peer that asked may edit these words and
        // retry them ordinarily, and no other companion is concerned.
        this.send(
          { v: PROTOCOL_VERSION, type: "refused", id: message.id, detail },
          socket,
        );
        return;
      }
      // Uncertainty is the companion's to resolve, and only the person can
      // resolve it, so it is answered to the peer that asked rather than
      // broadcast as a daemon-wide error.
      this.send(
        { v: PROTOCOL_VERSION, type: "uncertain", id: message.id, detail },
        socket,
      );
      // A failure this daemon cannot classify is also a fault of the daemon,
      // and the tray is where that belongs.
      if (!(error instanceof SubmissionUncertainError)) {
        this.broadcast({ state: "error", detail });
      }
      return;
    }
    this.send({ v: PROTOCOL_VERSION, type: "ack", id: message.id }, socket);
  }

  /**
   * Delivers one transcript at most once.
   *
   * Concurrent retries of the same body await the same delivery. A different
   * body under an identifier that is accepted or in flight is refused, because
   * acknowledging it would tell the companion that words the conversation never
   * received had landed.
   */
  private async deliver(
    id: string,
    text: string,
    digest: string,
    force: boolean,
  ): Promise<void> {
    const known = this.accepted.get(id);
    if (known !== undefined) {
      if (!known.has(digest)) throw new SubmissionConflictError(id);
      return;
    }
    const existing = this.inFlight.get(id);
    if (existing) {
      if (existing.digest !== digest) throw new SubmissionConflictError(id);
      // A decision the person made must have an effect. Everything else waits
      // with the delivery already running rather than sending the words twice.
      if (!force) return await existing.promise;
    }

    const promise = this.host.deliver(id, text, digest, force);
    this.inFlight.set(id, { digest, promise });
    try {
      await promise;
    } finally {
      this.inFlight.delete(id);
    }
    // Suppression is only claimed once the durable record behind it exists, so
    // a failed record cannot leave this process suppressing a delivery that a
    // restart would not know about.
    this.remember(id, digest);
  }

  private remember(id: string, digest: string): void {
    const digests = this.accepted.get(id) ?? new Set<string>();
    digests.add(digest);
    this.accepted.delete(id);
    this.accepted.set(id, digests);
    while (this.accepted.size > MAX_REMEMBERED_SUBMISSIONS) {
      const oldest = this.accepted.keys().next();
      if (oldest.done) break;
      this.accepted.delete(oldest.value);
    }
  }
}
