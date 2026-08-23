import assert from "node:assert/strict";
import test from "node:test";
import type { AssistantMessage } from "@earendil-works/pi-ai";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import speech, {
  extractSpokenParagraph,
  lastSafeAssistantParagraph,
  SpeechPlaybackError,
  type SpeechPlayback,
} from "../extensions/scufris/voice/speech.ts";

const usage = {
  input: 0,
  output: 0,
  cacheRead: 0,
  cacheWrite: 0,
  totalTokens: 0,
  cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
};

function assistant(
  text: string,
  stopReason: AssistantMessage["stopReason"] = "stop",
  toolCall = false,
): AssistantMessage {
  return {
    role: "assistant",
    content: [
      { type: "thinking", thinking: "not spoken" },
      { type: "text", text },
      ...(toolCall
        ? [
            {
              type: "toolCall" as const,
              id: "call",
              name: "read",
              arguments: {},
            },
          ]
        : []),
    ],
    api: "test",
    provider: "test",
    model: "test",
    usage,
    stopReason,
    timestamp: 0,
  };
}

class FakePlayback implements SpeechPlayback {
  readonly played: string[] = [];
  cancellations = 0;
  error?: Error;

  async play(text: string): Promise<void> {
    this.played.push(text);
    if (this.error) throw this.error;
  }

  async cancel(): Promise<void> {
    this.cancellations += 1;
  }
}

type Handler = (event: any, context: any) => any;

function harness(playback: FakePlayback, mode = "tui") {
  const handlers = new Map<string, Handler[]>();
  const commands = new Map<string, { handler: Handler }>();
  const entries: any[] = [];
  const notices: Array<{ message: string; type: string }> = [];
  const api = {
    on(event: string, handler: Handler) {
      const eventHandlers = handlers.get(event) ?? [];
      eventHandlers.push(handler);
      handlers.set(event, eventHandlers);
    },
    registerCommand(name: string, command: { handler: Handler }) {
      commands.set(name, command);
    },
    appendEntry(customType: string, data: unknown) {
      entries.push({
        type: "custom",
        id: `custom-${entries.length}`,
        customType,
        data,
      });
      return `custom-${entries.length - 1}`;
    },
  } as unknown as ExtensionAPI;
  const context = {
    mode,
    ui: {
      notify(message: string, type: string) {
        notices.push({ message, type });
      },
    },
    sessionManager: {
      getBranch() {
        return entries;
      },
    },
  };
  speech(api, { playback });
  return {
    entries,
    notices,
    commands,
    async emit(event: string, value: unknown = {}) {
      let result: unknown;
      for (const handler of handlers.get(event) ?? []) {
        result = await handler(value, context);
      }
      return result;
    },
    async command(args: string) {
      await commands.get("speech")?.handler(args, context);
    },
  };
}

function addAssistant(
  entries: any[],
  text: string,
  options: {
    stopReason?: AssistantMessage["stopReason"];
    toolCall?: boolean;
  } = {},
) {
  entries.push({
    type: "message",
    id: `assistant-${entries.length}`,
    message: assistant(text, options.stopReason, options.toolCall),
  });
}

async function flushPromises() {
  await new Promise((resolve) => setImmediate(resolve));
}

test("speech extraction accepts only bounded complete plain prose", () => {
  assert.equal(
    extractSpokenParagraph(
      assistant(
        "The update is complete and ready for review.\n\n- Changed files\n- Tests pass",
      ),
    ),
    "The update is complete and ready for review.",
  );
  assert.equal(
    extractSpokenParagraph(assistant("This paragraph\nwraps safely.")),
    "This paragraph wraps safely.",
  );

  for (const unsafe of [
    "- The update is complete.",
    "See /tmp/result for details.",
    "See package.json for details.",
    "Read https://example.invalid now.",
    "Use `npm test` now.",
    "Issue #42 is fixed.",
    "**Everything is complete.**",
    "This is not a complete sentence",
    "x".repeat(1_001) + ".",
  ]) {
    assert.equal(extractSpokenParagraph(assistant(unsafe)), undefined, unsafe);
  }
  assert.equal(
    extractSpokenParagraph(assistant("Do not speak this.", "toolUse", true)),
    undefined,
  );
  assert.equal(
    extractSpokenParagraph(assistant("An incomplete response.", "length")),
    undefined,
  );
});

test("last response extraction never falls back from an unsafe final response", () => {
  const entries: any[] = [];
  addAssistant(entries, "This earlier response is safe.");
  entries.push({ type: "custom", id: "state", customType: "other", data: {} });
  assert.equal(
    lastSafeAssistantParagraph(entries)?.paragraph,
    "This earlier response is safe.",
  );
  addAssistant(entries, "See /unsafe/path now.");
  assert.equal(lastSafeAssistantParagraph(entries), undefined);

  entries.push({
    type: "custom",
    id: "response",
    customType: "scufris-response-v1",
    data: {
      version: 1,
      spoken: "Only this paragraph reaches Piper.",
      artifact_id: "0123456789abcdef01234567",
    },
  });
  assert.deepEqual(lastSafeAssistantParagraph(entries), {
    entryId: "response",
    paragraph: "Only this paragraph reaches Piper.",
  });
});

test("speech on, off, once, and replay have deterministic turn behavior", async () => {
  const originalRole = process.env.SCUFRIS_ROLE;
  const originalSpeech = process.env.SCUFRIS_SPEECH;
  process.env.SCUFRIS_ROLE = "orchestrator";
  delete process.env.SCUFRIS_SPEECH;
  try {
    const playback = new FakePlayback();
    const app = harness(playback);
    await app.emit("session_start");

    assert.equal(await app.emit("agent_start"), undefined);
    await app.emit("agent_settled");
    assert.deepEqual(playback.played, []);

    await app.command("once");
    assert.equal(await app.emit("agent_start"), undefined);
    addAssistant(
      app.entries,
      "This response is spoken once.\n\nVisual detail remains here.",
    );
    await app.emit("agent_settled");
    assert.deepEqual(playback.played, ["This response is spoken once."]);
    assert.equal(
      (
        app.entries.filter((entry) => entry.type === "custom").at(-1)?.data as {
          mode: string;
        }
      ).mode,
      "off",
    );

    assert.equal(await app.emit("agent_start"), undefined);
    addAssistant(app.entries, "This response stays silent.");
    await app.emit("agent_settled");
    assert.equal(playback.played.length, 1);

    await app.command("on");
    assert.equal(await app.emit("agent_start"), undefined);
    addAssistant(app.entries, "Speech mode remains enabled.");
    await app.emit("agent_settled");
    assert.equal(playback.played.at(-1), "Speech mode remains enabled.");

    await app.command("off");
    assert.equal(playback.cancellations >= 2, true);
    assert.equal(await app.emit("agent_start"), undefined);
    addAssistant(app.entries, "This response also stays silent.");
    await app.emit("agent_settled");

    await app.command("replay");
    assert.equal(playback.played.at(-1), "This response also stays silent.");
    assert.deepEqual(
      app.notices
        .map((notice) => notice.message)
        .filter((message) => message.startsWith("Speech")),
      ["Speech armed for one response.", "Speech mode on.", "Speech mode off."],
    );
  } finally {
    if (originalRole === undefined) delete process.env.SCUFRIS_ROLE;
    else process.env.SCUFRIS_ROLE = originalRole;
    if (originalSpeech === undefined) delete process.env.SCUFRIS_SPEECH;
    else process.env.SCUFRIS_SPEECH = originalSpeech;
  }
});

test("ordinary and extension-triggered turns each speak only their settled response", async () => {
  const originalRole = process.env.SCUFRIS_ROLE;
  const originalSpeech = process.env.SCUFRIS_SPEECH;
  process.env.SCUFRIS_ROLE = "orchestrator";
  process.env.SCUFRIS_SPEECH = "1";
  try {
    const playback = new FakePlayback();
    const app = harness(playback);
    await app.emit("session_start");

    await app.emit("before_agent_start", { systemPrompt: "base" });
    await app.emit("agent_start");
    addAssistant(app.entries, "The ordinary response is settled and safe.");
    await app.emit("agent_settled");

    const wakes = [
      {
        customType: "scufris-job-event",
        payload: "ready: implementation-complete",
        response: "The milestone response is settled and safe.",
      },
      {
        customType: "scufris-job-event",
        payload: "done: raw lifecycle payload",
        response: "The done response is settled and safe.",
      },
      {
        customType: "scufris-detail-feedback",
        payload: "raw human review feedback",
        response: "The review feedback response is settled and safe.",
      },
    ];
    for (const wake of wakes) {
      app.entries.push({
        type: "message",
        id: `wake-${app.entries.length}`,
        message: {
          role: "custom",
          customType: wake.customType,
          content: wake.payload,
        },
      });
      await app.emit("agent_start");
      addAssistant(app.entries, wake.response);
      await app.emit("agent_settled");
    }

    assert.deepEqual(playback.played, [
      "The ordinary response is settled and safe.",
      ...wakes.map((wake) => wake.response),
    ]);
  } finally {
    if (originalRole === undefined) delete process.env.SCUFRIS_ROLE;
    else process.env.SCUFRIS_ROLE = originalRole;
    if (originalSpeech === undefined) delete process.env.SCUFRIS_SPEECH;
    else process.env.SCUFRIS_SPEECH = originalSpeech;
  }
});

test("wake settlement without a new response never replays prior speech", async () => {
  const originalRole = process.env.SCUFRIS_ROLE;
  const originalSpeech = process.env.SCUFRIS_SPEECH;
  process.env.SCUFRIS_ROLE = "orchestrator";
  process.env.SCUFRIS_SPEECH = "1";
  try {
    const playback = new FakePlayback();
    const app = harness(playback);
    await app.emit("session_start");

    await app.emit("agent_start");
    addAssistant(app.entries, "The prior response is spoken once.");
    await app.emit("agent_settled");

    app.entries.push({
      type: "message",
      id: "failed-wake",
      message: {
        role: "custom",
        customType: "scufris-job-event",
        content: "done: raw wake without a response",
      },
    });
    await app.emit("agent_start");
    await app.emit("agent_settled");

    assert.deepEqual(playback.played, ["The prior response is spoken once."]);
    assert.equal(app.notices.at(-1)?.message, "No safe response to speak.");
  } finally {
    if (originalRole === undefined) delete process.env.SCUFRIS_ROLE;
    else process.env.SCUFRIS_ROLE = originalRole;
    if (originalSpeech === undefined) delete process.env.SCUFRIS_SPEECH;
    else process.env.SCUFRIS_SPEECH = originalSpeech;
  }
});

test("settlement, input, reload state, unsafe output, and errors fail safely", async () => {
  const originalRole = process.env.SCUFRIS_ROLE;
  const originalSpeech = process.env.SCUFRIS_SPEECH;
  process.env.SCUFRIS_ROLE = "orchestrator";
  process.env.SCUFRIS_SPEECH = "1";
  try {
    const playback = new FakePlayback();
    const app = harness(playback);
    await app.emit("session_start");
    await app.emit("input", { text: "new request", source: "interactive" });
    assert.equal(playback.cancellations, 2);

    await app.emit("agent_start");
    addAssistant(app.entries, "I will inspect the result.", {
      stopReason: "toolUse",
      toolCall: true,
    });
    assert.deepEqual(playback.played, []);
    addAssistant(app.entries, "The final result is ready.");
    await app.emit("agent_settled");
    assert.deepEqual(playback.played, ["The final result is ready."]);

    await app.command("off");
    const reloadedPlayback = new FakePlayback();
    const reloaded = harness(reloadedPlayback);
    reloaded.entries.push(...app.entries);
    await reloaded.emit("session_start", { reason: "reload" });
    assert.equal(await reloaded.emit("agent_start"), undefined);
    await reloaded.emit("agent_settled");

    await reloaded.command("on");
    await reloaded.emit("agent_start");
    addAssistant(reloaded.entries, "See /tmp/private for the result.");
    await reloaded.emit("agent_settled");
    assert.equal(reloadedPlayback.played.length, 0);
    assert.equal(
      reloaded.notices.at(-1)?.message,
      "No safe response to speak.",
    );

    reloadedPlayback.error = new SpeechPlaybackError(
      "Speech synthesis failed.",
    );
    await reloaded.emit("agent_start");
    addAssistant(reloaded.entries, "This safe response reaches playback.");
    await reloaded.emit("agent_settled");
    await flushPromises();
    assert.deepEqual(reloaded.notices.at(-1), {
      message: "Speech synthesis failed.",
      type: "error",
    });

    await reloaded.emit("session_shutdown");
    assert.equal(reloadedPlayback.cancellations >= 2, true);
  } finally {
    if (originalRole === undefined) delete process.env.SCUFRIS_ROLE;
    else process.env.SCUFRIS_ROLE = originalRole;
    if (originalSpeech === undefined) delete process.env.SCUFRIS_SPEECH;
    else process.env.SCUFRIS_SPEECH = originalSpeech;
  }
});

test("normal Pi and non-TUI Scufris modes never register or speak", async () => {
  const originalRole = process.env.SCUFRIS_ROLE;
  const originalSpeech = process.env.SCUFRIS_SPEECH;
  process.env.SCUFRIS_SPEECH = "1";
  try {
    delete process.env.SCUFRIS_ROLE;
    const ordinary = harness(new FakePlayback());
    assert.equal(ordinary.commands.size, 0);

    for (const mode of ["rpc", "json", "print"]) {
      process.env.SCUFRIS_ROLE = "orchestrator";
      const playback = new FakePlayback();
      const app = harness(playback, mode);
      await app.emit("session_start");
      assert.equal(await app.emit("agent_start"), undefined);
      addAssistant(app.entries, "This response must stay silent.");
      await app.emit("agent_settled");
      await app.command("on");
      assert.deepEqual(playback.played, []);
      assert.equal(
        app.entries.some((entry) => entry.type === "custom"),
        false,
      );
    }
  } finally {
    if (originalRole === undefined) delete process.env.SCUFRIS_ROLE;
    else process.env.SCUFRIS_ROLE = originalRole;
    if (originalSpeech === undefined) delete process.env.SCUFRIS_SPEECH;
    else process.env.SCUFRIS_SPEECH = originalSpeech;
  }
});
