import assert from "node:assert/strict";
import test from "node:test";
import type { AssistantMessage } from "@earendil-works/pi-ai";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { SPOKEN_EVENT } from "../extensions/scufris/shared/spoken.ts";
import speech, {
  extractSpokenParagraph,
  lastSafeAssistantParagraph,
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

type Handler = (event: any, context: any) => any;

function harness(mode = "tui") {
  const handlers = new Map<string, Handler[]>();
  const commands = new Map<string, { handler: Handler }>();
  const entries: any[] = [];
  const notices: Array<{ message: string; type: string }> = [];
  const spoken: string[] = [];
  const api = {
    events: {
      emit(event: string, value: any) {
        if (event !== SPOKEN_EVENT) return;
        if (typeof value.speak === "string") spoken.push(value.speak);
      },
    },
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
    hasUI: mode === "tui",
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
  speech(api);
  return {
    entries,
    notices,
    commands,
    spoken,
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
      spoken: "Only this paragraph reaches the speaker.",
      artifact_id: "0123456789abcdef01234567",
    },
  });
  assert.deepEqual(lastSafeAssistantParagraph(entries), {
    entryId: "response",
    paragraph: "Only this paragraph reaches the speaker.",
  });
});

test("speech on, off, once, and replay have deterministic turn behavior", async () => {
  const originalRole = process.env.SCUFRIS_ROLE;
  const originalSpeech = process.env.SCUFRIS_SPEECH;
  process.env.SCUFRIS_ROLE = "orchestrator";
  delete process.env.SCUFRIS_SPEECH;
  try {
    const app = harness();
    await app.emit("session_start");

    assert.equal(await app.emit("agent_start"), undefined);
    await app.emit("agent_settled");
    assert.deepEqual(app.spoken, []);

    await app.command("once");
    assert.equal(await app.emit("agent_start"), undefined);
    addAssistant(
      app.entries,
      "This response is spoken once.\n\nVisual detail remains here.",
    );
    await app.emit("agent_settled");
    assert.deepEqual(app.spoken, ["This response is spoken once."]);
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
    assert.equal(app.spoken.length, 1);

    await app.command("on");
    assert.equal(await app.emit("agent_start"), undefined);
    addAssistant(app.entries, "Speech mode remains enabled.");
    await app.emit("agent_settled");
    assert.equal(app.spoken.at(-1), "Speech mode remains enabled.");

    await app.command("off");
    assert.equal(await app.emit("agent_start"), undefined);
    addAssistant(app.entries, "This response also stays silent.");
    await app.emit("agent_settled");
    assert.equal(app.spoken.length, 2);

    // Replay is the one verb that speaks with the mode off: the person just
    // asked for it, which is a different thing from a turn settling.
    await app.command("replay");
    assert.equal(app.spoken.at(-1), "This response also stays silent.");
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
    const app = harness();
    await app.emit("session_start");

    await app.emit("before_agent_start", { systemPrompt: "base" });
    await app.emit("agent_start");
    addAssistant(app.entries, "The ordinary response is settled and safe.");
    await app.emit("agent_settled");

    const wakes = [
      {
        customType: "scufris-job-event",
        payload: "blocked: implementation needs mediation",
        response: "The blockage response is settled and safe.",
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

    assert.deepEqual(app.spoken, [
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
    const app = harness();
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

    await app.emit("agent_start");
    await app.emit("agent_settled");

    assert.deepEqual(app.spoken, ["The prior response is spoken once."]);
    assert.equal(
      app.notices.filter(
        (notice) => notice.message === "No safe response to speak.",
      ).length,
      1,
    );
  } finally {
    if (originalRole === undefined) delete process.env.SCUFRIS_ROLE;
    else process.env.SCUFRIS_ROLE = originalRole;
    if (originalSpeech === undefined) delete process.env.SCUFRIS_SPEECH;
    else process.env.SCUFRIS_SPEECH = originalSpeech;
  }
});

test("a reloaded session resumes the mode it was left in and never speaks unsafe output", async () => {
  const originalRole = process.env.SCUFRIS_ROLE;
  const originalSpeech = process.env.SCUFRIS_SPEECH;
  process.env.SCUFRIS_ROLE = "orchestrator";
  process.env.SCUFRIS_SPEECH = "1";
  try {
    const app = harness();
    await app.emit("session_start");

    await app.emit("agent_start");
    addAssistant(app.entries, "I will inspect the result.", {
      stopReason: "toolUse",
      toolCall: true,
    });
    assert.deepEqual(app.spoken, []);
    addAssistant(app.entries, "The final result is ready.");
    await app.emit("agent_settled");
    assert.deepEqual(app.spoken, ["The final result is ready."]);

    await app.command("off");
    const reloaded = harness();
    reloaded.entries.push(...app.entries);
    await reloaded.emit("session_start", { reason: "reload" });
    assert.equal(await reloaded.emit("agent_start"), undefined);
    await reloaded.emit("agent_settled");
    assert.deepEqual(reloaded.spoken, []);

    await reloaded.command("on");
    await reloaded.emit("agent_start");
    addAssistant(reloaded.entries, "See /tmp/private for the result.");
    await reloaded.emit("agent_settled");
    assert.deepEqual(reloaded.spoken, []);
    assert.equal(
      reloaded.notices.at(-1)?.message,
      "No safe response to speak.",
    );

    await reloaded.emit("agent_start");
    addAssistant(reloaded.entries, "See /tmp/still-private for the result.");
    await reloaded.emit("agent_settled");
    assert.equal(
      reloaded.notices.filter(
        (notice) => notice.message === "No safe response to speak.",
      ).length,
      1,
    );
  } finally {
    if (originalRole === undefined) delete process.env.SCUFRIS_ROLE;
    else process.env.SCUFRIS_ROLE = originalRole;
    if (originalSpeech === undefined) delete process.env.SCUFRIS_SPEECH;
    else process.env.SCUFRIS_SPEECH = originalSpeech;
  }
});

test("speech follows the role, not the mode, because the agent runs headless", async () => {
  const originalRole = process.env.SCUFRIS_ROLE;
  const originalSpeech = process.env.SCUFRIS_SPEECH;
  process.env.SCUFRIS_SPEECH = "1";
  try {
    // Ordinary Pi is not Scufris and registers nothing.
    delete process.env.SCUFRIS_ROLE;
    assert.equal(harness().commands.size, 0);

    // The service runs its agent in RPC mode, so a mode gate here would mean
    // Scufris never speaks again. What it says goes to whoever owns a speaker.
    process.env.SCUFRIS_ROLE = "orchestrator";
    const app = harness("rpc");
    await app.emit("session_start");
    await app.emit("agent_start");
    addAssistant(app.entries, "This response reaches the speaker.");
    await app.emit("agent_settled");
    assert.deepEqual(app.spoken, ["This response reaches the speaker."]);
  } finally {
    if (originalRole === undefined) delete process.env.SCUFRIS_ROLE;
    else process.env.SCUFRIS_ROLE = originalRole;
    if (originalSpeech === undefined) delete process.env.SCUFRIS_SPEECH;
    else process.env.SCUFRIS_SPEECH = originalSpeech;
  }
});
