import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createServer, type Server, type Socket } from "node:net";
import test from "node:test";
import type {
  ExtensionAPI,
  ExtensionCommandContext,
  ExtensionContext,
} from "@earendil-works/pi-coding-agent";
import {
  LeaseClient,
  LeaseRefused,
  resolveControlSocketPath,
  type LeaseGrant,
  type LeaseSource,
  type ServiceState,
} from "../agent/extensions/scufris/service/lease.ts";
import {
  decodeControlResponse,
  encodeAgentRequest,
  encodeControlRequest,
  recordedText,
  MAX_TEXT_BYTES,
  REFUSAL,
} from "../agent/extensions/scufris/service/protocol.ts";
import {
  bindService,
  catchUpContent,
  CATCH_UP_TYPE,
  OWNER_EVENT,
  type OwnerSignal,
} from "../agent/extensions/scufris/service/index.ts";
import type {
  AgentClientOptions,
  AtomicResponse,
} from "../agent/extensions/scufris/service/client.ts";
import {
  bindTerminal,
  fileTrace,
  terminalEnabled,
  type TerminalControl,
} from "../agent/extensions/scufris/terminal/index.ts";
import scufrisTerminal from "../.pi/extensions/scufris-terminal/index.ts";

const SESSION = "/srv/sessions/one.jsonl";
const LINEAGE = "/srv/sessions/lineage.jsonl";

/** A control socket that answers like the host and remembers what it heard. */
async function fakeControl(
  behaviour: "grant" | "refuse" | "drop" = "grant",
): Promise<{
  server: Server;
  socketPath: string;
  requests: string[];
  sockets: Socket[];
  close: () => Promise<void>;
}> {
  const directory = await mkdtemp(join(tmpdir(), "scufris-terminal-"));
  const socketPath = join(directory, "control.sock");
  const requests: string[] = [];
  const sockets: Socket[] = [];
  const server = createServer((socket) => {
    sockets.push(socket);
    socket.setEncoding("utf8");
    let buffer = "";
    const say = (message: Record<string, unknown>) =>
      socket.write(`${JSON.stringify({ v: 11, ...message })}\n`);
    socket.on("data", (chunk: string) => {
      buffer += chunk;
      const lines = buffer.split("\n");
      buffer = lines.pop() ?? "";
      for (const line of lines) {
        const message = JSON.parse(line) as Record<string, string>;
        requests.push(message.type ?? "");
        if (message.type === "control.hello") say({ type: "control.ready" });
        else if (message.type === "control.lease_acquire") {
          if (behaviour === "refuse")
            say({
              type: "control.rejected",
              id: message.id,
              code: REFUSAL.LEASE_DISABLED,
              detail: "Not here.",
            });
          else if (behaviour === "drop") socket.destroy();
          else
            say({
              type: "control.lease",
              id: message.id,
              generation: 4,
              session_dir: "/srv/sessions",
              lineage_file: LINEAGE,
              sequence: 12,
              owner: "foreground",
            });
        } else if (message.type === "control.lease_ping")
          say({ type: "control.lease_pong", id: message.id, generation: 4 });
        else if (message.type === "control.lease_release")
          say({ type: "control.lease_released", id: message.id });
        else if (message.type === "control.conversation")
          say({
            type: "control.conversation_entries",
            id: message.id,
            entries: [
              { sequence: 13, role: "user", surface: "ios", text: "Hello." },
            ],
            more: true,
          });
        else if (message.type === "control.state")
          say({
            type: "control.state",
            id: message.id,
            state: "idle",
            detail: "",
            holder: "terminal",
            generation: 4,
            session_dir: "/srv/sessions",
            lineage_file: LINEAGE,
          });
      }
    });
  });
  await new Promise<void>((resolve) => server.listen(socketPath, resolve));
  return {
    server,
    socketPath,
    requests,
    sockets,
    close: async () => {
      for (const socket of sockets) socket.destroy();
      await new Promise<void>((resolve) => server.close(() => resolve()));
      await rm(directory, { recursive: true, force: true });
    },
  };
}

test("the control socket resolves the way the agent socket does", () => {
  assert.equal(
    resolveControlSocketPath({ SCUFRIS_RUNTIME_DIR: "/run/scufris" }),
    "/run/scufris/control.sock",
  );
  assert.equal(
    resolveControlSocketPath({ XDG_RUNTIME_DIR: "/run/user/1" }),
    "/run/user/1/scufris/control.sock",
  );
  assert.equal(
    resolveControlSocketPath({ SCUFRIS_CONTROL_SOCKET: "/x/c.sock" }),
    "/x/c.sock",
  );
  assert.equal(resolveControlSocketPath({}), undefined);
});

test("the lease client holds one grant, pages history, and releases it", async () => {
  const control = await fakeControl();
  try {
    const client = new LeaseClient(control.socketPath);
    const lost: string[] = [];
    client.onLost((reason) => lost.push(reason));
    const grant = await client.acquire({ pid: 7, cwd: "/repo" });
    assert.deepEqual(grant, {
      generation: 4,
      sessionDir: "/srv/sessions",
      lineageFile: LINEAGE,
      sequence: 12,
      owner: "foreground",
    });
    // Asking again is the same lease, not a second request.
    assert.deepEqual(await client.acquire({ pid: 7, cwd: "/repo" }), grant);
    assert.equal(client.held?.generation, 4);

    // A page and a state read share the connection the lease is held on.
    const page = await client.conversation(12);
    assert.deepEqual(
      page.entries.map((entry) => entry.text),
      ["Hello."],
    );
    assert.equal(page.more, true);
    const state: ServiceState = await client.state();
    assert.equal(state.holder, "terminal");
    assert.equal(state.sessionDir, "/srv/sessions");
    assert.equal(state.lineageFile, LINEAGE);

    await client.release();
    assert.equal(client.held, undefined);
    assert.deepEqual(control.requests, [
      "control.hello",
      "control.lease_acquire",
      "control.conversation",
      "control.state",
      "control.lease_release",
    ]);
    // A release is not a loss.
    await new Promise((resolve) => setTimeout(resolve, 20));
    assert.deepEqual(lost, []);
  } finally {
    await control.close();
  }
});

test("a refused lease rejects with the host's code and holds nothing", async () => {
  const control = await fakeControl("refuse");
  try {
    const client = new LeaseClient(control.socketPath);
    await assert.rejects(
      client.acquire({ pid: 7, cwd: "/repo" }),
      (error: unknown) =>
        error instanceof LeaseRefused &&
        error.code === REFUSAL.LEASE_DISABLED &&
        error.message === "Not here.",
    );
    assert.equal(client.held, undefined);
    // The failed connection is closed rather than left half-open.
    await new Promise((resolve) => setTimeout(resolve, 20));
    assert.ok(control.sockets.every((socket) => socket.destroyed));
  } finally {
    await control.close();
  }
});

test("a connection that ends while held is reported once as lost", async () => {
  const control = await fakeControl();
  try {
    const client = new LeaseClient(control.socketPath);
    const lost: string[] = [];
    client.onLost((reason) => lost.push(reason));
    await client.acquire({ pid: 7, cwd: "/repo" });
    for (const socket of control.sockets) socket.destroy();
    await new Promise((resolve) => setTimeout(resolve, 50));
    assert.deepEqual(lost, ["closed"]);
    assert.equal(client.held, undefined);
    // A release after the loss has nothing to say and does not throw.
    await client.release();
  } finally {
    await control.close();
  }
});

test("control and agent messages are bounded on both directions", () => {
  assert.equal(
    encodeControlRequest({
      v: 11,
      type: "control.lease_acquire",
      id: "a",
      holder: { pid: 9, cwd: "/repo" },
    }),
    '{"v":11,"type":"control.lease_acquire","id":"a","holder":{"pid":9,"cwd":"/repo"}}\n',
  );
  assert.throws(() =>
    encodeControlRequest({
      v: 11,
      type: "control.lease_acquire",
      id: "a",
      holder: { pid: 0, cwd: "/repo" },
    }),
  );
  assert.throws(() =>
    encodeControlRequest({
      v: 11,
      type: "control.lease_acquire",
      id: "a",
      holder: { pid: 9, cwd: "repo" },
    }),
  );
  assert.throws(() =>
    encodeControlRequest({ v: 11, type: "control.lease_release", id: "a b" }),
  );
  assert.deepEqual(
    decodeControlResponse(
      '{"v":11,"type":"control.lease","id":"a","generation":2,"session_dir":"/s","sequence":0,"owner":"foreground"}',
    ),
    {
      v: 11,
      type: "control.lease",
      id: "a",
      generation: 2,
      session_dir: "/s",
      sequence: 0,
      owner: "foreground",
    },
  );
  assert.throws(() =>
    decodeControlResponse(
      '{"v":11,"type":"control.lease","id":"a","generation":0,"session_dir":"/s","sequence":0,"owner":"foreground"}',
    ),
  );
  assert.throws(() => decodeControlResponse('{"v":10,"type":"control.ready"}'));
  assert.throws(() =>
    decodeControlResponse('{"v":11,"type":"control.nonsense"}'),
  );

  assert.equal(
    encodeAgentRequest({ v: 11, type: "agent.hello", lease: 3 }),
    '{"v":11,"type":"agent.hello","lease":3}\n',
  );
  assert.equal(
    encodeAgentRequest({ v: 11, type: "agent.hello" }),
    '{"v":11,"type":"agent.hello"}\n',
  );
  assert.throws(() =>
    encodeAgentRequest({ v: 11, type: "agent.hello", lease: 0 }),
  );
  assert.throws(() =>
    encodeAgentRequest({ v: 11, type: "agent.turn", id: "t 1", text: "x" }),
  );
  // A paste past the bound is cut, not refused: the turn already happened.
  const long = recordedText(`${"é".repeat(MAX_TEXT_BYTES)}\r\0`);
  assert.ok(Buffer.byteLength(long, "utf8") <= MAX_TEXT_BYTES);
  assert.doesNotMatch(long, /[\r\0�]/);
  assert.equal(recordedText("plain\r\n"), "plain \n");
});

interface FakePi {
  pi: ExtensionAPI;
  /** Fires every handler registered under the name, like Pi does. */
  fire: (
    name: string,
    event?: unknown,
    context?: unknown,
  ) => Promise<unknown[]>;
  /** Runs the registered `/scufris` command. */
  command: (args: string, context?: unknown) => Promise<void>;
  registered: () => number;
  sent: Array<{ message: Record<string, unknown>; options?: unknown }>;
  dispatched: string[];
  emitted: Array<{ name: string; value: unknown }>;
}

function fakePi(): FakePi {
  const handlers = new Map<
    string,
    Array<(event?: unknown, context?: unknown) => unknown>
  >();
  const commands = new Map<
    string,
    (args: string, context: unknown) => Promise<void>
  >();
  const sent: FakePi["sent"] = [];
  const dispatched: string[] = [];
  const emitted: FakePi["emitted"] = [];
  const listeners = new Map<string, Array<(value: unknown) => void>>();
  const pi = {
    registerTool() {},
    registerCommand(
      name: string,
      definition: {
        handler: (args: string, context: unknown) => Promise<void>;
      },
    ) {
      commands.set(name, definition.handler);
    },
    on(name: string, handler: (event?: unknown, context?: unknown) => unknown) {
      handlers.set(name, [...(handlers.get(name) ?? []), handler]);
    },
    events: {
      on(name: string, listener: (value: unknown) => void) {
        listeners.set(name, [...(listeners.get(name) ?? []), listener]);
      },
      emit(name: string, value: unknown) {
        emitted.push({ name, value });
        for (const listener of listeners.get(name) ?? []) listener(value);
      },
    },
    sendMessage(message: Record<string, unknown>, options?: unknown) {
      sent.push({ message, options });
    },
    sendUserMessage(text: string) {
      dispatched.push(text);
    },
  } as unknown as ExtensionAPI;
  return {
    pi,
    fire: async (name, event, context) => {
      const results: unknown[] = [];
      for (const handler of handlers.get(name) ?? [])
        results.push(await handler(event, context));
      return results;
    },
    command: async (args, context) => {
      const handler = commands.get("scufris");
      assert.ok(handler, "no /scufris command was registered");
      await handler(args, context ?? commandContext());
    },
    registered: () => handlers.size,
    sent,
    dispatched,
    emitted,
  };
}

function fakeClient(calls: string[], options?: AgentClientOptions) {
  return {
    // A fake that answers its own hello at once, unless no options were
    // passed: then the channel never comes up, as with a dead service.
    start: () => {
      calls.push(`start:${JSON.stringify(options?.hello?.() ?? {})}`);
      options?.connected?.();
    },
    stop: () => calls.push("stop"),
    jobs: () => calls.push("jobs"),
    response: (
      response: AtomicResponse,
      proactiveId?: string,
      turnId?: string,
    ) => calls.push(`response:${response.text}:${turnId ?? "none"}`),
    proactiveStarted: () => calls.push("started"),
    proactiveSettled: () => calls.push("settled"),
    turn: (id: string, text: string, images = 0) =>
      calls.push(`turn:${id}:${text}:${images}`),
    activity: (working: boolean) => calls.push(`activity:${working}`),
    session: (session: { file: string }) =>
      calls.push(`session:${session.file}`),
  };
}

function fakeLease(
  outcome: "grant" | "refuse",
  calls: string[],
  lineage: string | undefined = LINEAGE,
): LeaseSource & { lose: (reason: string) => void } {
  let lostHandler: ((reason: string) => void) | undefined;
  const grant: LeaseGrant = {
    generation: 7,
    sessionDir: "/srv/sessions",
    sequence: 3,
    owner: "foreground",
    ...(lineage === undefined ? {} : { lineageFile: lineage }),
  };
  return {
    acquire: async () => {
      calls.push("acquire");
      if (outcome === "refuse") throw new LeaseRefused("lease_held", "Taken.");
      return grant;
    },
    release: async () => {
      calls.push("release");
    },
    onLost: (handler) => {
      lostHandler = handler;
    },
    state: async () => ({
      state: "idle",
      detail: "",
      holder: "terminal",
      generation: 7,
      sessionDir: "/srv/sessions",
      lineageFile: LINEAGE,
    }),
    lose: (reason) => lostHandler?.(reason),
  };
}

function sessionManager(file = SESSION, parent?: string) {
  return {
    getSessionId: () => "s1",
    getSessionFile: () => file,
    getSessionDir: () => "/srv/sessions",
    getCwd: () => "/repo",
    getHeader: () => (parent === undefined ? {} : { parentSession: parent }),
    getBranch: () => [],
  };
}

function extensionContext(file = SESSION, parent?: string): ExtensionContext {
  return {
    hasUI: false,
    isIdle: () => true,
    abort() {},
    sessionManager: sessionManager(file, parent),
  } as unknown as ExtensionContext;
}

function commandContext(
  file = SESSION,
  parent?: string,
  switched: string[] = [],
): ExtensionCommandContext {
  return {
    ...extensionContext(file, parent),
    switchSession: async (path: string) => {
      switched.push(path);
      return { cancelled: false };
    },
  } as unknown as ExtensionCommandContext;
}

/** A terminal wired to fakes, with the retry loop made fast. */
function terminal(
  options: {
    outcome?: "grant" | "refuse";
    lineage?: string | undefined;
    attachOnStart?: boolean;
    forkOnStart?: boolean;
    forked?: string;
    retryMinMs?: number;
  } = {},
) {
  const harness = fakePi();
  const calls: string[] = [];
  const trace: string[] = [];
  const forkedFrom: string[] = [];
  const lease = fakeLease(
    options.outcome ?? "grant",
    calls,
    "lineage" in options ? options.lineage : LINEAGE,
  );
  const control = bindTerminal(harness.pi, {
    role: "orchestrator",
    socketPath: "/tmp/scufris-test-agent.sock",
    lease,
    retryMinMs: options.retryMinMs ?? 5,
    retryMaxMs: 20,
    attachOnStart: options.attachOnStart !== false,
    forkOnStart: options.forkOnStart !== false,
    forkSession: async (from) => {
      forkedFrom.push(from);
      return options.forked;
    },
    trace: (event, fields) =>
      trace.push(fields ? `${event}:${JSON.stringify(fields)}` : event),
    createClient: (clientOptions) => fakeClient(calls, clientOptions),
  }) as TerminalControl;
  return { ...harness, control, calls, trace, lease, forkedFrom };
}

test("a typed turn is recorded and its answer names it", async () => {
  const t = terminal({ lineage: undefined });
  await t.fire("session_start", { reason: "startup" }, extensionContext());
  assert.equal(t.control.state(), "leased");
  // The hello is fenced by the generation and declares the session, so the
  // host knows both who may write and what to fork from next.
  assert.deepEqual(t.calls, [
    "acquire",
    `start:{"lease":7,"session":{"id":"s1","file":"${SESSION}","cwd":"/repo"}}`,
    "jobs",
  ]);
  // The owner token moved with the agent.
  assert.deepEqual(
    t.emitted.filter((event) => event.name === OWNER_EVENT).map((e) => e.value),
    [{ owner: "foreground" } satisfies OwnerSignal],
  );

  await t.fire("input", { text: "Typed here.", source: "interactive" });
  assert.match(t.calls.at(-1)!, /^turn:terminal-\d+-1:Typed here\.:0$/);
});

test("an accepted turn is the one the answer names", async () => {
  const harness = fakePi();
  const calls: string[] = [];
  let clientOptions: AgentClientOptions | undefined;
  const lease = fakeLease("grant", calls, undefined);
  bindTerminal(harness.pi, {
    role: "orchestrator",
    socketPath: "/tmp/scufris-test-agent.sock",
    lease,
    forkOnStart: false,
    createClient: (options) => {
      clientOptions = options;
      return fakeClient(calls, options);
    },
  });
  await harness.fire(
    "session_start",
    { reason: "startup" },
    extensionContext(),
  );
  await harness.fire("input", { text: "Question?", source: "interactive" });
  const id = calls.at(-1)!.split(":")[1]!;

  // Unacknowledged, the answer names nothing: the host recorded no such turn.
  harness.pi.events.emit("scufris:agent-response", { text: "Before." });
  assert.equal(calls.at(-1), "response:Before.:none");

  clientOptions?.turnAck?.(id, 41);
  harness.pi.events.emit("scufris:agent-response", { text: "After." });
  assert.equal(calls.at(-1), `response:After.:${id}`);

  // The turn ends with the agent, and so does the identifier.
  await harness.fire("agent_settled");
  harness.pi.events.emit("scufris:agent-response", { text: "Later." });
  assert.equal(calls.at(-1), "response:Later.:none");
});

test("a turn the host fences off moves this Pi out of the way", async () => {
  const harness = fakePi();
  const calls: string[] = [];
  let clientOptions: AgentClientOptions | undefined;
  const control = bindTerminal(harness.pi, {
    role: "orchestrator",
    socketPath: "/tmp/scufris-test-agent.sock",
    lease: fakeLease("grant", calls, undefined),
    forkOnStart: false,
    retryMinMs: 5_000,
    createClient: (options) => {
      clientOptions = options;
      return fakeClient(calls, options);
    },
  }) as TerminalControl;
  await harness.fire(
    "session_start",
    { reason: "startup" },
    extensionContext(),
  );
  assert.equal(control.state(), "leased");
  clientOptions?.refused?.("terminal-1-1", REFUSAL.NOT_LEASE_HOLDER, "Gone.");
  // The words stay in this Pi, which goes on answering the person in front
  // of it. What stops is writing to a conversation it no longer holds.
  assert.equal(control.state(), "lost");
  assert.equal(calls.at(-1), "stop");
  const owners = harness.emitted
    .filter((event) => event.name === OWNER_EVENT)
    .map((event) => (event.value as OwnerSignal).owner);
  assert.deepEqual(owners, ["foreground", "s1"]);
  control.hold();
});

test("a lost lease is retried with a backoff, and hold stops it", async () => {
  const t = terminal({ lineage: undefined, retryMinMs: 5 });
  await t.fire("session_start", { reason: "startup" }, extensionContext());
  t.lease.lose("closed");
  assert.equal(t.control.state(), "lost");
  await new Promise((resolve) => setTimeout(resolve, 40));
  // It came back on its own.
  assert.equal(t.control.state(), "leased");
  assert.ok(t.trace.some((line) => line.startsWith("reacquire_scheduled:")));

  t.control.hold();
  t.lease.lose("closed");
  t.control.hold();
  const before = t.calls.length;
  await new Promise((resolve) => setTimeout(resolve, 40));
  assert.equal(t.calls.length, before, "hold stops the loop");
  assert.equal(t.control.state(), "independent");
});

test("release gives the agent back and says who owns the jobs now", async () => {
  const t = terminal({ lineage: undefined });
  await t.fire("session_start", { reason: "startup" }, extensionContext());
  t.emitted.length = 0;
  await t.command("release");
  assert.equal(t.control.state(), "independent");
  assert.equal(t.calls.at(-1), "release");
  assert.deepEqual(
    t.emitted.filter((event) => event.name === OWNER_EVENT).map((e) => e.value),
    [{ owner: "s1" } satisfies OwnerSignal],
  );
});

test("a leased terminal keeps the lineage a single chain", async () => {
  const t = terminal({ lineage: undefined });
  await t.fire("session_start", { reason: "startup" }, extensionContext());
  assert.deepEqual(await t.fire("session_before_switch", { reason: "new" }), [
    { cancel: true },
  ]);
  assert.deepEqual(await t.fire("session_before_fork", {}), [{ cancel: true }]);

  // Independent, this is an ordinary Pi and switches freely.
  await t.command("release");
  assert.deepEqual(
    await t.fire("session_before_switch", { reason: "resume" }),
    [{}],
  );
  assert.deepEqual(await t.fire("session_before_fork", {}), [{}]);
});

test("attach forks the lineage once, and never asks twice", async () => {
  const forked = "/srv/sessions/forked.jsonl";
  const t = terminal({ forked });
  // Gate G3: a switch starts a session, which would ask for another switch.
  // The guard is that the ask happens only off the lineage.
  await t.fire("session_start", { reason: "startup" }, extensionContext());
  assert.deepEqual(t.dispatched, ["/scufris attach"]);

  const switched: string[] = [];
  await t.command("attach", commandContext(SESSION, undefined, switched));
  assert.deepEqual(t.forkedFrom, [LINEAGE]);
  assert.deepEqual(switched, [forked]);

  // The replacement session is a fork of the lineage, so the start that
  // follows the switch asks for nothing.
  t.dispatched.length = 0;
  await t.fire(
    "session_start",
    { reason: "resume" },
    extensionContext(forked, LINEAGE),
  );
  assert.deepEqual(t.dispatched, []);
  // Nor does a terminal the launcher already forked.
  await t.command("attach", commandContext(forked, LINEAGE, switched));
  assert.deepEqual(switched, [forked]);
});

test("a refused attach leaves an ordinary Pi with no channel", async () => {
  const t = terminal({ outcome: "refuse" });
  await t.fire("session_start", { reason: "startup" }, extensionContext());
  assert.equal(t.control.state(), "independent");
  assert.deepEqual(t.calls, ["acquire"]);
  assert.ok(t.trace.some((line) => line.includes("lease_held")));
  await t.fire("input", { text: "Typed.", source: "interactive" });
  assert.deepEqual(t.calls, ["acquire"]);
  await t.fire("session_shutdown", { reason: "quit" });
  assert.deepEqual(t.calls, ["acquire"], "there is nothing to give back");
});

test("what Pi calls interactive is what the conversation records", async () => {
  const t = terminal({ lineage: undefined });
  await t.fire("session_start", { reason: "startup" }, extensionContext());
  const before = t.calls.length;
  // Gate G4 measured what reaches this event at all: built-in commands,
  // extension commands such as `/scufris`, and `!` shell lines never do.
  // A `/skill:` line does, raw, and is the person's words.
  await t.fire("input", { text: "/skill:den status", source: "interactive" });
  await t.fire("input", {
    text: "<scufris_surface_message/>",
    source: "extension",
  });
  await t.fire("input", { text: "From the phone.", source: "rpc" });
  await t.fire("input", {
    text: "Steered.",
    source: "interactive",
    streamingBehavior: "steer",
    images: [{}, {}],
  });
  assert.deepEqual(
    t.calls.slice(before).map((call) => call.replace(/terminal-\d+-/, "")),
    ["turn:1:/skill:den status:0", "turn:2:Steered.:2"],
  );
  assert.ok(t.trace.some((line) => line.includes('"streaming":"steer"')));
});

test("a joining agent is caught up in one hidden message", async () => {
  const harness = fakePi();
  const calls: string[] = [];
  let clientOptions: AgentClientOptions | undefined;
  bindTerminal(harness.pi, {
    role: "orchestrator",
    socketPath: "/tmp/scufris-test-agent.sock",
    lease: fakeLease("grant", calls, undefined),
    forkOnStart: false,
    createClient: (options) => {
      clientOptions = options;
      return fakeClient(calls, options);
    },
  });
  await harness.fire(
    "session_start",
    { reason: "startup" },
    extensionContext(),
  );
  clientOptions?.catchUp?.(0, []);
  assert.equal(harness.sent.length, 0, "an empty page is not worth a message");

  clientOptions?.catchUp?.(9, [
    { sequence: 10, role: "user", surface: "ios", text: "Where is it?" },
    { sequence: 11, role: "assistant", surface: "ios", text: "In the drawer." },
  ]);
  assert.equal(harness.sent.length, 1);
  const injected = harness.sent[0]!;
  assert.equal(injected.message.customType, CATCH_UP_TYPE);
  assert.equal(injected.message.display, false);
  assert.deepEqual(injected.options, {
    deliverAs: "nextTurn",
    triggerTurn: false,
  });
  assert.match(String(injected.message.content), /user \(ios\): Where is it\?/);
  assert.match(String(injected.message.content), /you: In the drawer\./);
});

test("the words of a catch-up page say who said them", () => {
  assert.equal(catchUpContent([]).split("\n").length, 3);
  const content = catchUpContent([
    { sequence: 1, role: "user", surface: "desk", text: "One." },
  ]);
  assert.match(content, /Nobody is waiting for an answer to it\./);
  assert.match(content, /user \(desk\): One\./);
});

test("a handoff ends the lease without releasing it again", async () => {
  const harness = fakePi();
  const calls: string[] = [];
  let clientOptions: AgentClientOptions | undefined;
  const control = bindTerminal(harness.pi, {
    role: "orchestrator",
    socketPath: "/tmp/scufris-test-agent.sock",
    lease: fakeLease("grant", calls, undefined),
    forkOnStart: false,
    retryMinMs: 5_000,
    createClient: (options) => {
      clientOptions = options;
      return fakeClient(calls, options);
    },
  }) as TerminalControl;
  await harness.fire(
    "session_start",
    { reason: "startup" },
    extensionContext(),
  );
  clientOptions?.handoff?.(7, "managed");
  await harness.fire("session_shutdown", { reason: "quit" });
  assert.ok(
    !calls.includes("release"),
    "a handoff already gave the agent back",
  );
  control.hold();

  // Without a handoff, the shutdown is what gives it back.
  const plain = terminal({ lineage: undefined });
  await plain.fire("session_start", { reason: "startup" }, extensionContext());
  await plain.fire("session_shutdown", { reason: "quit" });
  assert.equal(plain.calls.at(-1), "release");
});

test("a leased terminal reports its own activity", async () => {
  const t = terminal({ lineage: undefined });
  await t.fire("session_start", { reason: "startup" }, extensionContext());
  const before = t.calls.length;
  await t.fire("agent_start");
  await t.fire("agent_settled");
  assert.deepEqual(t.calls.slice(before), ["activity:true", "activity:false"]);
});

test("status says what this terminal is and what the host thinks", async () => {
  const t = terminal({ lineage: undefined });
  await t.fire("session_start", { reason: "startup" }, extensionContext());
  const status = await t.control.status();
  assert.match(status, /state: leased/);
  assert.match(status, /channel: up/);
  assert.match(status, /generation: 7/);
  assert.match(status, /owner: foreground/);
  assert.match(status, /service: idle \(terminal\)/);
});

test("the project extension is off unless asked for, and never for a worker", async () => {
  assert.equal(terminalEnabled({}), false);
  assert.equal(terminalEnabled({ SCUFRIS_TERMINAL: "true" }), false);
  assert.equal(terminalEnabled({ SCUFRIS_TERMINAL: "1" }), true);
  assert.equal(
    terminalEnabled({ SCUFRIS_TERMINAL: "1", SCUFRIS_ROLE: "orchestrator" }),
    true,
  );
  assert.equal(
    terminalEnabled({ SCUFRIS_TERMINAL: "1", SCUFRIS_ROLE: "worker" }),
    false,
  );
  const previous = process.env.SCUFRIS_TERMINAL;
  delete process.env.SCUFRIS_TERMINAL;
  try {
    const { pi, registered } = fakePi();
    scufrisTerminal(pi);
    assert.equal(registered(), 0);
  } finally {
    if (previous !== undefined) process.env.SCUFRIS_TERMINAL = previous;
  }

  const directory = await mkdtemp(join(tmpdir(), "scufris-terminal-log-"));
  try {
    const log = join(directory, "terminal.jsonl");
    const trace = fileTrace(log);
    trace("input", { source: "interactive" });
    trace("agent_settled");
    const lines = readFileSync(log, "utf8").trim().split("\n");
    assert.equal(lines.length, 2);
    const first = JSON.parse(lines[0]!) as Record<string, unknown>;
    assert.equal(first.event, "input");
    assert.equal(first.source, "interactive");
    assert.equal(typeof first.at, "string");
    fileTrace(undefined)("nothing");
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("the agent channel is not joined without a lease", async () => {
  const harness = fakePi();
  const calls: string[] = [];
  bindService(harness.pi, {
    role: "orchestrator",
    socketPath: "/tmp/scufris-test-agent.sock",
    createClient: (options) => fakeClient(calls, options),
  });
  await harness.fire(
    "session_start",
    { reason: "startup" },
    extensionContext(),
  );
  // The managed child declares its session too: it is the lineage a terminal
  // forks from next.
  assert.deepEqual(calls, [
    `start:{"session":{"id":"s1","file":"${SESSION}","cwd":"/repo"}}`,
    "jobs",
  ]);
});
