import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { createServer, type Server, type Socket } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import {
  ProtocolError,
  decodeServiceMessage,
  encodeClientMessage,
  takeLines,
} from "../extensions/scufris/service/protocol.ts";
import {
  ServiceClient,
  WidgetCommandError,
  nextBackoff,
  MAX_BACKOFF_MS,
  MIN_BACKOFF_MS,
  type WidgetNotice,
} from "../extensions/scufris/service/client.ts";
import service, {
  resolveSocketPath,
} from "../extensions/scufris/service/index.ts";
import { SPOKEN_EVENT } from "../extensions/scufris/shared/spoken.ts";
import { WIDGET_CONTROL_EVENT } from "../extensions/scufris/service/client.ts";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";

/** One service standing in for the real one, on a socket in a scratch directory. */
class FakeService {
  readonly lines: string[] = [];
  private readonly server: Server;
  private connection?: Socket;
  private buffer = "";
  private waiting?: () => void;

  private constructor(server: Server) {
    this.server = server;
    server.on("connection", (socket) => {
      this.connection = socket;
      socket.setEncoding("utf8");
      socket.on("data", (chunk: string) => {
        this.buffer += chunk;
        const taken = takeLines(this.buffer);
        this.buffer = taken.rest;
        this.lines.push(...taken.lines);
        this.waiting?.();
      });
    });
  }

  static async listen(path: string): Promise<FakeService> {
    const server = createServer();
    await new Promise<void>((resolve) => server.listen(path, resolve));
    server.unref();
    return new FakeService(server);
  }

  /** Pushes one raw line, as the service would. */
  push(line: string): void {
    this.connection?.write(`${line}\n`);
  }

  /** Waits until at least `count` lines have arrived. */
  async until(count: number): Promise<string[]> {
    while (this.lines.length < count) {
      await new Promise<void>((resolve) => {
        this.waiting = resolve;
      });
    }
    this.waiting = undefined;
    return this.lines;
  }

  async close(): Promise<void> {
    this.connection?.destroy();
    await new Promise<void>((resolve) => this.server.close(() => resolve()));
  }
}

async function scratch(name: string): Promise<string> {
  return await mkdtemp(join(tmpdir(), `scufris-${name}-`));
}

test("an agent encodes only what an agent may say", () => {
  assert.equal(
    encodeClientMessage({ v: 3, type: "hello", role: "agent" }),
    '{"v":3,"type":"hello","role":"agent"}\n',
  );
  assert.equal(
    encodeClientMessage({ v: 3, type: "said", text: "the harness is green" }),
    '{"v":3,"type":"said","text":"the harness is green"}\n',
  );
  assert.equal(
    encodeClientMessage({
      v: 3,
      type: "widget",
      command: {
        type: "open",
        id: "w-1",
        widget: "clock",
        posture: "exhibit",
        data: {},
      },
    }),
    '{"v":3,"type":"widget","command":{"type":"open","id":"w-1",' +
      '"widget":"clock","posture":"exhibit","data":{}}}\n',
  );
});

test("a payload the frontend would refuse is refused here, with its own code", () => {
  // Refused before the line is written, so an oversized payload is a tool-call
  // error the model can act on rather than a dropped connection.
  const oversized = () =>
    encodeClientMessage({
      v: 3,
      type: "widget",
      command: {
        type: "update",
        id: "w-1",
        surface: "clock-1",
        data: { text: "x".repeat(9_000) },
      },
    });
  assert.throws(oversized, (error: unknown) => {
    assert.ok(error instanceof ProtocolError);
    assert.equal(error.code, "widget_data_too_large");
    return true;
  });

  const named = () =>
    encodeClientMessage({
      v: 3,
      type: "widget",
      command: { type: "close", id: "w-1", surface: "clock 1" },
    });
  assert.throws(named, (error: unknown) => {
    assert.ok(error instanceof ProtocolError);
    assert.equal(error.code, "invalid_widget");
    return true;
  });

  const lonely = () =>
    encodeClientMessage({ v: 3, type: "said", text: `broken \ud800 half` });
  assert.throws(lonely, (error: unknown) => {
    assert.ok(error instanceof ProtocolError);
    assert.equal(error.code, "not_well_formed");
    return true;
  });
});

test("an agent reads what is addressed to it and ignores the rest", () => {
  assert.deepEqual(
    decodeServiceMessage('{"v":3,"type":"welcome","role":"agent"}'),
    { v: 3, type: "welcome", role: "agent" },
  );
  assert.deepEqual(
    decodeServiceMessage(
      '{"v":3,"type":"report","report":{"type":"opened","id":"w-1","surface":"clock-1"}}',
    ),
    {
      v: 3,
      type: "report",
      report: { type: "opened", id: "w-1", surface: "clock-1" },
    },
  );
  // The state, the transcript and the speech are a surface's. An agent that
  // dropped the link over one would be an agent no service could push to.
  for (const line of [
    '{"v":3,"type":"state","state":"idle","detail":""}',
    '{"v":3,"type":"speak","text":"the harness is green"}',
    '{"v":3,"type":"ok","id":"c-1"}',
  ]) {
    assert.equal(decodeServiceMessage(line), undefined, line);
  }
  // Version 2 is gone. A peer that speaks it is told which version it spoke.
  assert.throws(
    () => decodeServiceMessage('{"v":2,"type":"welcome","session":"popup-1"}'),
    (error: unknown) => {
      assert.ok(error instanceof ProtocolError);
      assert.equal(error.code, "unsupported_version");
      return true;
    },
  );
});

test("the backoff grows and stays bounded", () => {
  let backoff = MIN_BACKOFF_MS;
  for (let attempt = 0; attempt < 10; attempt += 1) {
    backoff = nextBackoff(backoff);
  }
  assert.equal(backoff, MAX_BACKOFF_MS);
  assert.equal(nextBackoff(MIN_BACKOFF_MS), MIN_BACKOFF_MS * 2);
});

test("the socket is named beside the runtime directory, and configurable", () => {
  assert.equal(
    resolveSocketPath({
      XDG_RUNTIME_DIR: "/run/user/1000",
    } as NodeJS.ProcessEnv),
    "/run/user/1000/scufris/service.sock",
  );
  assert.equal(
    resolveSocketPath({
      XDG_RUNTIME_DIR: "/run/user/1000",
      SCUFRIS_SERVICE_SOCKET: "/tmp/one.sock",
    } as NodeJS.ProcessEnv),
    "/tmp/one.sock",
  );
  assert.equal(resolveSocketPath({} as NodeJS.ProcessEnv), undefined);
});

test("the client says hello as an agent and carries what it is told to say", async () => {
  const root = await scratch("client");
  const socketPath = join(root, "service.sock");
  const listening = await FakeService.listen(socketPath);
  const client = new ServiceClient({ socketPath, widgetTimeoutMs: 200 });
  try {
    client.start();
    assert.deepEqual(await listening.until(1), [
      '{"v":3,"type":"hello","role":"agent"}',
    ]);

    client.said("the harness is green");
    client.speak("the harness is green");
    assert.deepEqual((await listening.until(3)).slice(1), [
      '{"v":3,"type":"said","text":"the harness is green"}',
      '{"v":3,"type":"speak","text":"the harness is green"}',
    ]);
  } finally {
    client.stop();
    await listening.close();
    await rm(root, { recursive: true, force: true });
  }
});

test("a widget command is settled by the answer that names it", async () => {
  const root = await scratch("widget");
  const socketPath = join(root, "service.sock");
  const listening = await FakeService.listen(socketPath);
  const client = new ServiceClient({ socketPath, widgetTimeoutMs: 200 });
  const notices: WidgetNotice[] = [];
  try {
    client.watchWidgets((notice) => notices.push(notice));
    client.start();
    await listening.until(1);

    // The catalog arrives unasked, so it belongs to the watcher rather than to
    // the caller of any command.
    listening.push(
      '{"v":3,"type":"report","report":{"type":"catalog","widgets":' +
        '[{"id":"clock","name":"Clock","description":"The time."}]}}',
    );
    const opening = client.request({
      type: "open",
      widget: "clock",
      posture: "exhibit",
      data: {},
    });
    assert.deepEqual(
      (await listening.until(2))[1],
      '{"v":3,"type":"widget","command":{"type":"open","id":"w-1",' +
        '"widget":"clock","posture":"exhibit","data":{}}}',
    );
    listening.push(
      '{"v":3,"type":"report","report":{"type":"opened","id":"w-1","surface":"clock-1"}}',
    );
    assert.deepEqual(await opening, { surface: "clock-1" });

    // The service answers this itself when there is no screen to open on, so
    // the tool call ends rather than hanging.
    const failing = client.request({ type: "clear" });
    await listening.until(3);
    listening.push(
      '{"v":3,"type":"report","report":{"type":"failed","id":"w-2",' +
        '"code":"no_frontend","detail":"there is no frontend connected"}}',
    );
    await assert.rejects(failing, (error: unknown) => {
      assert.ok(error instanceof WidgetCommandError);
      assert.equal(error.code, "no_frontend");
      return true;
    });

    // Nothing answers this one.
    await assert.rejects(
      client.request({ type: "close", surface: "clock-1" }),
      (error: unknown) => {
        assert.ok(error instanceof WidgetCommandError);
        assert.equal(error.code, "timeout");
        return true;
      },
    );

    assert.deepEqual(notices, [
      {
        type: "catalog",
        widgets: [{ id: "clock", name: "Clock", description: "The time." }],
      },
    ]);
  } finally {
    client.stop();
    await listening.close();
    await rm(root, { recursive: true, force: true });
  }
});

test("a command with no link is refused rather than queued", async () => {
  const client = new ServiceClient({ socketPath: "/nonexistent/service.sock" });
  await assert.rejects(client.request({ type: "clear" }), (error: unknown) => {
    assert.ok(error instanceof WidgetCommandError);
    assert.equal(error.code, "service_unavailable");
    return true;
  });
  client.stop();
});

test("only the foreground Scufris connects, and it carries what was said", async () => {
  const root = await scratch("extension");
  const socketPath = join(root, "service.sock");
  const listening = await FakeService.listen(socketPath);
  const role = process.env.SCUFRIS_ROLE;
  const configured = process.env.SCUFRIS_SERVICE_SOCKET;
  process.env.SCUFRIS_SERVICE_SOCKET = socketPath;

  const handlers = new Map<string, Array<(event: any, context: any) => any>>();
  const bus = new Map<string, Array<(data: unknown) => void>>();
  const api = {
    events: {
      emit(channel: string, data: unknown) {
        for (const handler of bus.get(channel) ?? []) handler(data);
      },
      on(channel: string, handler: (data: unknown) => void) {
        const listeners = bus.get(channel) ?? [];
        listeners.push(handler);
        bus.set(channel, listeners);
        return () => {};
      },
    },
    on(event: string, handler: (event: any, context: any) => any) {
      const eventHandlers = handlers.get(event) ?? [];
      eventHandlers.push(handler);
      handlers.set(event, eventHandlers);
    },
  } as unknown as ExtensionAPI;
  const context = { hasUI: false, ui: { notify() {} } };
  const emit = (event: string, value: unknown = {}) => {
    for (const handler of handlers.get(event) ?? []) handler(value, context);
  };

  try {
    // A worker Pi has no conversation to report and no screen to ask for, and a
    // second agent would take the foreground one's place on the socket.
    process.env.SCUFRIS_ROLE = "worker";
    service(api);
    assert.equal(handlers.size, 0);

    process.env.SCUFRIS_ROLE = "orchestrator";
    service(api);
    let control: unknown;
    api.events.on(WIDGET_CONTROL_EVENT, (signal: any) => {
      control = signal.control;
    });
    emit("session_start");
    assert.deepEqual(await listening.until(1), [
      '{"v":3,"type":"hello","role":"agent"}',
    ]);
    assert.ok(control, "the widgets runtime is handed a control");

    api.events.emit(SPOKEN_EVENT, { said: "the harness is green" });
    api.events.emit(SPOKEN_EVENT, { speak: "the harness is green" });
    assert.deepEqual((await listening.until(3)).slice(1), [
      '{"v":3,"type":"said","text":"the harness is green"}',
      '{"v":3,"type":"speak","text":"the harness is green"}',
    ]);

    // Withdrawn before the link closes, so nothing sends a command into a
    // connection that is being taken down under it.
    emit("session_shutdown");
    assert.equal(control, undefined);
  } finally {
    if (role === undefined) delete process.env.SCUFRIS_ROLE;
    else process.env.SCUFRIS_ROLE = role;
    if (configured === undefined) delete process.env.SCUFRIS_SERVICE_SOCKET;
    else process.env.SCUFRIS_SERVICE_SOCKET = configured;
    await listening.close();
    await rm(root, { recursive: true, force: true });
  }
});
