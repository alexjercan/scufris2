import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { closeSync, openSync } from "node:fs";
import { connect, type Socket } from "node:net";
import {
  mkdir,
  mkdtemp,
  readFile,
  readdir,
  rm,
  stat,
  symlink,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import {
  AssistantStateTracker,
  type AttentionStateSignal,
} from "../extensions/scufris/shared/assistant-state.ts";
import {
  MAX_MESSAGE_BYTES,
  MAX_SUBMISSION_TEXT_BYTES,
  MAX_WIDGET_DATA_BYTES,
  ProtocolError,
  decodeClientMessage,
  encodeDaemonMessage,
  takeLines,
} from "../extensions/scufris/desktop/protocol.ts";
import {
  ControlServer,
  MAX_REMEMBERED_SUBMISSIONS,
  OwnershipLock,
  SocketBusyError,
  SubmissionUncertainError,
  WidgetCommandError,
  canonicalSocketPath,
  ownershipLockFile,
  submissionDigest,
  type AcceptedSubmission,
  type ControlHost,
  type ControlServerOptions,
  type WidgetNotice,
} from "../extensions/scufris/desktop/server.ts";
import {
  ACCEPTED_ENTRY,
  DISPATCH_ENTRY,
  LEGACY_RECEIPT_ENTRY,
  SessionAcceptance,
  SessionClosedError,
  acceptedSubmissions,
  entryId,
  landings,
  submissionState,
  transcriptCommit,
  userMessageDigest,
  type SessionView,
  type TranscriptDispatch,
} from "../extensions/scufris/desktop/acceptance.ts";
import desktop, {
  TRANSCRIPT_LABEL,
  commitRenderer,
  resolveSocketPath,
  sessionIdentity,
} from "../extensions/scufris/desktop/index.ts";

/** A host whose delivery resolves immediately, for tests about other things. */
function simpleHost(
  deliver: ControlHost["deliver"],
  seeded: AcceptedSubmission[] = [],
): ControlHost {
  return {
    session: () => "popup-session",
    deliver,
    accepted: () => seeded,
  };
}

/**
 * A Pi session that behaves like the real one: `send` only announces the
 * prompt, and the words become an entry when the session delivers them. The
 * receipt is written the instant that happens, directly in front of them, as
 * the extension's `message_end` handler does.
 */
class FakeSession implements SessionView {
  readonly entries: unknown[] = [];
  readonly queued: Array<{ id: string; text: string; digest: string }> = [];
  /** When set, the session refuses to record a dispatch, as a closed one would. */
  refuseSend?: string;
  /** Extension handlers registered for `message_end`. */
  readonly onMessageEnd: Array<(submission?: TranscriptDispatch) => void> = [];
  private nextEntry = 0;

  branch(): readonly unknown[] {
    return this.entries;
  }

  leaf(): string | undefined {
    return entryId(this.entries.at(-1));
  }

  dispatch(id: string, digest: string): void {
    if (this.refuseSend) throw new Error(this.refuseSend);
    this.append({
      type: "custom",
      customType: DISPATCH_ENTRY,
      data: { version: 1, id, digest },
    });
  }

  send(id: string, text: string, digest: string): void {
    this.queued.push({ id, text, digest });
  }

  commit(submission: TranscriptDispatch, entry: string): void {
    this.accept(submission.id, submission.digest, entry);
  }

  /** Someone typing into the popup, which becomes an ordinary user message. */
  type(text: string): string {
    this.announce();
    return this.prompt(text);
  }

  /** Another extension writing its own entry that looks like this daemon's. */
  foreign(text: string): void {
    this.append({
      type: "custom",
      customType: "some-other-extension-v1",
      data: { version: 1, id: "pill-1", digest: submissionDigest(text) },
    });
    this.announce();
    this.prompt(text);
  }

  /**
   * Delivers one queued prompt the way Pi does: extensions see `message_end`
   * first, the message is appended, and only then can anything name it
   * (agent-session.js:363-379).
   */
  land(id: string): void {
    this.prompt(this.deliver(id).text);
  }

  /**
   * Delivers one queued prompt whose append never happens, which is what a
   * session that cannot record the message leaves behind: the event was seen
   * and the entry is not there.
   */
  lose(id: string): void {
    this.deliver(id);
  }

  /** Takes a branch at an earlier entry, dropping the newest `count`. */
  rewind(count: number): void {
    this.entries.length = Math.max(0, this.entries.length - count);
  }

  /** Appends one landed submission with no event at all. */
  seed(id: string, text: string, digest = submissionDigest(text)): void {
    this.dispatch(id, digest);
    this.accept(id, digest, this.prompt(text));
  }

  /** Appends one user prompt, returning its entry identifier. */
  prompt(text: string): string {
    return this.append({
      type: "message",
      message: { role: "user", content: [{ type: "text", text }] },
    });
  }

  /** Appends one acceptance commit naming the prompt it accepted. */
  accept(id: string, digest: string, entry: string): void {
    this.append({
      type: "custom",
      customType: ACCEPTED_ENTRY,
      data: { version: 1, id, digest, entry },
    });
  }

  private deliver(id: string): { id: string; text: string; digest: string } {
    const index = this.queued.findIndex((message) => message.id === id);
    if (index === -1) throw new Error(`nothing queued for ${id}`);
    const message = this.queued.splice(index, 1)[0]!;
    this.announce({ version: 1, id: message.id, digest: message.digest });
    return message;
  }

  /**
   * Registers the extension's own `message_end` wiring for one acceptance.
   *
   * One session has one live daemon, so this replaces whatever served it
   * before, exactly as a restarted daemon replaces its predecessor.
   */
  serve(acceptance: SessionAcceptance): void {
    this.onMessageEnd.length = 0;
    this.onMessageEnd.push((submission) => {
      acceptance.landed(submission);
      setImmediate(() => acceptance.notify());
    });
  }

  /** Tells the extension one prompt is landing, before the entry exists. */
  private announce(submission?: TranscriptDispatch): void {
    for (const handler of this.onMessageEnd) handler(submission);
  }

  private append(entry: Record<string, unknown>): string {
    this.nextEntry += 1;
    const id = `entry-${this.nextEntry}`;
    this.entries.push({ ...entry, id });
    return id;
  }

  /** Returns the submissions this session accepted. */
  transcripts(): string[] {
    return [...landings(this.entries).keys()];
  }
}

interface Companion {
  socket: Socket;
  next(): Promise<Record<string, unknown>>;
  /** Everything received and not yet taken, without taking any of it. */
  peek(): Array<Record<string, unknown>>;
  send(line: string): void;
  closed(): Promise<void>;
}

function companion(socketPath: string): Promise<Companion> {
  return new Promise((resolve, reject) => {
    const socket = connect(socketPath);
    const lines: string[] = [];
    const waiting: Array<(line: string) => void> = [];
    let buffer = "";
    let ended = false;
    const endings: Array<() => void> = [];
    socket.setEncoding("utf8");
    socket.on("data", (chunk: string) => {
      buffer += chunk;
      for (;;) {
        const index = buffer.indexOf("\n");
        if (index === -1) break;
        const line = buffer.slice(0, index);
        buffer = buffer.slice(index + 1);
        const pending = waiting.shift();
        if (pending) pending(line);
        else lines.push(line);
      }
    });
    const finish = () => {
      ended = true;
      for (const resolveEnd of endings.splice(0)) resolveEnd();
    };
    socket.on("close", finish);
    socket.on("error", finish);
    socket.once("connect", () =>
      resolve({
        socket,
        next: () =>
          new Promise((resolveLine, rejectLine) => {
            const buffered = lines.shift();
            if (buffered !== undefined) {
              resolveLine(JSON.parse(buffered));
              return;
            }
            const timer = setTimeout(
              () => rejectLine(new Error("no daemon message arrived")),
              5_000,
            );
            waiting.push((line) => {
              clearTimeout(timer);
              resolveLine(JSON.parse(line));
            });
          }),
        peek: () => lines.map((line) => JSON.parse(line)),
        send: (line: string) => socket.write(line),
        closed: () =>
          ended
            ? Promise.resolve()
            : new Promise<void>((resolveEnd) => endings.push(resolveEnd)),
      }),
    );
    socket.once("error", reject);
  });
}

async function withServer(
  host: Partial<ControlHost>,
  body: (socketPath: string, server: ControlServer) => Promise<void>,
  options: ControlServerOptions = {},
): Promise<void> {
  const directory = await mkdtemp(join(tmpdir(), "scufris-desktop-"));
  const socketPath = join(directory, "scufris", "daemon.sock");
  const server = new ControlServer(
    socketPath,
    {
      session: host.session ?? (() => "popup-session"),
      deliver: host.deliver ?? (async () => {}),
      accepted: host.accepted ?? (() => []),
    },
    () => {},
    options,
  );
  try {
    await server.start();
    await body(socketPath, server);
  } finally {
    await server.stop();
    await rm(directory, { recursive: true, force: true });
  }
}

function acceptanceHarness(timeoutMs = 2_000): {
  session: FakeSession;
  acceptance: SessionAcceptance;
} {
  const session = new FakeSession();
  const acceptance = new SessionAcceptance(session, timeoutMs, 10);
  session.serve(acceptance);
  return { session, acceptance };
}

/** Waits for the deferred acceptance work of the current tick to finish. */
function settled(ms = 40): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/**
 * Keeps the event loop alive while a delivery waits for its landing timeout.
 *
 * A pending delivery never holds the process open by itself; in the daemon the
 * socket does, and here this does.
 */
function awake(): () => void {
  const timer = setInterval(() => {}, 20);
  return () => clearInterval(timer);
}

test("hello answers with the authoritative session and the current state", async () => {
  await withServer({ session: () => "popup-1" }, async (socketPath, server) => {
    server.broadcast({ state: "working", detail: "indexing" });
    const client = await companion(socketPath);
    client.send('{"v":2,"type":"hello"}\n');
    assert.deepEqual(await client.next(), {
      v: 2,
      type: "welcome",
      session: "popup-1",
    });
    assert.deepEqual(await client.next(), {
      v: 2,
      type: "state",
      state: "working",
      detail: "indexing",
    });
    client.socket.destroy();
  });
});

test("ping answers with pong", async () => {
  await withServer({}, async (socketPath) => {
    const client = await companion(socketPath);
    client.send('{"v":2,"type":"ping"}\n');
    assert.deepEqual(await client.next(), { v: 2, type: "pong" });
    client.socket.destroy();
  });
});

test("an accepted transcript enters the conversation and is acknowledged", async () => {
  const submitted: Array<[string, string]> = [];
  await withServer(
    {
      deliver: async (id, text) => {
        submitted.push([id, text]);
      },
    },
    async (socketPath) => {
      const client = await companion(socketPath);
      client.send(
        '{"v":2,"type":"submit","id":"pill-1","text":"open tasks"}\n',
      );
      assert.deepEqual(await client.next(), {
        v: 2,
        type: "ack",
        id: "pill-1",
      });
      assert.deepEqual(submitted, [["pill-1", "open tasks"]]);
      client.socket.destroy();
    },
  );
});

test("a transcript delivered before a lost acknowledgment is not delivered twice", async () => {
  const submitted: Array<[string, string]> = [];
  await withServer(
    {
      deliver: async (id, text) => {
        submitted.push([id, text]);
      },
    },
    async (socketPath) => {
      // The daemon accepts the text, then the connection drops before the
      // companion sees the acknowledgment.
      const first = await companion(socketPath);
      first.send('{"v":2,"type":"submit","id":"pill-1","text":"open tasks"}\n');
      assert.deepEqual(await first.next(), { v: 2, type: "ack", id: "pill-1" });
      first.socket.destroy();
      await first.closed();

      // The companion reconnects and retries under the same identifier.
      const second = await companion(socketPath);
      second.send(
        '{"v":2,"type":"submit","id":"pill-1","text":"open tasks"}\n',
      );
      assert.deepEqual(await second.next(), {
        v: 2,
        type: "ack",
        id: "pill-1",
      });
      assert.deepEqual(
        submitted,
        [["pill-1", "open tasks"]],
        "the retry entered the conversation a second time",
      );

      // A genuinely new identifier is still delivered.
      second.send('{"v":2,"type":"submit","id":"pill-2","text":"and again"}\n');
      assert.deepEqual(await second.next(), {
        v: 2,
        type: "ack",
        id: "pill-2",
      });
      assert.equal(submitted.length, 2);
      second.socket.destroy();
    },
  );
});

test("a submission that was never delivered is retried rather than suppressed", async () => {
  let refuse = true;
  const submitted: string[] = [];
  await withServer(
    {
      deliver: async (_id, text) => {
        if (refuse) throw new Error("the session is not ready");
        submitted.push(text);
      },
    },
    async (socketPath) => {
      const client = await companion(socketPath);
      client.send(
        '{"v":2,"type":"submit","id":"pill-1","text":"open tasks"}\n',
      );
      assert.deepEqual(await client.next(), {
        v: 2,
        type: "uncertain",
        id: "pill-1",
        detail: "the session is not ready",
      });

      refuse = false;
      client.send(
        '{"v":2,"type":"submit","id":"pill-1","text":"open tasks"}\n',
      );
      let answered = await client.next();
      while (answered.type !== "ack") answered = await client.next();
      assert.deepEqual(answered, { v: 2, type: "ack", id: "pill-1" });
      assert.deepEqual(submitted, ["open tasks"]);
      client.socket.destroy();
    },
  );
});

test("the remembered submission set stays bounded", async () => {
  const submitted: string[] = [];
  await withServer(
    {
      deliver: async (id) => {
        submitted.push(id);
      },
    },
    async (socketPath) => {
      const client = await companion(socketPath);
      const total = MAX_REMEMBERED_SUBMISSIONS + 1;
      for (let index = 0; index < total; index += 1) {
        client.send(
          `{"v":2,"type":"submit","id":"pill-${index}","text":"x"}\n`,
        );
        await client.next();
      }
      assert.equal(submitted.length, total);

      // The oldest identifier has been evicted, so it is delivered again. That
      // is the deliberate cost of a bounded set, and it only bites after
      // hundreds of newer submissions.
      client.send('{"v":2,"type":"submit","id":"pill-0","text":"x"}\n');
      await client.next();
      assert.equal(submitted.length, total + 1);

      // The newest identifier is still suppressed.
      client.send(
        `{"v":2,"type":"submit","id":"pill-${total - 1}","text":"x"}\n`,
      );
      await client.next();
      assert.equal(submitted.length, total + 1);
      client.socket.destroy();
    },
  );
});

test("an undelivered submission reports an error state instead of an acknowledgment", async () => {
  await withServer(
    {
      deliver: async () => {
        throw new Error("the session is not ready");
      },
    },
    async (socketPath) => {
      const client = await companion(socketPath);
      client.send(
        '{"v":2,"type":"submit","id":"pill-1","text":"open tasks"}\n',
      );
      // A failure this daemon cannot classify may have left words behind, so
      // the peer is told the honest thing and the tray shows the fault.
      assert.deepEqual(await client.next(), {
        v: 2,
        type: "uncertain",
        id: "pill-1",
        detail: "the session is not ready",
      });
      assert.deepEqual(await client.next(), {
        v: 2,
        type: "state",
        state: "error",
        detail: "the session is not ready",
      });
      client.socket.destroy();
    },
  );
});

test("unknown message types and other protocol versions close the connection", async () => {
  await withServer({}, async (socketPath) => {
    const unknown = await companion(socketPath);
    unknown.send('{"v":2,"type":"mirror"}\n');
    await unknown.closed();

    // A version 1 companion is refused at hello rather than half understood.
    const versioned = await companion(socketPath);
    versioned.send('{"v":1,"type":"hello"}\n');
    await versioned.closed();

    const malformed = await companion(socketPath);
    malformed.send('{"v":2,"type":"submit","id":"a b","text":"x"}\n');
    await malformed.closed();
  });
});

test("an oversized line closes the connection before it is parsed", async () => {
  await withServer({}, async (socketPath) => {
    const client = await companion(socketPath);
    client.send(`${"x".repeat(MAX_MESSAGE_BYTES + 1)}\n`);
    await client.closed();
  });
});

test("state broadcasts reach every connected companion", async () => {
  await withServer({}, async (socketPath, server) => {
    const first = await companion(socketPath);
    const second = await companion(socketPath);
    server.broadcast({ state: "attention", detail: "job 1 is blocked" });
    const expected = {
      v: 2,
      type: "state",
      state: "attention",
      detail: "job 1 is blocked",
    };
    assert.deepEqual(await first.next(), expected);
    assert.deepEqual(await second.next(), expected);
    first.socket.destroy();
    second.socket.destroy();
  });
});

test("the socket is private to its owner and removed on shutdown", async () => {
  const directory = await mkdtemp(join(tmpdir(), "scufris-desktop-"));
  const socketPath = join(directory, "scufris", "daemon.sock");
  const host: ControlHost = simpleHost(async () => {});
  const server = new ControlServer(socketPath, host, () => {});
  await server.start();
  const socketStat = await stat(socketPath);
  assert.equal(socketStat.mode & 0o777, 0o600);
  const directoryStat = await stat(join(directory, "scufris"));
  assert.equal(directoryStat.mode & 0o777, 0o700);

  const busy = new ControlServer(socketPath, host, () => {});
  await assert.rejects(() => busy.start(), SocketBusyError);

  await server.stop();
  await assert.rejects(() => stat(socketPath));

  const replacement = new ControlServer(socketPath, host, () => {});
  await replacement.start();
  await replacement.stop();
  await rm(directory, { recursive: true, force: true });
});

test("the protocol codec bounds every field it accepts", () => {
  assert.deepEqual(decodeClientMessage('{"v":2,"type":"hello"}'), {
    v: 2,
    type: "hello",
  });
  assert.throws(
    () => decodeClientMessage('{"v":2,"type":"hello"}\r'),
    ProtocolError,
  );
  assert.throws(() => decodeClientMessage("not json"), ProtocolError);
  assert.throws(() => decodeClientMessage("[]"), ProtocolError);
  assert.throws(
    () =>
      decodeClientMessage(
        JSON.stringify({
          v: 2,
          type: "submit",
          id: "pill-1",
          text: "x".repeat(MAX_SUBMISSION_TEXT_BYTES + 1),
        }),
      ),
    ProtocolError,
  );
  assert.equal(
    encodeDaemonMessage({ v: 2, type: "pong" }),
    '{"v":2,"type":"pong"}\n',
  );
  assert.throws(
    () =>
      encodeDaemonMessage({
        v: 2,
        type: "welcome",
        session: "x".repeat(MAX_MESSAGE_BYTES),
      }),
    ProtocolError,
  );
  assert.deepEqual(takeLines('{"a":1}\n{"b":2}\n{"c'), {
    lines: ['{"a":1}', '{"b":2}'],
    rest: '{"c',
  });
  assert.throws(() => takeLines("x".repeat(MAX_MESSAGE_BYTES)), ProtocolError);
});

test("the widget commands carry a correlation id and a bounded payload", () => {
  assert.equal(
    encodeDaemonMessage({
      v: 2,
      type: "widget_open",
      id: "w-1",
      widget: "note",
      posture: "exhibit",
      data: { text: "the harness is green" },
    }),
    '{"v":2,"type":"widget_open","id":"w-1","widget":"note",' +
      '"posture":"exhibit","data":{"text":"the harness is green"}}\n',
  );
  // The payload cap is smaller than the message cap on purpose: the same
  // bytes cross the companion's per-window channel afterwards.
  assert.throws(
    () =>
      encodeDaemonMessage({
        v: 2,
        type: "widget_update",
        id: "w-2",
        surface: "widget-3",
        data: { text: "x".repeat(MAX_WIDGET_DATA_BYTES) },
      }),
    ProtocolError,
  );

  assert.deepEqual(
    decodeClientMessage(
      '{"v":2,"type":"widget_opened","id":"w-1","surface":"widget-3"}',
    ),
    { v: 2, type: "widget_opened", id: "w-1", surface: "widget-3" },
  );
  assert.deepEqual(
    decodeClientMessage(
      '{"v":2,"type":"catalog","widgets":[{"id":"note","name":"Note","description":"A short note."}]}',
    ),
    {
      v: 2,
      type: "catalog",
      widgets: [{ id: "note", name: "Note", description: "A short note." }],
    },
  );
  // An answer nobody can match is an answer that can act on the wrong surface.
  assert.throws(
    () =>
      decodeClientMessage(
        '{"v":2,"type":"widget_opened","id":"w 1","surface":"widget-3"}',
      ),
    ProtocolError,
  );
  assert.throws(
    () =>
      decodeClientMessage(
        '{"v":2,"type":"widget_event","surface":"widget-3","event":"exploded"}',
      ),
    ProtocolError,
  );
});

test("a widget payload that is not text is refused here, not by the companion", () => {
  // The model writes this payload. An unpaired surrogate survives
  // JSON.stringify as an escape no strict decoder reads back, and the decoder
  // at the far end of this socket rejects the connection rather than the
  // message: the companion drops the link, backs off, and tells the person the
  // backend is unavailable. It has to fail as a tool result instead.
  const lone = "half a character: \ud800";
  assert.throws(
    () =>
      encodeDaemonMessage({
        v: 2,
        type: "widget_open",
        id: "w-1",
        widget: "note",
        posture: "exhibit",
        data: { text: lone },
      }),
    (error: unknown) =>
      error instanceof ProtocolError && error.code === "not_well_formed",
  );
  // A key is as much of the payload as a value is.
  assert.throws(
    () =>
      encodeDaemonMessage({
        v: 2,
        type: "widget_update",
        id: "w-2",
        surface: "widget-3",
        data: { ["\udfff"]: "whole" },
      }),
    ProtocolError,
  );
  // A pair is a character, and characters outside the basic plane are text.
  assert.equal(
    encodeDaemonMessage({
      v: 2,
      type: "widget_update",
      id: "w-3",
      surface: "widget-3",
      data: { text: "\u{1f600} and é" },
    }),
    '{"v":2,"type":"widget_update","id":"w-3","surface":"widget-3",' +
      '"data":{"text":"\u{1f600} and é"}}\n',
  );
  // And an identifier this daemon cannot write is refused before the round
  // trip that would have refused it anyway.
  assert.throws(
    () =>
      encodeDaemonMessage({
        v: 2,
        type: "widget_open",
        id: "w-4",
        widget: "no such widget",
        posture: "exhibit",
        data: {},
      }),
    ProtocolError,
  );
});

test("a widget command is settled by the answer that carries its id", async () => {
  await withServer({}, async (socketPath, server) => {
    const client = await companion(socketPath);
    const opened = server.request({
      type: "widget_open",
      widget: "note",
      posture: "exhibit",
      data: { text: "hi" },
    });
    const command = (await client.next()) as { id: string };
    assert.deepEqual(command, {
      v: 2,
      type: "widget_open",
      id: command.id,
      widget: "note",
      posture: "exhibit",
      data: { text: "hi" },
    });
    client.send(
      `${JSON.stringify({ v: 2, type: "widget_opened", id: command.id, surface: "widget-3" })}\n`,
    );
    assert.deepEqual(await opened, { surface: "widget-3" });

    // An update names no new surface, so it is answered by widget_done.
    const updated = server.request({
      type: "widget_update",
      surface: "widget-3",
      data: { text: "there" },
    });
    const second = (await client.next()) as { id: string };
    assert.notEqual(second.id, command.id, "correlation ids were reused");
    client.send(
      `${JSON.stringify({ v: 2, type: "widget_done", id: second.id })}\n`,
    );
    assert.deepEqual(await updated, {});

    const refused = server.request({ type: "widget_close", surface: "gone" });
    const third = (await client.next()) as { id: string };
    client.send(
      `${JSON.stringify({
        v: 2,
        type: "widget_failed",
        id: third.id,
        code: "surface_not_found",
        detail: "no surface named gone",
      })}\n`,
    );
    const error = await refused.then(
      () => undefined,
      (reason: unknown) => reason,
    );
    assert.ok(error instanceof WidgetCommandError);
    assert.equal(error.code, "surface_not_found");
    assert.equal(error.message, "no surface named gone");
    client.socket.destroy();
  });
});

test("each widget command waits for its own answer, whatever order they arrive in", async () => {
  await withServer({}, async (socketPath, server) => {
    const client = await companion(socketPath);
    const first = server.request({ type: "widget_close", surface: "widget-1" });
    const second = server.request({ type: "widget_clear" });
    const one = (await client.next()) as { id: string };
    const two = (await client.next()) as { id: string };

    // Answered back to front: a command settles on the identifier it carries
    // and never on whichever answer happens to arrive first.
    client.send(
      `${JSON.stringify({ v: 2, type: "widget_done", id: two.id })}\n`,
    );
    assert.deepEqual(await second, {});
    client.send(
      `${JSON.stringify({
        v: 2,
        type: "widget_failed",
        id: one.id,
        code: "surface_not_found",
        detail: "",
      })}\n`,
    );
    await assert.rejects(first, WidgetCommandError);
    client.socket.destroy();
  });
});

test("a widget command nothing can answer fails instead of hanging the turn", async () => {
  await withServer(
    {},
    async (socketPath, server) => {
      // Nothing is connected to ask.
      const unreachable = await server.request({ type: "widget_clear" }).then(
        () => undefined,
        (reason: unknown) => reason,
      );
      assert.ok(unreachable instanceof WidgetCommandError);
      assert.equal(unreachable.code, "companion_unavailable");

      // A companion that leaves takes its commands with it, rather than
      // leaving the caller to wait out a timeout that can teach it nothing.
      const leaving = await companion(socketPath);
      const abandoned = server.request({ type: "widget_clear" });
      await leaving.next();
      leaving.socket.destroy();
      const gone = await abandoned.then(
        () => undefined,
        (reason: unknown) => reason,
      );
      assert.ok(gone instanceof WidgetCommandError);
      assert.equal(gone.code, "companion_unavailable");

      // A companion that stays but never answers is the timeout case.
      const silent = await companion(socketPath);
      const unanswered = server.request({ type: "widget_clear" });
      await silent.next();
      const timed = await unanswered.then(
        () => undefined,
        (reason: unknown) => reason,
      );
      assert.ok(timed instanceof WidgetCommandError);
      assert.equal(timed.code, "timeout");
      silent.socket.destroy();
    },
    { widgetTimeoutMs: 60 },
  );
});

test("a widget that opens after its command was given up on is closed again", async () => {
  await withServer(
    {},
    async (socketPath, server) => {
      const client = await companion(socketPath);
      const abandoned = server.request({
        type: "widget_open",
        widget: "note",
        posture: "exhibit",
        data: {},
      });
      const asked = await client.next();
      assert.equal(asked.type, "widget_open");
      const timed = await abandoned.then(
        () => undefined,
        (reason: unknown) => reason,
      );
      assert.ok(timed instanceof WidgetCommandError);
      assert.equal(timed.code, "timeout");

      // The panel arrives anyway. Nothing holds its surface identifier now, so
      // nobody but the person could ever put it away and Scufris cannot even
      // name it. Closing it is the only honest end to a tool call that has
      // already failed.
      client.send(
        '{"v":2,"type":"widget_opened","id":"w-1","surface":"widget-3"}\n',
      );
      const closing = await client.next();
      assert.equal(closing.type, "widget_close");
      assert.equal(closing.surface, "widget-3");
      client.socket.destroy();
    },
    { widgetTimeoutMs: 60 },
  );
});

test("surface events and the catalog reach the widget listener, not a caller", async () => {
  await withServer({}, async (socketPath, server) => {
    const seen: WidgetNotice[] = [];
    server.watchWidgets((notice) => seen.push(notice));
    const client = await companion(socketPath);
    client.send(
      '{"v":2,"type":"catalog","widgets":[{"id":"note","name":"Note","description":"A short note."}]}\n',
    );
    client.send(
      '{"v":2,"type":"widget_event","surface":"widget-3","event":"closed"}\n',
    );
    // An answer to a command nobody sent is dropped rather than acted on.
    client.send('{"v":2,"type":"widget_done","id":"w-99"}\n');
    client.send('{"v":2,"type":"ping"}\n');
    assert.deepEqual(await client.next(), { v: 2, type: "pong" });
    assert.deepEqual(seen, [
      {
        type: "catalog",
        widgets: [{ id: "note", name: "Note", description: "A short note." }],
      },
      { type: "widget_event", surface: "widget-3", event: "closed" },
    ]);
    client.socket.destroy();
  });
});

test("a version 1 peer is refused instead of half understood", () => {
  assert.throws(
    () => decodeClientMessage('{"v":1,"type":"hello"}'),
    ProtocolError,
  );
  assert.throws(
    () =>
      decodeClientMessage('{"v":1,"type":"submit","id":"pill-1","text":"x"}'),
    ProtocolError,
  );
});

test("assistant state prefers the active run, then speech, then unattended work", () => {
  const tracker = new AssistantStateTracker();
  assert.deepEqual(tracker.report(), { state: "idle", detail: "" });

  const blocked: AttentionStateSignal = {
    state: "attention",
    detail: "Job 1 is blocked:  needs\n a decision ",
  };
  tracker.setUnattended(blocked);
  assert.deepEqual(tracker.report(), {
    state: "attention",
    detail: "Job 1 is blocked: needs a decision",
  });

  tracker.setSpeaking(true);
  assert.equal(tracker.report().state, "speaking");
  tracker.setRunning(true);
  assert.deepEqual(tracker.report(), { state: "working", detail: "" });

  tracker.setRunning(false);
  tracker.setSpeaking(false);
  assert.deepEqual(tracker.report(), { state: "idle", detail: "" });

  tracker.setUnattended({ state: "error", detail: "Job 1 failed" });
  assert.deepEqual(tracker.report(), {
    state: "error",
    detail: "Job 1 failed",
  });
  tracker.setUnattended({ state: "clear", detail: "" });
  assert.equal(tracker.report().state, "idle");

  tracker.setUnattended({ state: "error", detail: "y".repeat(400) });
  assert.equal(tracker.report().detail.length, 200);
  tracker.reset();
  assert.deepEqual(tracker.report(), { state: "idle", detail: "" });
});

test("the socket path follows the runtime directory and the session stays bounded", () => {
  assert.equal(
    resolveSocketPath({
      XDG_RUNTIME_DIR: "/run/user/1000",
    } as NodeJS.ProcessEnv),
    "/run/user/1000/scufris/daemon.sock",
  );
  assert.equal(
    resolveSocketPath({
      XDG_RUNTIME_DIR: "/run/user/1000",
      SCUFRIS_DESKTOP_SOCKET: "/tmp/other.sock",
    } as NodeJS.ProcessEnv),
    "/tmp/other.sock",
  );
  assert.equal(resolveSocketPath({} as NodeJS.ProcessEnv), undefined);
  assert.equal(sessionIdentity(undefined), "ephemeral");
  assert.equal(
    sessionIdentity(
      "/home/alex/.local/share/scufris-popup/sessions/2026-08-24.jsonl",
    ),
    "2026-08-24",
  );
  assert.equal(
    sessionIdentity(`/sessions/${"n".repeat(300)}.jsonl`).length,
    128,
  );
});

test("both protocol implementations agree on the same wire fixtures", async () => {
  // The companion implements this protocol separately in Rust. Both suites read
  // these exact lines so the two implementations cannot drift.
  const fixtures = JSON.parse(
    await readFile(
      new URL("../desktop/control-protocol-v2.json", import.meta.url),
      "utf8",
    ),
  );

  for (const line of fixtures.canonical.companion) {
    assert.deepEqual(decodeClientMessage(line), JSON.parse(line), line);
  }
  for (const line of fixtures.canonical.daemon) {
    assert.equal(encodeDaemonMessage(JSON.parse(line)), `${line}\n`, line);
  }
  for (const line of fixtures.tolerated.companion) {
    assert.doesNotThrow(() => decodeClientMessage(line), line);
  }
  for (const line of fixtures.rejected.companion) {
    assert.throws(() => decodeClientMessage(line), ProtocolError, line);
  }
});

test("concurrent retries of one identifier share a single delivery", async () => {
  const started: string[] = [];
  let release: (() => void) | undefined;
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  await withServer(
    {
      deliver: async (id) => {
        started.push(id);
        await gate;
      },
    },
    async (socketPath) => {
      // Both lines arrive before the first delivery resolves, which is exactly
      // the window where an unsynchronised check would deliver twice.
      const client = await companion(socketPath);
      client.send(
        '{"v":2,"type":"submit","id":"pill-1","text":"open tasks"}\n' +
          '{"v":2,"type":"submit","id":"pill-1","text":"open tasks"}\n',
      );
      const second = await companion(socketPath);
      second.send(
        '{"v":2,"type":"submit","id":"pill-1","text":"open tasks"}\n',
      );

      await new Promise((resolve) => setTimeout(resolve, 50));
      assert.deepEqual(
        started,
        ["pill-1"],
        "the transcript was delivered twice",
      );

      release?.();
      assert.deepEqual(await client.next(), {
        v: 2,
        type: "ack",
        id: "pill-1",
      });
      assert.deepEqual(await client.next(), {
        v: 2,
        type: "ack",
        id: "pill-1",
      });
      assert.deepEqual(await second.next(), {
        v: 2,
        type: "ack",
        id: "pill-1",
      });
      assert.deepEqual(started, ["pill-1"]);
      client.socket.destroy();
      second.socket.destroy();
    },
  );
});

test("a failed delivery leaves no reservation, so the next retry is delivered", async () => {
  let refuse = true;
  const started: string[] = [];
  await withServer(
    {
      deliver: async (id) => {
        started.push(id);
        if (refuse) throw new Error("the session is not ready");
      },
    },
    async (socketPath) => {
      const client = await companion(socketPath);
      client.send(
        '{"v":2,"type":"submit","id":"pill-1","text":"open tasks"}\n',
      );
      assert.equal((await client.next()).type, "uncertain");

      refuse = false;
      client.send(
        '{"v":2,"type":"submit","id":"pill-1","text":"open tasks"}\n',
      );
      let answered = await client.next();
      while (answered.type !== "ack") answered = await client.next();
      assert.deepEqual(answered, { v: 2, type: "ack", id: "pill-1" });
      assert.deepEqual(started, ["pill-1", "pill-1"]);
      client.socket.destroy();
    },
  );
});

test("a reused identifier carrying different text is refused, not acknowledged", async () => {
  const delivered: string[] = [];
  await withServer(
    {
      deliver: async (_id, text) => {
        delivered.push(text);
      },
    },
    async (socketPath) => {
      const client = await companion(socketPath);
      client.send('{"v":2,"type":"submit","id":"pill-1","text":"first"}\n');
      assert.deepEqual(await client.next(), {
        v: 2,
        type: "ack",
        id: "pill-1",
      });

      // Acknowledging this would tell the pill that "second" landed, when only
      // "first" is in the conversation.
      client.send('{"v":2,"type":"submit","id":"pill-1","text":"second"}\n');
      const answer = await client.next();
      // Answered to the peer that asked, naming the submission it refuses:
      // those words never left the daemon, so they are still the pill's.
      assert.equal(answer.type, "refused", JSON.stringify(answer));
      assert.equal(answer.id, "pill-1");
      assert.match(
        String(answer.detail),
        /already accepted with different text/,
      );
      assert.deepEqual(delivered, ["first"]);
      client.socket.destroy();
    },
  );
});

test("idempotency survives a daemon restart that resumes the session", async () => {
  const directory = await mkdtemp(join(tmpdir(), "scufris-desktop-"));
  const socketPath = join(directory, "scufris", "daemon.sock");
  // One session outlives both daemons, exactly as the session file does.
  const { session, acceptance } = acceptanceHarness();
  const host = (): ControlHost => ({
    session: () => "popup-1",
    deliver: (id, text, digest) => acceptance.deliver(id, text, digest),
    accepted: () => acceptance.accepted(MAX_REMEMBERED_SUBMISSIONS),
  });

  const first = new ControlServer(socketPath, host(), () => {});
  await first.start();
  const before = await companion(socketPath);
  before.send('{"v":2,"type":"submit","id":"pill-1","text":"open tasks"}\n');
  await new Promise((resolve) => setTimeout(resolve, 20));
  session.land("pill-1");
  assert.deepEqual(await before.next(), { v: 2, type: "ack", id: "pill-1" });
  // The acknowledgment is lost: the daemon dies before the companion sees it.
  before.socket.destroy();
  await first.stop();

  const second = new ControlServer(socketPath, host(), () => {});
  await second.start();
  const after = await companion(socketPath);
  after.send('{"v":2,"type":"submit","id":"pill-1","text":"open tasks"}\n');
  assert.deepEqual(await after.next(), { v: 2, type: "ack", id: "pill-1" });
  assert.equal(
    session.queued.length,
    0,
    "the restarted daemon queued the retry a second time",
  );
  assert.deepEqual(
    session.transcripts(),
    ["pill-1"],
    "the retry entered the conversation a second time",
  );

  after.socket.destroy();
  await second.stop();
  await rm(directory, { recursive: true, force: true });
});

test("only one of two daemons racing to start owns the socket", async () => {
  const directory = await mkdtemp(join(tmpdir(), "scufris-desktop-"));
  const socketPath = join(directory, "scufris", "daemon.sock");
  const build = () =>
    new ControlServer(
      socketPath,
      simpleHost(async () => {}),
      () => {},
    );
  const servers = [build(), build(), build()];

  const outcomes = await Promise.allSettled(
    servers.map((server) => server.start()),
  );
  const winners = outcomes.filter((outcome) => outcome.status === "fulfilled");
  assert.equal(winners.length, 1, "more than one daemon claimed the socket");
  for (const outcome of outcomes) {
    if (outcome.status === "rejected") {
      assert.ok(
        outcome.reason instanceof SocketBusyError,
        String(outcome.reason),
      );
    }
  }

  // The winner is still serving, and no loser removed its socket.
  const client = await companion(socketPath);
  client.send('{"v":2,"type":"ping"}\n');
  assert.deepEqual(await client.next(), { v: 2, type: "pong" });
  client.socket.destroy();

  for (const server of servers) await server.stop();
  await rm(directory, { recursive: true, force: true });
});

test("a stale socket left by a dead daemon is replaced", async () => {
  const directory = await mkdtemp(join(tmpdir(), "scufris-desktop-"));
  const socketPath = join(directory, "scufris", "daemon.sock");
  const first = new ControlServer(
    socketPath,
    simpleHost(async () => {}),
    () => {},
  );
  await first.start();
  // Close the listener without the cleanup a graceful stop would do.
  await new Promise<void>((resolve) =>
    (
      first as unknown as { server: { close(cb: () => void): void } }
    ).server.close(() => resolve()),
  );

  const second = new ControlServer(
    socketPath,
    simpleHost(async () => {}),
    () => {},
  );
  await second.start();
  const client = await companion(socketPath);
  client.send('{"v":2,"type":"ping"}\n');
  assert.deepEqual(await client.next(), { v: 2, type: "pong" });
  client.socket.destroy();

  await second.stop();
  await rm(directory, { recursive: true, force: true });
});

test("a failure after the listener exists leaves no unreachable server behind", async () => {
  const directory = await mkdtemp(join(tmpdir(), "scufris-desktop-"));
  const socketPath = join(directory, "scufris", "daemon.sock");
  const refused = new ControlServer(
    socketPath,
    simpleHost(async () => {}),
    () => {},
    {
      harden: () => {
        throw new Error("chmod refused");
      },
    },
  );
  await assert.rejects(() => refused.start(), /chmod refused/);
  assert.equal(refused.listening, false);

  // Nothing is listening and no path was claimed, so a healthy daemon starts.
  const healthy = new ControlServer(
    socketPath,
    simpleHost(async () => {}),
    () => {},
  );
  await healthy.start();
  const client = await companion(socketPath);
  client.send('{"v":2,"type":"ping"}\n');
  assert.deepEqual(await client.next(), { v: 2, type: "pong" });
  client.socket.destroy();

  await healthy.stop();
  // The lock file stays: it is the inode the kernel lock lives on, and a lock
  // nobody unlinks is a lock nobody can remove from under a successor.
  const leftovers = (await readdir(join(directory, "scufris"))).sort();
  assert.deepEqual(
    leftovers,
    ["daemon.sock.lock"],
    `leftover socket files: ${leftovers}`,
  );
  await rm(directory, { recursive: true, force: true });
});

test("concurrent different bodies under one identifier are not both acknowledged", async () => {
  let release: (() => void) | undefined;
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  const delivered: string[] = [];
  await withServer(
    {
      deliver: async (_id, text) => {
        delivered.push(text);
        await gate;
      },
    },
    async (socketPath) => {
      const first = await companion(socketPath);
      first.send('{"v":2,"type":"submit","id":"pill-1","text":"first"}\n');
      await new Promise((resolve) => setTimeout(resolve, 20));

      // A second client retries the same identifier with different words while
      // the first delivery is still running.
      const second = await companion(socketPath);
      second.send('{"v":2,"type":"submit","id":"pill-1","text":"second"}\n');
      const answer = await second.next();
      assert.equal(answer.type, "refused", JSON.stringify(answer));
      assert.equal(answer.id, "pill-1");
      assert.match(
        String(answer.detail),
        /already accepted with different text/,
      );

      release?.();
      // The refusal went to the peer that asked and to nobody else, so the
      // first client sees only the acknowledgment it is owed.
      let answered = await first.next();
      while (answered.type !== "ack") answered = await first.next();
      assert.deepEqual(answered, { v: 2, type: "ack", id: "pill-1" });
      assert.deepEqual(delivered, ["first"]);
      first.socket.destroy();
      second.socket.destroy();
    },
  );
});

test("a stale socket is replaced by exactly one of several racing daemons", async () => {
  const directory = await mkdtemp(join(tmpdir(), "scufris-desktop-"));
  const socketPath = join(directory, "scufris", "daemon.sock");

  // Leave a stale socket behind, which is the interleaving a bare atomic link
  // does not cover: every starter sees the same dead inode.
  const dead = new ControlServer(
    socketPath,
    simpleHost(async () => {}),
    () => {},
  );
  await dead.start();
  await new Promise<void>((resolve) =>
    (
      dead as unknown as { server: { close(cb: () => void): void } }
    ).server.close(() => resolve()),
  );

  const servers = [0, 1, 2].map(
    () =>
      new ControlServer(
        socketPath,
        simpleHost(async () => {}),
        () => {},
      ),
  );
  const outcomes = await Promise.allSettled(
    servers.map((server) => server.start()),
  );
  const winners = outcomes.filter((outcome) => outcome.status === "fulfilled");
  assert.equal(
    winners.length,
    1,
    "more than one daemon replaced the stale socket",
  );

  const client = await companion(socketPath);
  client.send('{"v":2,"type":"ping"}\n');
  assert.deepEqual(await client.next(), { v: 2, type: "pong" });
  client.socket.destroy();

  for (const server of servers) await server.stop();
  await rm(directory, { recursive: true, force: true });
});

test("a failure after the public socket is claimed gives the pathname back", async () => {
  const directory = await mkdtemp(join(tmpdir(), "scufris-desktop-"));
  const socketPath = join(directory, "scufris", "daemon.sock");
  const refused = new ControlServer(
    socketPath,
    {
      session: () => "s",
      deliver: async () => {},
      accepted: () => {
        throw new Error("the session could not be read");
      },
    },
    () => {},
  );
  await assert.rejects(() => refused.start(), /the session could not be read/);
  assert.equal(refused.listening, false);

  // The claimed pathname must not survive as an unreachable stale socket. The
  // lock file is not a socket and is deliberately never removed.
  const leftovers = (await readdir(join(directory, "scufris"))).sort();
  assert.deepEqual(
    leftovers,
    ["daemon.sock.lock"],
    `leftover socket files: ${leftovers}`,
  );

  const healthy = new ControlServer(
    socketPath,
    simpleHost(async () => {}),
    () => {},
  );
  await healthy.start();
  const client = await companion(socketPath);
  client.send('{"v":2,"type":"ping"}\n');
  assert.deepEqual(await client.next(), { v: 2, type: "pong" });
  client.socket.destroy();
  await healthy.stop();
  await rm(directory, { recursive: true, force: true });
});

test("nothing is acknowledged until the session actually holds the transcript", async () => {
  const { session, acceptance } = acceptanceHarness();
  await withServer(
    {
      deliver: (id, text, digest) => acceptance.deliver(id, text, digest),
      accepted: () => acceptance.accepted(MAX_REMEMBERED_SUBMISSIONS),
    },
    async (socketPath) => {
      const client = await companion(socketPath);
      client.send(
        '{"v":2,"type":"submit","id":"pill-1","text":"open tasks"}\n',
      );
      await new Promise((resolve) => setTimeout(resolve, 40));

      // The agent is busy, so the message is only queued. Starting a send
      // proves nothing, and an acknowledgment here would be a lie.
      assert.equal(session.queued.length, 1);
      assert.deepEqual(session.transcripts(), []);

      session.land("pill-1");
      assert.deepEqual(await client.next(), {
        v: 2,
        type: "ack",
        id: "pill-1",
      });
      assert.deepEqual(session.transcripts(), ["pill-1"]);
      client.socket.destroy();
    },
  );
});

test("a send the session refuses leaves nothing uncertain behind", async () => {
  const { session, acceptance } = acceptanceHarness();
  session.refuseSend = "the session is not ready";
  await withServer(
    {
      deliver: (id, text, digest) => acceptance.deliver(id, text, digest),
      accepted: () => acceptance.accepted(MAX_REMEMBERED_SUBMISSIONS),
    },
    async (socketPath) => {
      const client = await companion(socketPath);
      client.send(
        '{"v":2,"type":"submit","id":"pill-1","text":"open tasks"}\n',
      );
      const answer = await client.next();
      // Nothing was dispatched and nothing was recorded, so this is refused
      // rather than uncertain: the pill may edit these words and retry them.
      assert.deepEqual(answer, {
        v: 2,
        type: "refused",
        id: "pill-1",
        detail: "submission pill-1 was not sent: the session is not ready",
      });

      // The retry succeeds once the send works, under the same identifier.
      session.refuseSend = undefined;
      client.send(
        '{"v":2,"type":"submit","id":"pill-1","text":"open tasks"}\n',
      );
      await new Promise((resolve) => setTimeout(resolve, 40));
      session.land("pill-1");
      assert.deepEqual(await client.next(), {
        v: 2,
        type: "ack",
        id: "pill-1",
      });
      assert.deepEqual(session.transcripts(), ["pill-1"]);
      client.socket.destroy();
    },
  );
});

test("neither the user nor another extension can acknowledge the pill", async () => {
  const { session, acceptance } = acceptanceHarness();
  await withServer(
    {
      deliver: (id, text, digest) => acceptance.deliver(id, text, digest),
      accepted: () => acceptance.accepted(MAX_REMEMBERED_SUBMISSIONS),
    },
    async (socketPath) => {
      const client = await companion(socketPath);
      client.send(
        '{"v":2,"type":"submit","id":"pill-1","text":"open tasks"}\n',
      );
      await new Promise((resolve) => setTimeout(resolve, 40));

      // The user types the very words the pill is waiting on, and another
      // extension sends them too, under an identifier that looks like ours.
      session.type("open tasks");
      session.foreign("open tasks");
      await new Promise((resolve) => setTimeout(resolve, 80));

      assert.deepEqual(
        session.transcripts(),
        [],
        "somebody else's message was taken for the pill's",
      );
      assert.equal(session.queued.length, 1);

      session.land("pill-1");
      assert.deepEqual(await client.next(), {
        v: 2,
        type: "ack",
        id: "pill-1",
      });
      assert.deepEqual(session.transcripts(), ["pill-1"]);
      client.socket.destroy();
    },
  );
});

test("a restart reconciles from the transcript the session holds", async () => {
  const { session, acceptance } = acceptanceHarness();
  // The daemon sent, the message landed, and the acknowledgment was lost.
  void acceptance
    .deliver("pill-1", "open tasks", submissionDigest("open tasks"))
    .catch(() => undefined);
  session.land("pill-1");
  await new Promise((resolve) => setTimeout(resolve, 40));

  const restarted = new SessionAcceptance(session, 200, 10);
  assert.deepEqual(restarted.accepted(MAX_REMEMBERED_SUBMISSIONS), [
    { id: "pill-1", digest: submissionDigest("open tasks") },
  ]);
  acceptance.reset();
});

test("a queued send lost to a restart is delivered again, not suppressed", async () => {
  const { session, acceptance } = acceptanceHarness();
  const abandoned = acceptance.deliver(
    "pill-1",
    "open tasks",
    submissionDigest("open tasks"),
  );
  assert.equal(session.queued.length, 1);
  acceptance.reset();
  await assert.rejects(() => abandoned, SessionClosedError);

  // The queue died with the process, so nothing was accepted and the retry
  // must go through.
  const restarted = new SessionAcceptance(session, 200, 10);
  assert.deepEqual(restarted.accepted(MAX_REMEMBERED_SUBMISSIONS), []);
});

test("a retry while the first send is still queued does not send twice", async () => {
  const { session, acceptance } = acceptanceHarness();
  await withServer(
    {
      deliver: (id, text, digest) => acceptance.deliver(id, text, digest),
      accepted: () => acceptance.accepted(MAX_REMEMBERED_SUBMISSIONS),
    },
    async (socketPath) => {
      const client = await companion(socketPath);
      client.send(
        '{"v":2,"type":"submit","id":"pill-1","text":"open tasks"}\n',
      );
      await new Promise((resolve) => setTimeout(resolve, 40));

      const second = await companion(socketPath);
      second.send(
        '{"v":2,"type":"submit","id":"pill-1","text":"open tasks"}\n',
      );
      await new Promise((resolve) => setTimeout(resolve, 40));
      assert.equal(
        session.queued.length,
        1,
        "the retry queued the words a second time",
      );

      session.land("pill-1");
      assert.deepEqual(await client.next(), {
        v: 2,
        type: "ack",
        id: "pill-1",
      });
      assert.deepEqual(session.transcripts(), ["pill-1"]);
      client.socket.destroy();
      second.socket.destroy();
    },
  );
});

test("a landing is observed even when no further Pi event follows", async () => {
  const { session, acceptance } = acceptanceHarness();
  await withServer(
    {
      deliver: (id, text, digest) => acceptance.deliver(id, text, digest),
      accepted: () => acceptance.accepted(MAX_REMEMBERED_SUBMISSIONS),
    },
    async (socketPath) => {
      const client = await companion(socketPath);
      client.send(
        '{"v":2,"type":"submit","id":"pill-1","text":"open tasks"}\n',
      );
      await new Promise((resolve) => setTimeout(resolve, 40));

      // Pi appends the entry with no message_end reaching this extension at
      // all, which is what a stalled provider looks like.
      session.queued.length = 0;
      session.seed("pill-1", "open tasks", submissionDigest("open tasks"));
      assert.deepEqual(await client.next(), {
        v: 2,
        type: "ack",
        id: "pill-1",
      });
      client.socket.destroy();
    },
  );
});

test("a commit names the entry this prompt became, not one that resembles it", async () => {
  const { session, acceptance } = acceptanceHarness();
  const text = "open tasks";
  const digest = submissionDigest(text);
  // The very same words are already in the conversation, from the person.
  const older = session.type(text);
  await settled();

  const delivery = acceptance.deliver("pill-1", text, digest);
  session.land("pill-1");
  await delivery;

  const committed = session.entries
    .map((entry) => transcriptCommit(entry))
    .find((commit) => commit !== undefined);
  const prompts = session.entries
    .filter((entry) => userMessageDigest(entry) === digest)
    .map((entry) => entryId(entry));
  assert.equal(prompts.length, 2, "both prompts must be in the conversation");
  assert.notEqual(
    committed?.entry,
    older,
    "the commit named an older prompt with the same words",
  );
  assert.equal(committed?.entry, prompts.at(-1));
  acceptance.reset();
});

test("an append that never happened commits nothing, whatever fills its place", async () => {
  const { session, acceptance } = acceptanceHarness(60);
  const sleeping = awake();
  const text = "book the flight";
  const delivery = acceptance.deliver("pill-1", text, submissionDigest(text));

  // Pi announced the prompt and then could not record it. The person's own
  // identical words land in the place it would have taken.
  session.lose("pill-1");
  session.type(text);

  await assert.rejects(() => delivery, SubmissionUncertainError);
  assert.deepEqual(
    session.transcripts(),
    [],
    "a prompt this submission never became was committed as the one it did",
  );
  sleeping();
});

test("an identical prompt landing beside a spoken one leaves it uncommitted", async () => {
  const { session, acceptance } = acceptanceHarness(60);
  const sleeping = awake();
  const text = "book the flight";
  const delivery = acceptance.deliver("pill-1", text, submissionDigest(text));

  // Both landed before anything could read the session back. Which entry is
  // whose is no longer provable, so neither is claimed.
  session.land("pill-1");
  session.type(text);

  await assert.rejects(() => delivery, SubmissionUncertainError);
  assert.deepEqual(session.transcripts(), []);
  assert.equal(
    session.entries.filter(
      (entry) => userMessageDigest(entry) === submissionDigest(text),
    ).length,
    2,
  );
  sleeping();
});

test("a branch taken while a prompt lands leaves it uncommitted", async () => {
  const { session, acceptance } = acceptanceHarness(60);
  const sleeping = awake();
  const text = "book the flight";
  const delivery = acceptance.deliver("pill-1", text, submissionDigest(text));

  // The event has been seen and the entry does not exist yet. The person takes
  // a branch at an earlier entry, and the same words land on that branch.
  session.lose("pill-1");
  session.rewind(1);
  session.prompt(text);

  await assert.rejects(() => delivery, SubmissionUncertainError);
  assert.deepEqual(session.transcripts(), []);
  sleeping();
});

test("a session replaced while a prompt lands cancels its commit", async () => {
  const { session, acceptance } = acceptanceHarness(60);
  const text = "open tasks";
  const delivery = acceptance.deliver("pill-1", text, submissionDigest(text));
  session.lose("pill-1");
  // The daemon's session ends between the event and the deferred work.
  acceptance.reset();
  await assert.rejects(() => delivery, SessionClosedError);

  // Whatever arrives afterwards belongs to the session that replaced it.
  session.prompt(text);
  await settled();
  assert.deepEqual(
    session.transcripts(),
    [],
    "a cancelled commit was written into the replacement session",
  );
});

test("closing the session settles pending deliveries instead of leaving them", async () => {
  const { session, acceptance } = acceptanceHarness();
  const delivery = acceptance.deliver(
    "pill-1",
    "open tasks",
    submissionDigest("open tasks"),
  );
  assert.equal(session.queued.length, 1);
  acceptance.reset();
  await assert.rejects(() => delivery, SessionClosedError);
});

test("a landing that never comes leaves the request uncertain, never resent", async () => {
  const { session, acceptance } = acceptanceHarness(60);
  const text = "book the flight";
  const digest = submissionDigest(text);
  // A pending delivery never holds the process open by itself; in the daemon
  // the socket does. Here the test does.
  const alive = setInterval(() => {}, 20);

  await assert.rejects(
    () => acceptance.deliver("pill-1", text, digest),
    SubmissionUncertainError,
  );
  assert.equal(session.queued.length, 1);

  // The timeout is not a decision. Retrying after it must not send the words
  // again, because they may already have entered the conversation and run.
  await assert.rejects(
    () => acceptance.deliver("pill-1", text, digest),
    SubmissionUncertainError,
  );
  assert.equal(session.queued.length, 1, "a timeout resent the request");

  // Neither is a restart: everything this process knew is gone, and the
  // session still says the words were dispatched.
  const restarted = new SessionAcceptance(session, 60, 10);
  session.serve(restarted);
  restarted.accepted(MAX_REMEMBERED_SUBMISSIONS);
  await assert.rejects(
    () => restarted.deliver("pill-1", text, digest),
    SubmissionUncertainError,
  );
  assert.equal(session.queued.length, 1, "a restart resent the request");

  // The person's own decision is the only thing that does.
  const forced = restarted.deliver("pill-1", text, digest, true);
  assert.equal(session.queued.length, 2);
  session.queued.shift();
  session.land("pill-1");
  restarted.notify();
  await forced;
  assert.deepEqual(session.transcripts(), ["pill-1"]);
  clearInterval(alive);
});

test("a conflicting retry is refused even after it left the daemon's cache", async () => {
  const { session, acceptance } = acceptanceHarness();
  // One old transcript, then enough newer ones to push it out of the bounded
  // set the daemon rebuilds when the socket opens.
  session.seed("pill-old", "read my mail");
  for (let index = 0; index < MAX_REMEMBERED_SUBMISSIONS; index += 1) {
    session.seed(`pill-${index}`, `later ${index}`);
  }
  const submit = (text: string) =>
    `${JSON.stringify({ v: 2, type: "submit", id: "pill-old", text })}\n`;

  await withServer(
    {
      deliver: (id, text, digest) => acceptance.deliver(id, text, digest),
      accepted: () => acceptance.accepted(MAX_REMEMBERED_SUBMISSIONS),
    },
    async (socketPath) => {
      const client = await companion(socketPath);
      client.send(submit("send my mail"));
      const refused = await client.next();
      assert.equal(refused.type, "refused", JSON.stringify(refused));
      assert.equal(refused.id, "pill-old");
      assert.match(String(refused.detail), /already accepted with different/);
      assert.equal(session.queued.length, 0, "the refused retry was sent");

      // The words the session does hold are still owed their acknowledgment,
      // and they must not be sent a second time to produce it.
      client.send(submit("read my mail"));
      let answer = await client.next();
      while (answer.type !== "ack") answer = await client.next();
      assert.deepEqual(answer, { v: 2, type: "ack", id: "pill-old" });
      assert.equal(session.queued.length, 0);
      client.socket.destroy();
    },
  );
});

test("every body a reused identifier landed is acknowledged and nothing else", async () => {
  const { session, acceptance } = acceptanceHarness();
  // One identifier over two different sentences: what a companion whose
  // identifiers collided leaves in a session.
  session.seed("pill-1", "read my mail");
  session.seed("pill-1", "send my mail");
  const submit = (text: string) =>
    `${JSON.stringify({ v: 2, type: "submit", id: "pill-1", text })}\n`;

  await withServer(
    {
      deliver: (id, text, digest) => acceptance.deliver(id, text, digest),
      accepted: () => acceptance.accepted(MAX_REMEMBERED_SUBMISSIONS),
    },
    async (socketPath) => {
      const client = await companion(socketPath);
      for (const text of ["read my mail", "send my mail"]) {
        client.send(submit(text));
        let answer = await client.next();
        while (answer.type !== "ack") answer = await client.next();
        assert.deepEqual(answer, { v: 2, type: "ack", id: "pill-1" }, text);
      }

      client.send(submit("delete my mail"));
      const refused = await client.next();
      assert.equal(refused.type, "refused", JSON.stringify(refused));
      assert.equal(refused.id, "pill-1");
      assert.equal(session.queued.length, 0, "a refused body was sent anyway");
      client.socket.destroy();
    },
  );
});

test("acceptance is read back from what was committed, never from what is beside it", () => {
  const text = "open tasks";
  const digest = submissionDigest(text);
  const prompt = (words: string, id: string) => ({
    type: "message",
    id,
    message: { role: "user", content: [{ type: "text", text: words }] },
  });
  const dispatch = (id: string, over = digest) => ({
    type: "custom",
    id: `d-${id}`,
    customType: DISPATCH_ENTRY,
    data: { version: 1, id, digest: over },
  });
  const commit = (id: string, entry: string, over = digest) => ({
    type: "custom",
    id: `c-${id}`,
    customType: ACCEPTED_ENTRY,
    data: { version: 1, id, digest: over, entry },
  });

  assert.deepEqual(
    [
      ...landings([
        dispatch("pill-1"),
        prompt(text, "e1"),
        commit("pill-1", "e1"),
      ]),
    ],
    [["pill-1", new Set([digest])]],
  );

  for (const branch of [
    // Dispatched and never committed: the words may be in the conversation
    // and may not be, which is not the same as being in it.
    [dispatch("pill-1"), prompt(text, "e1")],
    // A commit naming a prompt this branch does not hold. This is the shape a
    // branch taken at the prompt leaves, and the shape a crash between the two
    // appends leaves.
    [dispatch("pill-1"), commit("pill-1", "e1")],
    // A commit whose named prompt carries different words.
    [prompt("something else entirely", "e1"), commit("pill-1", "e1")],
    // A commit naming a stranger's prompt with the same words. Adjacency would
    // have accepted this; naming the entry does not.
    [prompt(text, "e9"), commit("pill-1", "e1")],
    // A prompt with nobody claiming it.
    [prompt(text, "e1")],
    // Another extension's entry that looks like this daemon's.
    [
      {
        type: "custom",
        id: "x1",
        customType: "some-other-extension-v1",
        data: { version: 1, id: "pill-1", digest, entry: "e1" },
      },
      prompt(text, "e1"),
    ],
    // Commits this daemon could not have written.
    [
      prompt(text, "e1"),
      { type: "custom", id: "c", customType: ACCEPTED_ENTRY },
    ],
    [
      prompt(text, "e1"),
      {
        type: "custom",
        id: "c",
        customType: ACCEPTED_ENTRY,
        data: { version: 2, id: "pill-1", digest, entry: "e1" },
      },
    ],
    [prompt(text, "e1"), commit("", "e1")],
    [prompt(text, "e1"), commit("pill-1", "")],
    [null],
  ]) {
    assert.deepEqual([...landings(branch)], [], JSON.stringify(branch));
  }

  // A commit that follows its prompt at any distance is still that prompt's.
  assert.deepEqual(
    [
      ...landings([
        prompt(text, "e1"),
        {
          type: "message",
          id: "e2",
          message: { role: "assistant", content: [] },
        },
        commit("pill-1", "e1"),
      ]),
    ],
    [["pill-1", new Set([digest])]],
  );

  const many = Array.from(
    { length: MAX_REMEMBERED_SUBMISSIONS + 5 },
    (_, index) => [
      prompt(text, `e${index}`),
      commit(`pill-${index}`, `e${index}`),
    ],
  ).flat();
  const bounded = acceptedSubmissions(many, MAX_REMEMBERED_SUBMISSIONS);
  assert.equal(bounded.length, MAX_REMEMBERED_SUBMISSIONS);
  assert.equal(bounded.at(-1)?.id, `pill-${MAX_REMEMBERED_SUBMISSIONS + 4}`);
});

test("a session says accepted, uncertain, or unsent, and never guesses", () => {
  const text = "open tasks";
  const digest = submissionDigest(text);
  const other = submissionDigest("something else");
  const prompt = {
    type: "message",
    id: "e1",
    message: { role: "user", content: [{ type: "text", text }] },
  };
  const dispatch = {
    type: "custom",
    id: "d1",
    customType: DISPATCH_ENTRY,
    data: { version: 1, id: "pill-1", digest },
  };
  const commit = {
    type: "custom",
    id: "c1",
    customType: ACCEPTED_ENTRY,
    data: { version: 1, id: "pill-1", digest, entry: "e1" },
  };
  // What an earlier build wrote before its prompt landed. It cannot prove
  // acceptance, so it is read as a dispatch and nothing more.
  const legacy = {
    type: "custom",
    id: "l1",
    customType: LEGACY_RECEIPT_ENTRY,
    data: { version: 1, id: "pill-1", digest },
  };

  assert.equal(submissionState([], "pill-1", digest), "unsent");
  assert.equal(submissionState([dispatch], "pill-1", digest), "uncertain");
  assert.equal(
    submissionState([legacy, prompt], "pill-1", digest),
    "uncertain",
  );
  assert.equal(
    submissionState([dispatch, prompt, commit], "pill-1", digest),
    "accepted",
  );
  assert.equal(
    submissionState([dispatch, prompt, commit], "pill-1", other),
    "conflict",
  );
  assert.equal(submissionState([dispatch], "pill-1", other), "conflict");
});

/**
 * The parts of Pi the desktop extension reaches, in Pi's order.
 *
 * `sendUserMessage` runs the whole preflight - the ordered input handler chain,
 * the pre-send compaction check, and the per-turn system prompt from
 * `before_agent_start` - and only then delivers the message
 * (`agent-session.js:792-919`). A custom message that triggers a turn runs none
 * of it (`agent-session.js:1068-1090`). Input handlers run in extension order
 * and any of them may rewrite a prompt or answer it outright
 * (`extensions/runner.js:930-965`), and while a turn is running a prompt is
 * queued instead of delivered.
 *
 * The runtime calls `sendUserMessage` directly rather than scheduling it
 * (`agent-session.js:1854-1861`), which is what puts Pi's announcement of a
 * prompt inside the caller's asynchronous context.
 */
class FakePi {
  readonly entries: unknown[] = [];
  readonly handlers = new Map<
    string,
    Array<(event: any, ctx: any) => unknown>
  >();
  readonly renderers = new Map<string, unknown>();
  /** The effective system prompt of every turn that has started. */
  readonly turns: string[] = [];
  /** How many times the pre-send compaction check ran. */
  compactionChecks = 0;
  private nextEntry = 0;
  /** True while a turn is running, so prompts are steered rather than run. */
  streaming = false;
  private readonly steering: string[] = [];
  readonly events = { on: () => {}, emit: () => {} };
  readonly context = {
    hasUI: false,
    sessionManager: {
      getBranch: () => this.entries,
      // What the next append becomes a child of, which is what Pi advances on
      // every append and what identifies a prompt that is still landing.
      getLeafId: () => entryId(this.entries.at(-1)) ?? null,
      getSessionFile: () => "/sessions/popup-1.jsonl",
    },
  };

  on(event: string, handler: (event: any, ctx: any) => unknown): void {
    const existing = this.handlers.get(event) ?? [];
    existing.push(handler);
    this.handlers.set(event, existing);
  }

  registerEntryRenderer(customType: string, renderer: unknown): void {
    this.renderers.set(customType, renderer);
  }

  appendEntry(customType: string, data: unknown): void {
    this.append({ type: "custom", customType, data });
  }

  /** Appends one entry with the identifier Pi would give it. */
  private append(entry: Record<string, unknown>): string {
    this.nextEntry += 1;
    const id = `entry-${this.nextEntry}`;
    this.entries.push({ ...entry, id });
    return id;
  }

  sendMessage(): void {
    throw new Error(
      "a desktop transcript must not bypass the prompt preflight",
    );
  }

  /** What an extension - this one or any other - calls to send a prompt. */
  sendUserMessage(text: string): void {
    void this.prompt(text, "extension");
  }

  /** Someone typing into the popup. */
  typed(text: string): Promise<void> {
    return this.prompt(text, "interactive");
  }

  /** A prompt arriving over Pi's RPC interface. */
  overRpc(text: string): Promise<void> {
    return this.prompt(text, "rpc");
  }

  /** Pi reaching a steering boundary and delivering what is queued. */
  async drain(): Promise<void> {
    this.streaming = false;
    for (const text of this.steering.splice(0)) await this.deliver(text);
  }

  async emit(event: string, payload: unknown): Promise<void> {
    for (const handler of this.handlers.get(event) ?? []) {
      await handler(payload, this.context);
    }
  }

  private async prompt(text: string, source: string): Promise<void> {
    let current = text;
    for (const handler of this.handlers.get("input") ?? []) {
      const result = (await handler(
        {
          type: "input",
          text: current,
          source,
          streamingBehavior: this.streaming ? "steer" : undefined,
        },
        this.context,
      )) as { action?: string; text?: string } | undefined;
      if (result?.action === "handled") return;
      if (result?.action === "transform" && typeof result.text === "string") {
        current = result.text;
      }
    }
    if (this.streaming) {
      this.steering.push(current);
      return;
    }
    this.compactionChecks += 1;
    let systemPrompt = "Pi base prompt";
    for (const handler of this.handlers.get("before_agent_start") ?? []) {
      const result = (await handler(
        { type: "before_agent_start", prompt: current, systemPrompt },
        this.context,
      )) as { systemPrompt?: string } | undefined;
      if (result?.systemPrompt) systemPrompt = result.systemPrompt;
    }
    this.turns.push(systemPrompt);
    await this.deliver(current);
  }

  private async deliver(text: string): Promise<void> {
    const message = { role: "user", content: [{ type: "text", text }] };
    await this.emit("message_end", { type: "message_end", message });
    this.append({ type: "message", message });
  }
}

/**
 * Runs one body against a real daemon serving a real socket over a fake Pi.
 *
 * `install` registers handlers before the desktop extension, exactly as an
 * extension loaded ahead of it would be; anything registered inside the body
 * runs after it.
 */
async function withDesktop(
  install: (pi: FakePi) => void,
  body: (socketPath: string, pi: FakePi) => Promise<void>,
): Promise<void> {
  const directory = await mkdtemp(join(tmpdir(), "scufris-desktop-"));
  const socketPath = join(directory, "scufris", "daemon.sock");
  process.env.SCUFRIS_DAEMON = "1";
  process.env.SCUFRIS_DESKTOP_SOCKET = socketPath;
  const pi = new FakePi();
  install(pi);
  try {
    desktop(pi as never);
    await pi.emit("session_start", { type: "session_start" });
    await body(socketPath, pi);
    await pi.emit("session_shutdown", { type: "session_shutdown" });
  } finally {
    delete process.env.SCUFRIS_DAEMON;
    delete process.env.SCUFRIS_DESKTOP_SOCKET;
    await rm(directory, { recursive: true, force: true });
  }
}

const submitLine = (id: string, text: string, force = false) =>
  `${JSON.stringify(force ? { v: 2, type: "submit", id, text, force } : { v: 2, type: "submit", id, text })}\n`;

/** Reads until the daemon answers this submission, or reports silence. */
async function answer(
  client: Companion,
  windowMs = 400,
): Promise<Record<string, unknown> | "silence"> {
  await new Promise((resolve) => setTimeout(resolve, windowMs));
  return (
    client
      .peek()
      .find(
        (message) => message.type === "ack" || message.type === "uncertain",
      ) ?? "silence"
  );
}

/** Waits out one window and reports whether an acknowledgment arrived in it. */
async function acknowledged(
  client: Companion,
  windowMs = 250,
): Promise<boolean> {
  await new Promise((resolve) => setTimeout(resolve, windowMs));
  return client.peek().some((message) => message.type === "ack");
}

test("a transcript starts the same turn a typed prompt starts", async () => {
  await withDesktop(
    (pi) => {
      // What Scufris installs per turn: its identity, the live job context, and
      // the final-response policy. None of it reaches a turn that skips the
      // preflight, which is the whole point of routing through a user prompt.
      pi.on("before_agent_start", (event) => ({
        systemPrompt: `${event.systemPrompt}\n\nScufris identity, active jobs, final-response policy`,
      }));
      // An extension loaded ahead of this one, as the voice module is.
      pi.on("input", async () => {
        await new Promise((resolve) => setTimeout(resolve, 1));
        return { action: "continue" };
      });
    },
    async (socketPath, pi) => {
      // And one loaded after it.
      pi.on("input", async () => ({ action: "continue" }));

      const client = await companion(socketPath);
      client.send(submitLine("pill-1", "open tasks"));
      assert.deepEqual(await client.next(), {
        v: 2,
        type: "ack",
        id: "pill-1",
      });

      assert.deepEqual(pi.turns, [
        "Pi base prompt\n\nScufris identity, active jobs, final-response policy",
      ]);
      assert.equal(pi.compactionChecks, 1);
      assert.deepEqual(acceptedSubmissions(pi.entries, 10), [
        { id: "pill-1", digest: submissionDigest("open tasks") },
      ]);
      // The transcript is a prompt, not an entry the person has to decode, and
      // the receipt that identifies it renders as words rather than a type name.
      assert.ok(pi.renderers.has(ACCEPTED_ENTRY));

      // The same sentence typed by hand is the typist's, however alike it looks.
      await pi.typed("open tasks");
      assert.deepEqual(acceptedSubmissions(pi.entries, 10), [
        { id: "pill-1", digest: submissionDigest("open tasks") },
      ]);
      client.socket.destroy();
    },
  );
});

test("a spoken prompt queued behind a running turn is acknowledged", async () => {
  await withDesktop(
    () => {},
    async (socketPath, pi) => {
      pi.streaming = true;
      const client = await companion(socketPath);
      client.send(submitLine("pill-1", "open tasks"));
      assert.equal(
        await acknowledged(client),
        false,
        "a queued prompt was acknowledged before it landed",
      );

      await pi.drain();
      assert.equal(await acknowledged(client), true);
      assert.deepEqual(acceptedSubmissions(pi.entries, 10), [
        { id: "pill-1", digest: submissionDigest("open tasks") },
      ]);
      client.socket.destroy();
    },
  );
});

test("a prompt another extension sends does not acknowledge the pill", async () => {
  await withDesktop(
    () => {},
    async (socketPath, pi) => {
      pi.streaming = true;
      const client = await companion(socketPath);
      client.send(submitLine("pill-1", "open tasks"));
      await new Promise((resolve) => setTimeout(resolve, 30));
      // Pi reports `extension` for every extension alike, so this send is
      // indistinguishable from the pill's by source. It is not this daemon's.
      pi.sendUserMessage("open tasks");
      await new Promise((resolve) => setTimeout(resolve, 10));

      await pi.drain();
      assert.equal(
        await acknowledged(client),
        false,
        "another extension's identical prompt acknowledged the pill",
      );
      assert.deepEqual(acceptedSubmissions(pi.entries, 10), []);
      client.socket.destroy();
    },
  );
});

test("another extension's identical prompt is never taken for the pill's", async () => {
  await withDesktop(
    () => {},
    async (socketPath, pi) => {
      const client = await companion(socketPath);
      client.send(submitLine("pill-1", "open tasks"));
      assert.deepEqual(await client.next(), {
        v: 2,
        type: "ack",
        id: "pill-1",
      });
      const commits = pi.entries.filter(
        (entry) => transcriptCommit(entry) !== undefined,
      ).length;

      // Alone, indistinguishable by words and by source class, and still not
      // this daemon's: it did not come from this daemon's own send.
      pi.sendUserMessage("open tasks");
      await new Promise((resolve) => setTimeout(resolve, 30));

      assert.equal(
        pi.entries.filter((entry) => transcriptCommit(entry) !== undefined)
          .length,
        commits,
        "a prompt this daemon never sent was recorded as one it did",
      );
      assert.deepEqual(acceptedSubmissions(pi.entries, 10), [
        { id: "pill-1", digest: submissionDigest("open tasks") },
      ]);
      client.socket.destroy();
    },
  );
});

test("a prompt a later handler answers itself never acknowledges the pill", async () => {
  await withDesktop(
    () => {},
    async (socketPath, pi) => {
      // Registered after the desktop extension, so this daemon cannot see the
      // outcome: it announced a prompt that will never land.
      pi.on("input", (event) =>
        event.text === "open tasks" && event.source === "extension"
          ? { action: "handled" }
          : { action: "continue" },
      );

      const client = await companion(socketPath);
      client.send(submitLine("pill-1", "open tasks"));
      assert.equal(await acknowledged(client), false);

      // The same words arriving later must not be taken for the answered one.
      await pi.typed("open tasks");
      await pi.overRpc("open tasks");
      assert.equal(await acknowledged(client), false);
      assert.deepEqual(acceptedSubmissions(pi.entries, 10), []);
      client.socket.destroy();
    },
  );
});

test("a prompt rewritten before this daemon sees it is not acknowledged", async () => {
  await withDesktop(
    (pi) => {
      pi.on("input", (event) =>
        event.text === "open tasks"
          ? { action: "transform", text: "open the tasks widget" }
          : { action: "continue" },
      );
    },
    async (socketPath, pi) => {
      const client = await companion(socketPath);
      client.send(submitLine("pill-1", "open tasks"));
      assert.equal(
        await acknowledged(client),
        false,
        "words the daemon never sent were acknowledged as the pill's",
      );
      // The rewritten words are in the conversation. They are not the pill's.
      assert.equal(
        userMessageDigest(pi.entries.at(-1)),
        submissionDigest("open the tasks widget"),
      );
      assert.deepEqual(acceptedSubmissions(pi.entries, 10), []);
      client.socket.destroy();
    },
  );
});

test("a prompt rewritten after this daemon sees it is not acknowledged", async () => {
  await withDesktop(
    () => {},
    async (socketPath, pi) => {
      pi.on("input", (event) =>
        event.text === "open tasks"
          ? { action: "transform", text: "open the tasks widget" }
          : { action: "continue" },
      );

      const client = await companion(socketPath);
      client.send(submitLine("pill-1", "open tasks"));
      assert.equal(await acknowledged(client), false);
      assert.equal(
        userMessageDigest(pi.entries.at(-1)),
        submissionDigest("open the tasks widget"),
      );
      assert.deepEqual(acceptedSubmissions(pi.entries, 10), []);
      client.socket.destroy();
    },
  );
});

test("a prompt typed while a spoken one waits leaves the pill unacknowledged", async () => {
  await withDesktop(
    () => {},
    async (socketPath, pi) => {
      pi.streaming = true;
      const client = await companion(socketPath);
      client.send(submitLine("pill-1", "open tasks"));
      await new Promise((resolve) => setTimeout(resolve, 30));
      // Pi does not say which landing is whose, so a second prompt in flight
      // is enough to stop the daemon claiming either of them.
      await pi.typed("and read my mail");

      await pi.drain();
      assert.equal(await acknowledged(client), false);
      assert.deepEqual(acceptedSubmissions(pi.entries, 10), []);
      // Both prompts are in the conversation; only the acknowledgment waits.
      assert.equal(
        pi.entries.filter((entry) => userMessageDigest(entry) !== undefined)
          .length,
        2,
      );
      client.socket.destroy();
    },
  );
});

test("a request that was dispatched and never landed is uncertain, not unsent", async () => {
  await withDesktop(
    () => {},
    async (socketPath, pi) => {
      // A handler after this one answers the prompt itself, so the words were
      // dispatched and never entered the conversation - and this daemon has no
      // way to know which of those happened.
      pi.on("input", (event) =>
        event.source === "extension"
          ? { action: "handled" }
          : { action: "continue" },
      );

      const first = await companion(socketPath);
      first.send(submitLine("pill-1", "book the flight"));
      assert.equal(await answer(first), "silence");
      first.socket.destroy();

      // The daemon restarts. Everything it knew in memory is gone; the session
      // is all that is left.
      await pi.emit("session_shutdown", { type: "session_shutdown" });
      await pi.emit("session_start", { type: "session_start" });

      const second = await companion(socketPath);
      second.send(submitLine("pill-1", "book the flight"));
      const refused = await answer(second);
      assert.notEqual(refused, "silence");
      assert.equal(
        (refused as Record<string, unknown>).type,
        "uncertain",
        "a request that may already have run was resent by an ordinary retry",
      );
      assert.equal(
        pi.entries.filter((entry) => userMessageDigest(entry) !== undefined)
          .length,
        0,
        "nothing may have been sent a second time",
      );
      second.socket.destroy();
    },
  );
});

test("only the person's own decision sends an uncertain request again", async () => {
  await withDesktop(
    () => {},
    async (socketPath, pi) => {
      let answering = true;
      pi.on("input", (event) =>
        answering && event.source === "extension"
          ? { action: "handled" }
          : { action: "continue" },
      );

      const client = await companion(socketPath);
      client.send(submitLine("pill-1", "book the flight"));
      assert.equal(await answer(client), "silence");
      await pi.emit("session_shutdown", { type: "session_shutdown" });
      await pi.emit("session_start", { type: "session_start" });

      // The person decided. This is the only thing that sends it again.
      answering = false;
      client.socket.destroy();
      const deciding = await companion(socketPath);
      deciding.send(submitLine("pill-1", "book the flight", true));
      const acknowledged = await answer(deciding);
      assert.deepEqual(acknowledged, { v: 2, type: "ack", id: "pill-1" });
      assert.deepEqual(acceptedSubmissions(pi.entries, 10), [
        { id: "pill-1", digest: submissionDigest("book the flight") },
      ]);
      deciding.socket.destroy();
    },
  );
});

test("a prompt that landed uncredited is uncertain rather than acknowledged", async () => {
  await withDesktop(
    () => {},
    async (socketPath, pi) => {
      pi.streaming = true;
      const client = await companion(socketPath);
      client.send(submitLine("pill-1", "book the flight"));
      await new Promise((resolve) => setTimeout(resolve, 30));
      // A second prompt in flight, so no landing can be credited to either.
      await pi.typed("and read my mail");
      await pi.drain();

      // The words are in the conversation. Nothing proves they are this
      // submission's, so the daemon says it does not know.
      assert.equal(
        pi.entries.filter(
          (entry) =>
            userMessageDigest(entry) === submissionDigest("book the flight"),
        ).length,
        1,
      );
      assert.deepEqual(acceptedSubmissions(pi.entries, 10), []);
      assert.equal(await answer(client), "silence");

      await pi.emit("session_shutdown", { type: "session_shutdown" });
      await pi.emit("session_start", { type: "session_start" });
      const retry = await companion(socketPath);
      retry.send(submitLine("pill-1", "book the flight"));
      const refused = await answer(retry);
      assert.equal(
        (refused as Record<string, unknown>).type,
        "uncertain",
        "words that may already be in the conversation were sent again",
      );
      assert.equal(
        pi.entries.filter(
          (entry) =>
            userMessageDigest(entry) === submissionDigest("book the flight"),
        ).length,
        1,
        "an ordinary retry put the same request in twice",
      );
      client.socket.destroy();
      retry.socket.destroy();
    },
  );
});

test("a restart is acknowledged from the session, not sent again", async () => {
  await withDesktop(
    () => {},
    async (socketPath, pi) => {
      const first = await companion(socketPath);
      first.send(submitLine("pill-1", "open tasks"));
      assert.deepEqual(await first.next(), { v: 2, type: "ack", id: "pill-1" });
      // The acknowledgment is lost: the daemon dies before the pill sees it.
      first.socket.destroy();
      await pi.emit("session_shutdown", { type: "session_shutdown" });

      const landed = pi.entries.length;
      await pi.emit("session_start", { type: "session_start" });
      const second = await companion(socketPath);
      second.send(submitLine("pill-1", "open tasks"));
      assert.deepEqual(await second.next(), {
        v: 2,
        type: "ack",
        id: "pill-1",
      });
      assert.equal(
        pi.entries.length,
        landed,
        "the retry entered the conversation a second time",
      );
      second.socket.destroy();
    },
  );
});

test("a commit renders as words rather than an internal type", () => {
  const theme = { fg: (_color: string, text: string) => text } as never;
  const render = commitRenderer();
  const shown = render(
    { data: { version: 1, id: "pill-1", digest: "d", entry: "e1" } } as never,
    {} as never,
    theme,
  );
  const lines = (shown?.render(60) ?? []).join("\n");
  assert.match(lines, new RegExp(TRANSCRIPT_LABEL));
  assert.doesNotMatch(lines, new RegExp(ACCEPTED_ENTRY));

  // A receipt from a version this daemon does not know falls back to Pi.
  assert.equal(
    render({ data: { version: 2 } } as never, {} as never, theme),
    undefined,
  );
});

test("a transcript at the byte bound is accepted and one past it is not", () => {
  // The companion measures in UTF-8 bytes, so this side must too: counting
  // UTF-16 units would accept text the companion cannot store.
  const cjk = "\u4f60\u597d";
  assert.equal(Buffer.byteLength(cjk, "utf8"), 6);
  assert.equal(cjk.length, 2);
  const filled = cjk.repeat(MAX_SUBMISSION_TEXT_BYTES / 6);
  const line = (text: string) =>
    JSON.stringify({ v: 2, type: "submit", id: "pill-1", text });

  assert.equal(decodeClientMessage(line(filled)).type, "submit");
  assert.throws(() => decodeClientMessage(line(filled + cjk)), ProtocolError);

  const astral = "\u{1f600}";
  assert.equal(Buffer.byteLength(astral, "utf8"), 4);
  assert.equal(astral.length, 2);
  const emoji = astral.repeat(MAX_SUBMISSION_TEXT_BYTES / 4);
  assert.equal(decodeClientMessage(line(emoji)).type, "submit");
  assert.throws(() => decodeClientMessage(line(emoji + astral)), ProtocolError);
});

const SERVER_MODULE = new URL(
  "../extensions/scufris/desktop/server.ts",
  import.meta.url,
).pathname;

/** Starts one short Node program with TypeScript stripping. */
function spawnChild(
  file: string,
  wrapper: string[] = [],
): {
  child: ReturnType<typeof spawn>;
  output: Promise<string>;
} {
  const argv = [
    ...wrapper,
    process.execPath,
    "--experimental-strip-types",
    "--no-warnings",
    file,
  ];
  const child = spawn(argv[0]!, argv.slice(1), {
    stdio: ["ignore", "pipe", "pipe"],
  });
  const output = new Promise<string>((resolve, reject) => {
    let out = "";
    let err = "";
    child.stdout?.on("data", (chunk) => (out += chunk));
    child.stderr?.on("data", (chunk) => (err += chunk));
    child.on("error", reject);
    child.on("close", () => resolve(out.trim() || err.trim()));
  });
  return { child, output };
}

/** Runs one short Node program to completion, returning its output. */
function runChild(file: string): Promise<string> {
  return spawnChild(file).output;
}

/** Waits for a child to report that it reached a barrier. */
async function awaitFile(path: string): Promise<void> {
  for (let waited = 0; waited < 400; waited += 1) {
    try {
      await stat(path);
      return;
    } catch {
      await new Promise((resolve) => setTimeout(resolve, 25));
    }
  }
  throw new Error(`the child never reached ${path}`);
}

/**
 * Source for a daemon that stalls at one mutation of the socket pathname.
 *
 * `stallAt` counts the barrier the runtime reaches after its last ownership
 * check and immediately before it unlinks or links the pathname. That is the
 * exact instant a lock that could expire or be stolen would let this process
 * damage a successor.
 */
function stallingDaemon(options: {
  socketPath: string;
  reached: string;
  proceed: string;
  stallAt: number;
  /** Written once the daemon is serving, if given. */
  ready?: string;
  stopAfterStart?: boolean;
  /**
   * Stalls the lock helper itself, between its probe and its unlink, rather
   * than stalling this process before it asks for the mutation.
   */
  inHelper?: boolean;
}): string {
  return `import { existsSync, writeFileSync } from "node:fs";
${
  options.inHelper
    ? `process.env.SCUFRIS_SOCKET_LOCK_BARRIER = ${JSON.stringify(options.proceed)};`
    : ""
}
import { ControlServer, SocketBusyError } from ${JSON.stringify(SERVER_MODULE)};
let mutations = 0;
const server = new ControlServer(
  ${JSON.stringify(options.socketPath)},
  { session: () => "s", deliver: async () => {}, accepted: () => [] },
  () => {},
  {
    lockTimeoutMs: 400,
    releaseTimeoutMs: 5_000,
    beforeMutate: () => {
      if (${options.inHelper ? "true" : "false"}) return;
      mutations += 1;
      if (mutations !== ${options.stallAt}) return;
      writeFileSync(${JSON.stringify(options.reached)}, "x");
      const until = Date.now() + 10_000;
      while (!existsSync(${JSON.stringify(options.proceed)}) && Date.now() < until) {}
    },
  },
);
try {
  await server.start();
  ${options.ready ? `writeFileSync(${JSON.stringify(options.ready)}, "x");` : ""}
  process.stdout.write("won\\n");
  ${options.stopAfterStart ? "await server.stop();" : ""}
} catch (error) {
  process.stdout.write(error instanceof SocketBusyError ? "busy\\n" : String(error) + "\\n");
}
`;
}

/** Answers whether this kernel gives an unprivileged network namespace. */
function canUnshareNetwork(): boolean {
  try {
    return (
      spawnSync("unshare", ["-r", "-n", "true"], { stdio: "ignore" }).status ===
      0
    );
  } catch {
    return false;
  }
}

/** Leaves a socket pathname behind with nothing listening on it. */
async function stranded(socketPath: string): Promise<void> {
  const dead = new ControlServer(
    socketPath,
    simpleHost(async () => {}),
    () => {},
  );
  await dead.start();
  await new Promise<void>((resolve) =>
    (
      dead as unknown as { server: { close(cb: () => void): void } }
    ).server.close(() => resolve()),
  );
}

test("the ownership lock is released the moment its holder stops existing", async () => {
  const directory = await mkdtemp(join(tmpdir(), "scufris-desktop-"));
  const socketPath = join(directory, "scufris", "daemon.sock");
  await mkdir(join(directory, "scufris"), { recursive: true, mode: 0o700 });
  const reached = join(directory, "reached");
  const holder = join(directory, "holder.ts");
  await writeFile(
    holder,
    stallingDaemon({
      socketPath,
      reached,
      proceed: join(directory, "never"),
      stallAt: 1,
    }),
  );

  const { child } = spawnChild(holder);
  await awaitFile(reached);

  // While the holder exists the lock is the kernel's answer, not a record this
  // process could misjudge.
  const blocked = new ControlServer(
    socketPath,
    simpleHost(async () => {}),
    () => {},
    { lockTimeoutMs: 150 },
  );
  await assert.rejects(() => blocked.start(), SocketBusyError);

  // A holder killed outright leaves nothing behind: no lock file, no lease to
  // wait out, no grace period before the next daemon may start.
  child.kill("SIGKILL");
  await new Promise<void>((resolve) => child.once("exit", () => resolve()));
  const successor = new ControlServer(
    socketPath,
    simpleHost(async () => {}),
    () => {},
    { lockTimeoutMs: 150 },
  );
  await successor.start();
  const client = await companion(socketPath);
  client.send('{"v":2,"type":"ping"}\n');
  assert.deepEqual(await client.next(), { v: 2, type: "pong" });
  client.socket.destroy();

  await successor.stop();
  await rm(directory, { recursive: true, force: true });
});

test("no starter can interleave with the unlink of a stale socket", async () => {
  const directory = await mkdtemp(join(tmpdir(), "scufris-desktop-"));
  const socketPath = join(directory, "scufris", "daemon.sock");
  const reached = join(directory, "reached");
  const proceed = join(directory, "proceed");
  const ready = join(directory, "ready");
  const stalled = join(directory, "stalled.ts");
  await stranded(socketPath);
  await writeFile(
    stalled,
    // Stalled inside the lock helper, between its probe of the stale socket and
    // its removal of the pathname - the process holding the lock, paused in the
    // middle of the mutation it owns.
    stallingDaemon({
      socketPath,
      reached: `${proceed}.reached`,
      proceed,
      ready,
      stallAt: 1,
      inHelper: true,
    }),
  );

  const { child, output } = spawnChild(stalled);
  await awaitFile(`${proceed}.reached`);

  // A successor cannot reach the pathname at all while that removal is
  // pending, so there is no window in which the stalled daemon could unlink
  // something another daemon had claimed.
  const successor = new ControlServer(
    socketPath,
    simpleHost(async () => {}),
    () => {},
    { lockTimeoutMs: 200 },
  );
  await assert.rejects(() => successor.start(), SocketBusyError);

  await writeFile(proceed, "x");
  await awaitFile(ready);

  // The daemon that held the lock owns the socket, and it is the one serving.
  const client = await companion(socketPath);
  client.send('{"v":2,"type":"ping"}\n');
  assert.deepEqual(await client.next(), { v: 2, type: "pong" });
  client.socket.destroy();

  child.kill("SIGTERM");
  assert.equal(await output, "won");
  await rm(directory, { recursive: true, force: true });
});

test("no starter can interleave with the unlink a departing daemon performs", async () => {
  const directory = await mkdtemp(join(tmpdir(), "scufris-desktop-"));
  const socketPath = join(directory, "scufris", "daemon.sock");
  const reached = join(directory, "reached");
  const proceed = join(directory, "proceed");
  const departing = join(directory, "departing.ts");
  await writeFile(
    departing,
    // Nothing is stale, so the claim links once; the second mutation is the
    // removal shutdown performs after recognising its own socket.
    stallingDaemon({
      socketPath,
      reached,
      proceed,
      stallAt: 2,
      stopAfterStart: true,
    }),
  );

  const { output } = spawnChild(departing);
  await awaitFile(reached);

  // The successor waits for the departing daemon's lock instead of claiming
  // the pathname it is about to remove.
  const blocked = new ControlServer(
    socketPath,
    simpleHost(async () => {}),
    () => {},
    { lockTimeoutMs: 200 },
  );
  await assert.rejects(() => blocked.start(), SocketBusyError);

  await writeFile(proceed, "x");
  assert.equal(await output, "won");

  const successor = new ControlServer(
    socketPath,
    simpleHost(async () => {}),
    () => {},
  );
  await successor.start();
  const client = await companion(socketPath);
  client.send('{"v":2,"type":"ping"}\n');
  assert.deepEqual(await client.next(), { v: 2, type: "pong" });
  client.socket.destroy();
  await successor.stop();
  await rm(directory, { recursive: true, force: true });
});

test("a daemon whose lock helper dies changes nothing afterwards", async () => {
  const directory = await mkdtemp(join(tmpdir(), "scufris-desktop-"));
  const socketPath = join(directory, "scufris", "daemon.sock");
  await mkdir(join(directory, "scufris"), { recursive: true, mode: 0o700 });
  const isolated = join(directory, "scufris", ".private.sock");
  await writeFile(isolated, "");

  const lock = await OwnershipLock.acquire(ownershipLockFile(socketPath), 500);
  assert.ok(lock.holder, "the lock must be held by a process, not a belief");
  process.kill(lock.holder!, "SIGKILL");

  // The kernel released the lock the instant the helper died. Nothing this
  // daemon asks for afterwards may touch the pathname, and asking must fail
  // rather than fall back to doing it here.
  await assert.rejects(() => lock.claim(socketPath, isolated), SocketBusyError);
  await assert.rejects(
    () => lock.release(socketPath, { device: 1, inode: 1 }),
    /released before this daemon finished|lock helper failed/,
  );
  await lock.close();

  // And a successor can take the lock at once: there is nothing left holding it.
  const successor = await OwnershipLock.acquire(
    ownershipLockFile(socketPath),
    500,
  );
  await successor.claim(socketPath, isolated);
  assert.ok((await stat(socketPath)).isFile());
  await successor.close();
  await rm(directory, { recursive: true, force: true });
});

test("a successor takes over from a daemon whose lock helper died", async () => {
  const directory = await mkdtemp(join(tmpdir(), "scufris-desktop-"));
  const socketPath = join(directory, "scufris", "daemon.sock");
  const stale = new ControlServer(
    socketPath,
    simpleHost(async () => {}),
    () => {},
  );
  await stale.start();
  // The listener dies without the cleanup a graceful stop would do, which is
  // the state a killed daemon leaves behind.
  await new Promise<void>((resolve) =>
    (
      stale as unknown as { server: { close(cb: () => void): void } }
    ).server.close(() => resolve()),
  );

  const successor = new ControlServer(
    socketPath,
    simpleHost(async () => {}),
    () => {},
  );
  await successor.start();
  const client = await companion(socketPath);
  client.send('{"v":2,"type":"ping"}\n');
  assert.deepEqual(await client.next(), { v: 2, type: "pong" });
  client.socket.destroy();

  // The old daemon now shuts down. It no longer owns this pathname, and its
  // shutdown must leave the successor serving.
  await stale.stop();
  const still = await companion(socketPath);
  still.send('{"v":2,"type":"ping"}\n');
  assert.deepEqual(await still.next(), { v: 2, type: "pong" });
  still.socket.destroy();

  await successor.stop();
  await rm(directory, { recursive: true, force: true });
});

test("a socket path with spaces in it is claimed and given back", async () => {
  // `SCUFRIS_DESKTOP_SOCKET` is whatever the person configured and
  // `XDG_RUNTIME_DIR` is whatever the session set. Both are ordinary pathnames,
  // and an ordinary pathname may hold a space.
  const directory = await mkdtemp(join(tmpdir(), "scufris desktop "));
  const socketPath = join(directory, "my scufris", "daemon.sock");
  await mkdir(join(directory, "my scufris"), { recursive: true, mode: 0o700 });

  const owner = new ControlServer(
    socketPath,
    simpleHost(async () => {}),
    () => {},
  );
  await owner.start();
  const client = await companion(socketPath);
  client.send('{"v":2,"type":"ping"}\n');
  assert.deepEqual(await client.next(), { v: 2, type: "pong" });
  client.socket.destroy();

  // A second daemon must still be refused: the lock guards the same pathname.
  const intruder = new ControlServer(
    socketPath,
    simpleHost(async () => {}),
    () => {},
    { lockTimeoutMs: 200 },
  );
  await assert.rejects(() => intruder.start(), SocketBusyError);

  // And the owner gives its own socket back, which needs the same path to
  // survive the round trip through the lock helper.
  await owner.stop();
  assert.deepEqual(await readdir(join(directory, "my scufris")), [
    "daemon.sock.lock",
  ]);
  await rm(directory, { recursive: true, force: true });
});

test("one daemon owns a socket whatever name reaches it", async () => {
  const directory = await mkdtemp(join(tmpdir(), "scufris-desktop-"));
  const real = join(directory, "scufris");
  await mkdir(real, { recursive: true, mode: 0o700 });
  const alias = join(directory, "runtime");
  await symlink(real, alias);
  const socketPath = join(real, "daemon.sock");
  // Names a configured `SCUFRIS_DESKTOP_SOCKET` can plausibly carry, all of
  // which reach the same directory entry.
  const aliases = [
    `${real}/./daemon.sock`,
    `${real}/../scufris/daemon.sock`,
    `${alias}/daemon.sock`,
  ];

  for (const name of aliases) {
    assert.equal(canonicalSocketPath(name), socketPath, name);
    assert.equal(ownershipLockFile(name), `${socketPath}.lock`, name);
  }

  const owner = new ControlServer(
    socketPath,
    simpleHost(async () => {}),
    () => {},
  );
  await owner.start();
  for (const name of aliases) {
    // A daemon that guarded a different lock for the same pathname would take
    // the socket from under this one instead of being refused.
    const intruder = new ControlServer(
      name,
      simpleHost(async () => {}),
      () => {},
      { lockTimeoutMs: 200 },
    );
    await assert.rejects(() => intruder.start(), SocketBusyError, name);
  }

  const client = await companion(socketPath);
  client.send('{"v":2,"type":"ping"}\n');
  assert.deepEqual(await client.next(), { v: 2, type: "pong" });
  client.socket.destroy();
  await owner.stop();
  await rm(directory, { recursive: true, force: true });
});

test("a daemon in another network namespace is serialized by the same lock", async (t) => {
  if (!canUnshareNetwork()) {
    t.skip("this kernel does not allow an unprivileged network namespace");
    return;
  }
  const directory = await mkdtemp(join(tmpdir(), "scufris-desktop-"));
  const socketPath = join(directory, "scufris", "daemon.sock");
  await mkdir(join(directory, "scufris"), { recursive: true, mode: 0o700 });
  const reached = join(directory, "reached");
  const holder = join(directory, "holder.ts");
  await writeFile(
    holder,
    stallingDaemon({
      socketPath,
      reached,
      proceed: join(directory, "never"),
      stallAt: 1,
    }),
  );

  // The filesystem is shared; only the network namespace differs. An abstract
  // socket is scoped to that namespace, so it would let both processes believe
  // they held the lock. A lock on the lock file's inode does not.
  const { child } = spawnChild(holder, ["unshare", "-r", "-n"]);
  await awaitFile(reached);

  const blocked = new ControlServer(
    socketPath,
    simpleHost(async () => {}),
    () => {},
    { lockTimeoutMs: 200 },
  );
  await assert.rejects(() => blocked.start(), SocketBusyError);

  child.kill("SIGKILL");
  await new Promise<void>((resolve) => child.once("exit", () => resolve()));
  // The kernel released the lock with the process, across the namespace too.
  const successor = new ControlServer(
    socketPath,
    simpleHost(async () => {}),
    () => {},
    { lockTimeoutMs: 500 },
  );
  await successor.start();
  await successor.stop();
  await rm(directory, { recursive: true, force: true });
});

test("only one of two separate daemons takes over a socket left by a dead one", async () => {
  const directory = await mkdtemp(join(tmpdir(), "scufris-desktop-"));
  const socketPath = join(directory, "scufris", "daemon.sock");
  await mkdir(join(directory, "scufris"), { recursive: true, mode: 0o700 });
  const starter = join(directory, "starter.ts");
  await writeFile(
    starter,
    `import { ControlServer } from ${JSON.stringify(SERVER_MODULE)};
const server = new ControlServer(
  ${JSON.stringify(socketPath)},
  { session: () => "s", deliver: async () => {}, accepted: () => [] },
  () => {},
);
try {
  await server.start();
  process.stdout.write("won\\n");
  await new Promise((resolve) => setTimeout(resolve, 1200));
  await server.stop();
} catch (error) {
  process.stdout.write("busy\\n");
}
`,
  );

  // The state a crash leaves, and the state every starter then races over.
  await stranded(socketPath);

  const outcomes = await Promise.all([runChild(starter), runChild(starter)]);
  assert.equal(
    outcomes.filter((outcome) => outcome === "won").length,
    1,
    `both daemons took over: ${JSON.stringify(outcomes)}`,
  );

  await rm(directory, { recursive: true, force: true });
});
