import { connect, type Socket } from "node:net";
import {
  MAX_TRANSCRIPT_TEXT_BYTES,
  ProtocolError,
  SERVICE_VERSION,
  encodeClientMessage,
  decodeServiceMessage,
  takeLines,
  type CatalogEntry,
  type Posture,
  type WidgetCommand,
  type WidgetReport,
} from "./protocol.ts";

/** Shortest wait before reconnecting. */
export const MIN_BACKOFF_MS = 250;

/** Longest wait before reconnecting. */
export const MAX_BACKOFF_MS = 5_000;

/** How long a widget command waits for the frontend's answer. */
export const WIDGET_ANSWER_TIMEOUT_MS = 5_000;

/** Returns the next backoff after one failed connection attempt. */
export function nextBackoff(current: number): number {
  return Math.min(current * 2, MAX_BACKOFF_MS);
}

/**
 * One widget command the frontend could not carry out.
 *
 * `code` is the frontend's own stable reason, or one of this client's three:
 * `service_unavailable` when nothing is connected to ask, `timeout` when a
 * connected frontend never answered, and `no_frontend` from the service itself
 * when there is no screen to open anything on.
 */
export class WidgetCommandError extends Error {
  readonly code: string;

  constructor(code: string, detail: string) {
    super(detail === "" ? code : detail);
    this.code = code;
  }
}

/** One widget command, without the correlation id this client assigns. */
export type WidgetRequest =
  | { type: "open"; widget: string; posture: Posture; data: unknown }
  | { type: "update"; surface: string; data: unknown }
  | { type: "close"; surface: string }
  | { type: "clear" };

/** What one carried-out widget command produced. */
export interface WidgetAnswer {
  /** Surface an open created. Absent from update, close, and clear. */
  surface?: string;
}

/** One frontend message about widgets that answers no command. */
export type WidgetNotice =
  | { type: "closed"; surface: string }
  | { type: "catalog"; widgets: CatalogEntry[] };

/**
 * Event this extension emits with the service link it holds.
 *
 * The socket and its lifetime belong to this extension. What the extensions
 * that ask the desktop for something need from it is a few verbs, so a few
 * verbs are what travel: a control while the link is up, and nothing once it
 * is not.
 */
export const DESKTOP_CONTROL_EVENT = "scufris:desktop-control";

/** The service half of the desktop, as the extensions that use it see it. */
export interface DesktopControl {
  /** Sends one command to the frontend and answers with what it produced. */
  request(command: WidgetRequest): Promise<WidgetAnswer>;
  /** Registers the one listener for messages that answer no command. */
  watchWidgets(listener: (notice: WidgetNotice) => void): void;
  /**
   * Asks for the conversation window to be up, or to be down.
   *
   * Not a toggle. The agent cannot see the screen, so a toggle would do one of
   * two opposite things and could not tell which; it says what it wants
   * instead, and asking for what is already there is harmless.
   */
  conversation(up: boolean): Promise<void>;
}

/** What [`DESKTOP_CONTROL_EVENT`] carries. */
export interface DesktopControlSignal {
  /** The control while the link is up, and nothing once it is not. */
  control?: DesktopControl;
}

/** One command waiting for its answer, whoever answers it. */
interface PendingWidget {
  resolve: (answer: WidgetAnswer) => void;
  reject: (error: WidgetCommandError) => void;
  timer: ReturnType<typeof setTimeout>;
  /**
   * The connection this command was sent on.
   *
   * Correlation ids restart at `w-1` and `c-1` with each connection, so an id
   * is only half of an answer's address. An answer that arrived after a
   * reconnect must not settle a command sent before it.
   */
  asked: Socket;
}

export interface ServiceClientOptions {
  /** Where the service listens. */
  socketPath: string;
  /** Reports what the link is doing, for the person and for the journal. */
  log?: (message: string, level: "info" | "error") => void;
  /** How long a widget command waits. Shortened by tests. */
  widgetTimeoutMs?: number;
}

/**
 * A supervised connection to the Scufris service, in the `agent` role.
 *
 * It reconnects with a bounded backoff, because the service restarts its agent
 * and the agent outlives none of that: the terminal `pi` a debug lease opens
 * connects the same way, and a service that is not up yet is a service that
 * will be.
 *
 * Everything an agent says is one-way except a widget command, which carries a
 * correlation id the frontend echoes.
 */
export class ServiceClient implements DesktopControl {
  private readonly socketPath: string;
  private readonly log: (message: string, level: "info" | "error") => void;
  private readonly widgetTimeoutMs: number;
  private socket?: Socket;
  private buffer = "";
  private backoff = MIN_BACKOFF_MS;
  private retry?: ReturnType<typeof setTimeout>;
  private stopped = false;
  private widgetCommands = 0;
  private conversationCommands = 0;
  private readonly pendingWidgets = new Map<string, PendingWidget>();
  /**
   * Commands the service answers itself, waiting on `ok` or `refused`.
   *
   * Its own map rather than a second kind of entry in the widget one. The two
   * counters are independent, so nothing stops `w-3` and `c-3` being in flight
   * at once, and one map would let a widget report settle a conversation
   * command that happened to share its number.
   */
  private readonly pendingAnswers = new Map<string, PendingWidget>();
  private notices?: (notice: WidgetNotice) => void;

  constructor(options: ServiceClientOptions) {
    this.socketPath = options.socketPath;
    this.log = options.log ?? (() => {});
    this.widgetTimeoutMs = options.widgetTimeoutMs ?? WIDGET_ANSWER_TIMEOUT_MS;
  }

  /** Opens the connection and keeps it open until [`stop`] is called. */
  start(): void {
    this.stopped = false;
    this.open();
  }

  /** Closes the connection and stops reconnecting. */
  stop(): void {
    this.stopped = true;
    if (this.retry) clearTimeout(this.retry);
    this.retry = undefined;
    const socket = this.socket;
    this.socket = undefined;
    this.buffer = "";
    this.abandon("service_unavailable", "The Scufris service link is closed.");
    socket?.destroy();
  }

  /** True while the link is connected and writable. */
  get connected(): boolean {
    return this.socket?.writable === true;
  }

  /**
   * Puts one line the assistant said on the transcript.
   *
   * The service cannot read this off the event stream: Scufris answers through
   * a tool call rather than an assistant text block.
   */
  said(text: string): void {
    const line = bounded(text);
    if (line) this.tell({ v: SERVICE_VERSION, type: "said", text: line });
  }

  /**
   * Hands one line to whatever owns the speaker.
   *
   * The agent decides what is worth saying aloud. The frontend synthesises it
   * and may refuse it, which is why nothing here waits for an answer.
   */
  speak(text: string): void {
    const line = bounded(text);
    if (line) this.tell({ v: SERVICE_VERSION, type: "speak", text: line });
  }

  /**
   * Registers the one listener for messages that answer no command.
   *
   * The catalog and a surface the person closed arrive unasked, so they belong
   * to whoever owns widgets rather than to the caller of a command.
   */
  watchWidgets(listener: (notice: WidgetNotice) => void): void {
    this.notices = listener;
  }

  /**
   * Sends one widget command and resolves with what the frontend produced.
   *
   * A command nothing can carry - no link, no answer, or a frontend that says
   * no - rejects with a [`WidgetCommandError`] the caller reports as a tool
   * result.
   */
  request(command: WidgetRequest): Promise<WidgetAnswer> {
    const socket = this.socket;
    if (!socket?.writable) {
      return Promise.reject(
        new WidgetCommandError(
          "service_unavailable",
          "The Scufris service is not connected.",
        ),
      );
    }
    this.widgetCommands += 1;
    const id = `w-${this.widgetCommands}`;
    let line: string;
    try {
      line = encodeClientMessage({
        v: SERVICE_VERSION,
        type: "widget",
        // Rebuilt rather than spread, so the correlation identifier is written
        // where the protocol documents it: the type, then the id, then the rest.
        command: { type: command.type, id, ...rest(command) } as WidgetCommand,
      });
    } catch (error) {
      // The protocol's own code, where it has one. "Too large" and "not text"
      // are different things for the model to do something about, and both are
      // different from a message this client could not build at all.
      if (error instanceof ProtocolError) {
        return Promise.reject(
          new WidgetCommandError(error.code, error.message),
        );
      }
      const detail = error instanceof Error ? error.message : String(error);
      return Promise.reject(new WidgetCommandError("invalid_command", detail));
    }
    return new Promise<WidgetAnswer>((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pendingWidgets.delete(id);
        reject(
          new WidgetCommandError(
            "timeout",
            "The frontend did not answer the widget command.",
          ),
        );
      }, this.widgetTimeoutMs);
      // A waiting command must never be the reason this process stays alive.
      timer.unref?.();
      this.pendingWidgets.set(id, { resolve, reject, timer, asked: socket });
      socket.write(line);
    });
  }

  /**
   * Asks for the conversation window to be up, or to be down.
   *
   * The service answers this one itself rather than the frontend, so what
   * comes back is `ok` or `refused` rather than a report. There is nothing to
   * carry back on success: the window is the frontend's own, and the only
   * failure the agent can act on is that there is no screen.
   */
  conversation(up: boolean): Promise<void> {
    const socket = this.socket;
    if (!socket?.writable) {
      return Promise.reject(
        new WidgetCommandError(
          "service_unavailable",
          "The Scufris service is not connected.",
        ),
      );
    }
    this.conversationCommands += 1;
    const id = `c-${this.conversationCommands}`;
    const line = encodeClientMessage({
      v: SERVICE_VERSION,
      type: "conversation",
      id,
      up,
    });
    return new Promise<void>((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pendingAnswers.delete(id);
        reject(
          new WidgetCommandError(
            "timeout",
            "The service did not answer the conversation command.",
          ),
        );
      }, this.widgetTimeoutMs);
      timer.unref?.();
      this.pendingAnswers.set(id, {
        resolve: () => resolve(),
        reject,
        timer,
        asked: socket,
      });
      socket.write(line);
    });
  }

  /** Writes one message, or reports that there was nowhere to write it. */
  private tell(message: Parameters<typeof encodeClientMessage>[0]): void {
    const socket = this.socket;
    if (!socket?.writable) return;
    try {
      socket.write(encodeClientMessage(message));
    } catch (error) {
      this.log(`${message.type} was not sent: ${String(error)}`, "info");
    }
  }

  private open(): void {
    if (this.stopped) return;
    const socket = connect(this.socketPath);
    this.socket = socket;
    this.buffer = "";
    socket.setEncoding("utf8");
    // The link must never be the reason this process stays alive: the agent's
    // own event loop decides when Pi exits.
    socket.unref();
    socket.on("connect", () => {
      this.backoff = MIN_BACKOFF_MS;
      socket.write(
        encodeClientMessage({
          v: SERVICE_VERSION,
          type: "hello",
          role: "agent",
        }),
      );
      this.log("connected to the Scufris service", "info");
    });
    socket.on("data", (chunk: string) => this.receive(socket, chunk));
    // Both ends of goodbye reach the same place, so a link that failed to
    // connect and one that was closed under us are handled once.
    socket.once("error", () => this.lost(socket));
    socket.once("close", () => this.lost(socket));
  }

  private receive(socket: Socket, chunk: string): void {
    this.buffer += chunk;
    let taken: { lines: string[]; rest: string };
    try {
      taken = takeLines(this.buffer);
    } catch (error) {
      this.log(`the service sent something unreadable: ${error}`, "error");
      socket.destroy();
      return;
    }
    this.buffer = taken.rest;
    for (const line of taken.lines) {
      if (line.length === 0) continue;
      let message;
      try {
        message = decodeServiceMessage(line);
      } catch (error) {
        this.log(`the service sent something unreadable: ${error}`, "error");
        socket.destroy();
        return;
      }
      if (message?.type === "report") this.settle(socket, message.report);
      // The service's own answers. Only one verb this client sends is answered
      // this way, and a pending it does not match is an answer to nothing.
      if (message?.type === "ok") this.answer(socket, message.id);
      if (message?.type === "refused") {
        this.answer(socket, message.id, message.code, message.detail);
      }
    }
  }

  /** Settles the command one `ok` or `refused` answers, if it is still there. */
  private answer(
    from: Socket,
    id: string,
    code?: string,
    detail?: string,
  ): void {
    const pending = this.pendingAnswers.get(id);
    if (!pending || pending.asked !== from) {
      this.log(`no command is waiting for ${id}`, "info");
      return;
    }
    this.pendingAnswers.delete(id);
    clearTimeout(pending.timer);
    if (code === undefined) {
      pending.resolve({});
      return;
    }
    pending.reject(new WidgetCommandError(code, detail ?? ""));
  }

  /** Applies one widget report to the command waiting for it, if one is. */
  private settle(from: Socket, report: WidgetReport): void {
    if (report.type === "catalog") {
      this.notices?.({ type: "catalog", widgets: report.widgets });
      return;
    }
    if (report.type === "closed") {
      this.notices?.({ type: "closed", surface: report.surface });
      return;
    }
    this.settleWidget(from, report);
  }

  private settleWidget(
    from: Socket,
    report: Exclude<WidgetReport, { type: "catalog" | "closed" }>,
  ): void {
    const pending = this.pendingWidgets.get(report.id);
    if (!pending || pending.asked !== from) {
      if (report.type === "opened") {
        // The open was given up on and the panel arrived anyway. Nothing holds
        // its surface identifier now, so nobody but the person could ever put
        // it away, and Scufris cannot even name it to say what it shows.
        // Closing it is the only honest end to a tool call that has failed.
        this.log(
          `closing ${report.surface}, whose open was already given up on`,
          "info",
        );
        void this.request({ type: "close", surface: report.surface }).catch(
          () => {},
        );
        return;
      }
      // Late, duplicated, or from a connection this client no longer has. It is
      // reported and dropped: applying it would act on nothing.
      this.log(`no widget command is waiting for ${report.id}`, "info");
      return;
    }
    this.pendingWidgets.delete(report.id);
    clearTimeout(pending.timer);
    if (report.type === "failed") {
      pending.reject(new WidgetCommandError(report.code, report.detail));
      return;
    }
    pending.resolve(
      report.type === "opened" ? { surface: report.surface } : {},
    );
  }

  private lost(socket: Socket): void {
    if (this.socket !== socket) return;
    this.socket = undefined;
    this.buffer = "";
    socket.destroy();
    this.abandon(
      "service_unavailable",
      "The Scufris service link was lost before it answered.",
    );
    if (this.stopped) return;
    const wait = this.backoff;
    this.backoff = nextBackoff(this.backoff);
    this.retry = setTimeout(() => this.open(), wait);
    this.retry.unref?.();
  }

  /** Fails every waiting command, because none of them can be answered. */
  private abandon(code: string, detail: string): void {
    for (const waiting of [this.pendingWidgets, this.pendingAnswers]) {
      for (const [, pending] of waiting) {
        clearTimeout(pending.timer);
        pending.reject(new WidgetCommandError(code, detail));
      }
      waiting.clear();
    }
  }
}

/** Returns one command's fields with its type taken out. */
function rest(command: WidgetRequest): Record<string, unknown> {
  const { type: _type, ...fields } = command;
  return fields;
}

/**
 * Trims one line to what the service accepts.
 *
 * A long answer is worth saying most of. Refusing it outright would drop the
 * only record of what Scufris said, so it is cut at the bound instead.
 */
function bounded(text: string): string {
  let cut = text.trim();
  while (Buffer.byteLength(cut, "utf8") > MAX_TRANSCRIPT_TEXT_BYTES) {
    cut = cut.slice(0, Math.floor(cut.length * 0.9));
  }
  return cut;
}
