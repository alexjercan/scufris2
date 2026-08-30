import { connect, type Socket } from "node:net";
import {
  AGENT_FILE_NAME,
  SERVICE_VERSION,
  decodeAgentResponse,
  encodeAgentRequest,
  surfacePrompt,
  takeLines,
  type AgentRequest,
  type WidgetCall,
} from "./protocol.ts";

export const MIN_BACKOFF_MS = 250;
export const MAX_BACKOFF_MS = 5_000;
export const UPDATE_TOGETHER =
  "The Scufris protocol handshake failed. Update the host and surface together.";
export const AGENT_RESPONSE_EVENT = "scufris:agent-response";

export interface AtomicResponse {
  text: string;
  details?: string;
  widgets?: WidgetCall[];
  attachments?: string[];
}

export function nextBackoff(current: number): number {
  return Math.min(current * 2, MAX_BACKOFF_MS);
}

export interface AgentClientOptions {
  socketPath: string;
  sendUserMessage: (message: string, busy: boolean) => void;
  abort: () => void;
  busy: () => boolean;
  log?: (message: string, level: "info" | "error") => void;
}

export class AgentClient {
  private readonly options: AgentClientOptions;
  private socket?: Socket;
  private buffer = "";
  private retry?: ReturnType<typeof setTimeout>;
  private backoff = MIN_BACKOFF_MS;
  private stopped = false;
  private ready = false;
  private readonly log: NonNullable<AgentClientOptions["log"]>;

  constructor(options: AgentClientOptions) {
    this.options = options;
    this.log = options.log ?? (() => {});
  }

  start(): void {
    this.stopped = false;
    this.open();
  }

  stop(): void {
    this.stopped = true;
    if (this.retry) clearTimeout(this.retry);
    this.retry = undefined;
    this.socket?.destroy();
    this.socket = undefined;
    this.ready = false;
    this.buffer = "";
  }

  response(response: AtomicResponse): void {
    this.tell({ v: SERVICE_VERSION, type: "agent.response", ...response });
  }

  state(state: "failed" | "blocked" | "clear", detail: string): void {
    this.tell({ v: SERVICE_VERSION, type: "agent.state", state, detail });
  }

  private tell(message: AgentRequest): void {
    if (!this.ready || !this.socket?.writable) return;
    try {
      this.socket.write(encodeAgentRequest(message));
    } catch (error) {
      this.log(`${message.type} was not sent: ${String(error)}`, "error");
    }
  }

  private open(): void {
    if (this.stopped) return;
    const socket = connect(this.options.socketPath);
    this.socket = socket;
    this.ready = false;
    this.buffer = "";
    socket.setEncoding("utf8");
    socket.unref();
    socket.on("connect", () => {
      socket.write(
        encodeAgentRequest({ v: SERVICE_VERSION, type: "agent.hello" }),
      );
    });
    socket.on("data", (chunk: string) => this.receive(socket, chunk));
    socket.once("error", () => this.lost(socket));
    socket.once("close", () => this.lost(socket));
  }

  private receive(socket: Socket, chunk: string): void {
    this.buffer += chunk;
    let framed;
    try {
      framed = takeLines(this.buffer);
    } catch (error) {
      this.log(UPDATE_TOGETHER, "error");
      socket.destroy();
      return;
    }
    this.buffer = framed.rest;
    for (const line of framed.lines) {
      if (!line) continue;
      try {
        const message = decodeAgentResponse(line);
        if (message.type === "agent.ready") {
          this.ready = true;
          this.backoff = MIN_BACKOFF_MS;
        } else if (message.type === "agent.message") {
          this.options.sendUserMessage(
            surfacePrompt(message.text, message.widgets, message.attachments),
            this.options.busy(),
          );
        } else if (message.type === "agent.abort") {
          this.options.abort();
        } else {
          this.log(`${message.code}: ${message.detail}`, "error");
        }
      } catch {
        this.log(UPDATE_TOGETHER, "error");
        socket.destroy();
        return;
      }
    }
  }

  private lost(socket: Socket): void {
    if (this.socket !== socket) return;
    const handshakeFailed = !this.ready;
    this.socket = undefined;
    this.ready = false;
    this.buffer = "";
    socket.destroy();
    if (handshakeFailed) this.log(UPDATE_TOGETHER, "error");
    if (this.stopped) return;
    const wait = this.backoff;
    this.backoff = nextBackoff(this.backoff);
    this.retry = setTimeout(() => this.open(), wait);
    this.retry.unref?.();
  }
}

export { AGENT_FILE_NAME };
