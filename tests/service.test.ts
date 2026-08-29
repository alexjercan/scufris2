import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { createServer, type Server, type Socket } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import {
  MAX_TRANSCRIPT_TEXT_BYTES,
  ProtocolError,
  decodeServiceMessage,
  encodeClientMessage,
  takeLines,
} from "../agent/extensions/scufris/service/protocol.ts";
import {
  ServiceClient,
  WidgetCommandError,
  nextBackoff,
  MAX_BACKOFF_MS,
  MIN_BACKOFF_MS,
  type WidgetNotice,
} from "../agent/extensions/scufris/service/client.ts";
import service, {
  resolveSocketPath,
} from "../agent/extensions/scufris/service/index.ts";
import { ATTENTION_NOTICE_EVENT } from "../agent/extensions/scufris/shared/attention-notice.ts";
import { SPOKEN_EVENT } from "../agent/extensions/scufris/shared/spoken.ts";
import { DESKTOP_CONTROL_EVENT } from "../agent/extensions/scufris/service/client.ts";
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

  /**
   * Waits until one line contains `needle`, and returns every line so far.
   *
   * For a test where a line before the marker may or may not be sent. Counting
   * would wait for a total that a dropped line never reaches, so the test that
   * meant to fail would hang instead.
   */
  async untilLine(needle: string): Promise<string[]> {
    while (!this.lines.some((line) => line.includes(needle))) {
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
      type: "notice",
      id: "abcdef123456",
      state: "attention",
      detail: "Job abcdef123456 is blocked",
    }),
    '{"v":3,"type":"notice","id":"abcdef123456","state":"attention",' +
      '"detail":"Job abcdef123456 is blocked"}\n',
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
  // The service answers the conversation window itself, so these two are the
  // agent's after all.
  assert.deepEqual(decodeServiceMessage('{"v":3,"type":"ok","id":"c-1"}'), {
    v: 3,
    type: "ok",
    id: "c-1",
  });
  assert.deepEqual(
    decodeServiceMessage(
      '{"v":3,"type":"refused","id":"c-1","code":"no_frontend","detail":"no screen"}',
    ),
    {
      v: 3,
      type: "refused",
      id: "c-1",
      code: "no_frontend",
      detail: "no screen",
    },
  );
  // The state, the transcript and the speech are a surface's. An agent that
  // dropped the link over one would be an agent no service could push to.
  for (const line of [
    '{"v":3,"type":"state","state":"idle","detail":""}',
    '{"v":3,"type":"speak","text":"the harness is green"}',
    '{"v":3,"type":"transcript","entry":{"speaker":"user","text":"hello"}}',
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

test("a chosen runtime directory is where the agent looks for the service", () => {
  // The directory named is the directory used, with no `scufris` below it, and
  // it outranks XDG_RUNTIME_DIR. This is what keeps a staging agent's answers
  // out of the deployed conversation: the service, the companion, `scufris-ctl`
  // and this all read the same variable, so they cannot disagree about which
  // Scufris they belong to.
  assert.equal(
    resolveSocketPath({
      XDG_RUNTIME_DIR: "/run/user/1000",
      SCUFRIS_RUNTIME_DIR: "/run/user/1000/scufris-staging",
    } as NodeJS.ProcessEnv),
    "/run/user/1000/scufris-staging/service.sock",
  );
  // A socket named outright still outranks the directory.
  assert.equal(
    resolveSocketPath({
      SCUFRIS_RUNTIME_DIR: "/run/user/1000/scufris-staging",
      SCUFRIS_SERVICE_SOCKET: "/tmp/one.sock",
    } as NodeJS.ProcessEnv),
    "/tmp/one.sock",
  );
  // Exported empty is a shell leaving something behind, not a request to put a
  // socket at the root of the filesystem.
  assert.equal(
    resolveSocketPath({
      XDG_RUNTIME_DIR: "/run/user/1000",
      SCUFRIS_RUNTIME_DIR: "",
    } as NodeJS.ProcessEnv),
    "/run/user/1000/scufris/service.sock",
  );
  assert.equal(
    resolveSocketPath({ SCUFRIS_RUNTIME_DIR: "" } as NodeJS.ProcessEnv),
    undefined,
  );
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
    client.notice({
      id: "abcdef123456",
      state: "attention",
      detail: "Job abcdef123456 is blocked",
    });
    assert.deepEqual((await listening.until(4)).slice(1), [
      '{"v":3,"type":"said","text":"the harness is green"}',
      '{"v":3,"type":"speak","text":"the harness is green"}',
      '{"v":3,"type":"notice","id":"abcdef123456","state":"attention",' +
        '"detail":"Job abcdef123456 is blocked"}',
    ]);
  } finally {
    client.stop();
    await listening.close();
    await rm(root, { recursive: true, force: true });
  }
});

test("a line too long to send is cut without breaking a character", async () => {
  const root = await scratch("bounded");
  const socketPath = join(root, "service.sock");
  const listening = await FakeService.listen(socketPath);
  const raised: string[] = [];
  const client = new ServiceClient({
    socketPath,
    widgetTimeoutMs: 200,
    log: (message, level) => {
      if (level === "error") raised.push(message);
    },
  });
  try {
    client.start();
    await listening.until(1);

    // Four UTF-8 bytes each and two UTF-16 units each, so a cut taken on
    // `string.length` lands between the halves of one of them and the encoder
    // refuses the lone surrogate that leaves. The line this exists to preserve
    // would be the thing it dropped.
    const astral = "\u{1F600}".repeat(2000);
    client.said(astral);
    // A second line, and the one waited for, so a dropped first one fails on
    // the count below rather than on a line that never arrives.
    client.said("after");
    const sent = (await listening.untilLine('"after"')).slice(1);
    assert.equal(sent.length, 2);
    const text = JSON.parse(sent[0]!).text as string;

    assert.ok(Buffer.byteLength(text, "utf8") <= MAX_TRANSCRIPT_TEXT_BYTES);
    // Every code point survived whole. A cut through one would have left an
    // unpaired half, which the encoder refuses, so the line would not be here.
    assert.equal(text, "\u{1F600}".repeat([...text].length));
    assert.ok(text.length > 0);
    // And nothing was dropped in silence.
    assert.deepEqual(raised, []);
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

test("the conversation window is settled by the service's own answer", async () => {
  const root = await scratch("conversation");
  const socketPath = join(root, "service.sock");
  const listening = await FakeService.listen(socketPath);
  const client = new ServiceClient({ socketPath, widgetTimeoutMs: 200 });
  try {
    client.start();
    await listening.until(1);

    // `ok` and `refused` rather than a report: the frontend answers nothing
    // here, because the service answered when it relayed.
    const showing = client.conversation(true);
    assert.equal(
      (await listening.until(2))[1],
      '{"v":3,"type":"conversation","id":"c-1","up":true}',
    );
    listening.push('{"v":3,"type":"ok","id":"c-1"}');
    assert.equal(await showing, undefined);

    const closing = client.conversation(false);
    assert.equal(
      (await listening.until(3))[2],
      '{"v":3,"type":"conversation","id":"c-2","up":false}',
    );
    listening.push(
      '{"v":3,"type":"refused","id":"c-2","code":"no_frontend",' +
        '"detail":"there is no frontend connected"}',
    );
    await assert.rejects(closing, (error: unknown) => {
      assert.ok(error instanceof WidgetCommandError);
      assert.equal(error.code, "no_frontend");
      return true;
    });

    // Its own counter, so a widget answer can never settle one of these.
    const waiting = client.conversation(true);
    await listening.until(4);
    listening.push(
      '{"v":3,"type":"report","report":{"type":"done","id":"c-3"}}',
    );
    await assert.rejects(waiting, (error: unknown) => {
      assert.ok(error instanceof WidgetCommandError);
      assert.equal(error.code, "timeout");
      return true;
    });
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
    api.events.on(DESKTOP_CONTROL_EVENT, (signal: any) => {
      control = signal.control;
    });
    emit("session_start");
    assert.deepEqual(await listening.until(1), [
      '{"v":3,"type":"hello","role":"agent"}',
    ]);
    assert.ok(control, "the widgets runtime is handed a control");

    api.events.emit(SPOKEN_EVENT, { said: "the harness is green" });
    api.events.emit(SPOKEN_EVENT, { speak: "the harness is green" });
    api.events.emit(ATTENTION_NOTICE_EVENT, {
      id: "abcdef123456",
      state: "attention",
      detail: "Job abcdef123456 is blocked",
    });
    assert.deepEqual((await listening.until(4)).slice(1), [
      '{"v":3,"type":"said","text":"the harness is green"}',
      '{"v":3,"type":"speak","text":"the harness is green"}',
      '{"v":3,"type":"notice","id":"abcdef123456","state":"attention",' +
        '"detail":"Job abcdef123456 is blocked"}',
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
