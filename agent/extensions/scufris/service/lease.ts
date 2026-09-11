/** The terminal lease: one control connection that holds the agent.
 *
 * The service stops its managed `pi --mode rpc` child when the lease is
 * granted and starts it again when this connection closes, whether or not a
 * release was said first. The grant carries a generation, and the agent client
 * says hello with it: that is the writer fence, so a terminal whose lease has
 * ended cannot go on writing to the conversation.
 *
 * The connection is also the heartbeat. A terminal that is suspended rather
 * than killed keeps an open socket that answers nothing, so the socket alone
 * never says the agent is gone; a ping every five seconds does.
 */
import { connect, type Socket } from "node:net";
import { join } from "node:path";
import {
  CONTROL_FILE_NAME,
  SERVICE_VERSION,
  SOCKET_DIRECTORY_NAME,
  decodeControlResponse,
  encodeControlRequest,
  takeLines,
  type AgentHolder,
  type ControlRequest,
  type ControlResponse,
  type ConversationEntry,
  type LeaseHolderInfo,
  type ScufrisState,
} from "./protocol.ts";

/** How long a normal request may take. */
export const REQUEST_TIMEOUT_MS = 5_000;
/** Acquire blocks while the managed child is stopped and reaped. */
export const ACQUIRE_TIMEOUT_MS = 20_000;
/** The service ends a lease after three missed pings. */
export const LEASE_PING_INTERVAL_MS = 5_000;

export interface LeaseGrant {
  generation: number;
  sessionDir: string;
  /** The session the service will fork from, and the one to have forked. */
  lineageFile?: string;
  /** Where a catch-up read starts. */
  sequence: number;
  /** The job-ownership token that moved with the agent. */
  owner: string;
}

export interface ServiceState {
  state: ScufrisState;
  detail: string;
  holder: AgentHolder;
  generation?: number;
  /** Where every session in the lineage is written. */
  sessionDir: string;
  lineageFile?: string;
}

export interface ConversationPage {
  entries: ConversationEntry[];
  more: boolean;
}

/** What the session binding needs from whatever holds the lease. */
export interface LeaseSource {
  /** Acquire once; a second call while held resolves with the same grant. */
  acquire(holder: LeaseHolderInfo, abortWorking?: boolean): Promise<LeaseGrant>;
  /** Give the agent back. Resolves once the service has acknowledged it. */
  release(): Promise<void>;
  /** Called whenever a held lease ends without a release. */
  onLost(handler: (reason: string) => void): void;
  /** What the service says about itself, with or without a lease. */
  state?(): Promise<ServiceState>;
}

export class LeaseRefused extends Error {
  readonly code: string;
  constructor(code: string, detail: string) {
    super(detail || code);
    this.name = "LeaseRefused";
    this.code = code;
  }
}

export function resolveControlSocketPath(
  environment: NodeJS.ProcessEnv = process.env,
): string | undefined {
  if (environment.SCUFRIS_CONTROL_SOCKET)
    return environment.SCUFRIS_CONTROL_SOCKET;
  if (environment.SCUFRIS_RUNTIME_DIR)
    return join(environment.SCUFRIS_RUNTIME_DIR, CONTROL_FILE_NAME);
  if (environment.XDG_RUNTIME_DIR)
    return join(
      environment.XDG_RUNTIME_DIR,
      SOCKET_DIRECTORY_NAME,
      CONTROL_FILE_NAME,
    );
  return undefined;
}

interface Waiting {
  resolve: (message: ControlResponse) => void;
  reject: (error: Error) => void;
  timer: ReturnType<typeof setTimeout>;
}

export class LeaseClient implements LeaseSource {
  private readonly socketPath: string;
  private socket?: Socket;
  private buffer = "";
  private grant?: LeaseGrant;
  private acquiring?: Promise<LeaseGrant>;
  /** One waiter per request identifier, so a ping never eats a page. */
  private readonly waiting = new Map<string, Waiting>();
  private ready?: Waiting;
  private lost?: (reason: string) => void;
  private released = false;
  private requests = 0;
  private heartbeat?: ReturnType<typeof setInterval>;

  constructor(socketPath: string) {
    this.socketPath = socketPath;
  }

  get held(): LeaseGrant | undefined {
    return this.grant;
  }

  onLost(handler: (reason: string) => void): void {
    this.lost = handler;
  }

  async acquire(
    holder: LeaseHolderInfo,
    abortWorking = false,
  ): Promise<LeaseGrant> {
    if (this.grant) return this.grant;
    this.acquiring ??= this.take(holder, abortWorking).finally(() => {
      this.acquiring = undefined;
    });
    return this.acquiring;
  }

  /** Reads the service state, with or without a lease of its own. */
  async state(): Promise<ServiceState> {
    await this.connected();
    const answer = await this.ask(
      { v: SERVICE_VERSION, type: "control.state", id: this.nextId() },
      REQUEST_TIMEOUT_MS,
    );
    if (answer.type !== "control.state")
      throw this.unexpected(answer, "control.state");
    return {
      state: answer.state,
      detail: answer.detail,
      holder: answer.holder ?? "managed",
      ...(answer.generation === undefined
        ? {}
        : { generation: answer.generation }),
      sessionDir: answer.session_dir,
      ...(answer.lineage_file === undefined
        ? {}
        : { lineageFile: answer.lineage_file }),
    };
  }

  /** One bounded page of canonical entries after `since`, oldest first. */
  async conversation(since: number): Promise<ConversationPage> {
    await this.connected();
    const answer = await this.ask(
      {
        v: SERVICE_VERSION,
        type: "control.conversation",
        id: this.nextId(),
        since,
      },
      REQUEST_TIMEOUT_MS,
    );
    if (answer.type !== "control.conversation_entries")
      throw this.unexpected(answer, "control.conversation_entries");
    return { entries: answer.entries, more: answer.more === true };
  }

  async release(): Promise<void> {
    this.stopHeartbeat();
    if (!this.grant || !this.socket) {
      this.close();
      return;
    }
    this.released = true;
    try {
      const answer = await this.ask(
        {
          v: SERVICE_VERSION,
          type: "control.lease_release",
          id: this.nextId(),
        },
        REQUEST_TIMEOUT_MS,
      );
      if (answer.type === "control.rejected")
        throw new LeaseRefused(answer.code, answer.detail);
    } finally {
      this.grant = undefined;
      this.close();
    }
  }

  private async take(
    holder: LeaseHolderInfo,
    abortWorking: boolean,
  ): Promise<LeaseGrant> {
    try {
      await this.connected();
      const answer = await this.ask(
        {
          v: SERVICE_VERSION,
          type: "control.lease_acquire",
          id: this.nextId(),
          holder,
          ...(abortWorking ? { abort_working: true } : {}),
        },
        ACQUIRE_TIMEOUT_MS,
      );
      if (answer.type === "control.rejected")
        throw new LeaseRefused(answer.code, answer.detail);
      if (answer.type !== "control.lease")
        throw this.unexpected(answer, "control.lease");
      this.grant = {
        generation: answer.generation,
        sessionDir: answer.session_dir,
        ...(answer.lineage_file === undefined
          ? {}
          : { lineageFile: answer.lineage_file }),
        sequence: answer.sequence,
        owner: answer.owner,
      };
      this.released = false;
      this.startHeartbeat();
      return this.grant;
    } catch (error) {
      this.close();
      throw error instanceof Error ? error : new Error(String(error));
    }
  }

  private startHeartbeat(): void {
    this.stopHeartbeat();
    const timer = setInterval(() => {
      if (!this.grant || !this.socket) return;
      void this.ask(
        { v: SERVICE_VERSION, type: "control.lease_ping", id: this.nextId() },
        REQUEST_TIMEOUT_MS,
      ).then(
        (answer) => {
          // A refused ping means the service already gave the agent back.
          if (answer.type === "control.rejected")
            this.drop(`heartbeat refused: ${answer.code}`);
        },
        (error: unknown) =>
          this.drop(
            `heartbeat failed: ${error instanceof Error ? error.message : String(error)}`,
          ),
      );
    }, LEASE_PING_INTERVAL_MS);
    timer.unref?.();
    this.heartbeat = timer;
  }

  private stopHeartbeat(): void {
    if (this.heartbeat === undefined) return;
    clearInterval(this.heartbeat);
    this.heartbeat = undefined;
  }

  private unexpected(answer: ControlResponse, wanted: string): Error {
    return new Error(
      `the control channel answered ${answer.type} instead of ${wanted}`,
    );
  }

  private nextId(): string {
    this.requests += 1;
    return `lease-${process.pid}-${this.requests}`;
  }

  /** Opens the socket and says hello, at most once per connection. */
  private async connected(): Promise<void> {
    if (this.socket) return;
    await this.open();
    const ready = await new Promise<ControlResponse>((resolve, reject) => {
      const timer = setTimeout(() => {
        this.ready = undefined;
        reject(new Error("the control channel did not answer in time"));
      }, REQUEST_TIMEOUT_MS);
      timer.unref?.();
      this.ready = { resolve, reject, timer };
      this.write({ v: SERVICE_VERSION, type: "control.hello" }, (error) => {
        clearTimeout(timer);
        this.ready = undefined;
        reject(error);
      });
    });
    if (ready.type !== "control.ready")
      throw this.unexpected(ready, "control.ready");
  }

  private open(): Promise<void> {
    return new Promise((resolve, reject) => {
      const socket = connect(this.socketPath);
      this.socket = socket;
      this.buffer = "";
      socket.setEncoding("utf8");
      socket.once("connect", () => resolve());
      socket.once("error", (error) => {
        if (this.socket !== socket) return;
        this.ended(`error: ${error.message}`);
        reject(error);
      });
      socket.once("close", () => {
        if (this.socket !== socket) return;
        this.ended("closed");
      });
      socket.on("data", (chunk: string) => this.receive(socket, chunk));
    });
  }

  private ask(message: ControlRequest, timeoutMs: number) {
    return new Promise<ControlResponse>((resolve, reject) => {
      if (message.type === "control.hello") {
        reject(new Error("hello is not a correlated request"));
        return;
      }
      if (!this.socket) {
        reject(new Error("the control channel is not open"));
        return;
      }
      const key = message.id;
      const timer = setTimeout(() => {
        this.waiting.delete(key);
        reject(new Error("the control channel did not answer in time"));
      }, timeoutMs);
      timer.unref?.();
      this.waiting.set(key, { resolve, reject, timer });
      this.write(message, (error) => {
        this.waiting.delete(key);
        clearTimeout(timer);
        reject(error);
      });
    });
  }

  private write(
    message: ControlRequest,
    onError: (error: Error) => void,
  ): void {
    try {
      this.socket?.write(encodeControlRequest(message));
    } catch (error) {
      onError(error instanceof Error ? error : new Error(String(error)));
    }
  }

  private receive(socket: Socket, chunk: string): void {
    this.buffer += chunk;
    let framed;
    try {
      framed = takeLines(this.buffer);
    } catch (error) {
      this.fail(error instanceof Error ? error : new Error(String(error)));
      socket.destroy();
      return;
    }
    this.buffer = framed.rest;
    for (const line of framed.lines) {
      if (!line) continue;
      let message: ControlResponse;
      try {
        message = decodeControlResponse(line);
      } catch (error) {
        this.fail(error instanceof Error ? error : new Error(String(error)));
        socket.destroy();
        return;
      }
      if (message.type === "control.ready") {
        const ready = this.ready;
        this.ready = undefined;
        if (ready) {
          clearTimeout(ready.timer);
          ready.resolve(message);
        }
        continue;
      }
      const waiting = this.waiting.get(message.id);
      if (!waiting) continue;
      this.waiting.delete(message.id);
      clearTimeout(waiting.timer);
      waiting.resolve(message);
    }
  }

  private fail(error: Error): void {
    const ready = this.ready;
    this.ready = undefined;
    if (ready) {
      clearTimeout(ready.timer);
      ready.reject(error);
    }
    for (const waiting of this.waiting.values()) {
      clearTimeout(waiting.timer);
      waiting.reject(error);
    }
    this.waiting.clear();
  }

  private ended(reason: string): void {
    this.stopHeartbeat();
    this.fail(new Error(`the control channel ended: ${reason}`));
    this.socket = undefined;
    this.buffer = "";
    const wasHeld = this.grant !== undefined && !this.released;
    this.grant = undefined;
    if (wasHeld) {
      const lost = this.lost;
      lost?.(reason);
    }
  }

  /** Ends a held lease this side noticed first, and says so. */
  private drop(reason: string): void {
    const held = this.grant !== undefined && !this.released;
    this.grant = undefined;
    this.close();
    if (held) this.lost?.(reason);
  }

  private close(): void {
    this.stopHeartbeat();
    const socket = this.socket;
    this.socket = undefined;
    this.buffer = "";
    socket?.destroy();
  }
}
