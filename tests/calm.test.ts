import assert from "node:assert/strict";
import test from "node:test";
import type { AssistantMessage } from "@earendil-works/pi-ai";
import {
  AssistantMessageComponent,
  CustomMessageComponent,
  ToolExecutionComponent,
  initTheme,
  type ExtensionAPI,
  type MarkdownTransformer,
} from "@earendil-works/pi-coding-agent";
import calm, { resolveCalmCommand } from "../agent/extensions/scufris/calm.ts";
import { surfacePrompt } from "../agent/extensions/scufris/service/protocol.ts";

initTheme("dark", false);

const usage = {
  input: 0,
  output: 0,
  cacheRead: 0,
  cacheWrite: 0,
  totalTokens: 0,
  cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
};

function assistantMessage(
  content: AssistantMessage["content"],
  stopReason: AssistantMessage["stopReason"],
): AssistantMessage {
  return {
    role: "assistant",
    content,
    api: "anthropic-messages",
    provider: "anthropic",
    model: "test",
    usage,
    stopReason,
    timestamp: 0,
  };
}

function rendered(component: { render(width: number): string[] }): string {
  return component.render(100).join("\n");
}

function harness(entries: any[] = []) {
  const commands = new Map<
    string,
    { handler(args: string, context: any): Promise<void> }
  >();
  const handlers = new Map<
    string,
    Array<(event: unknown, context: any) => void>
  >();
  const notices: Array<{ message: string; type: string }> = [];
  const labels: Array<string | undefined> = [];
  let transformer: MarkdownTransformer = (markdown) => markdown;
  const api = {
    registerMarkdownTransformer(transform: MarkdownTransformer) {
      transformer = transform;
    },
    appendEntry(customType: string, data: unknown) {
      entries.push({ type: "custom", customType, data });
    },
    on(event: string, handler: (event: unknown, context: any) => void) {
      handlers.set(event, [...(handlers.get(event) ?? []), handler]);
    },
    registerCommand(name: string, command: any) {
      commands.set(name, command);
    },
  } as unknown as ExtensionAPI;
  calm(api);
  const context = {
    sessionManager: { getBranch: () => entries },
    ui: {
      setHiddenThinkingLabel(label?: string) {
        labels.push(label);
      },
      notify(message: string, type: string) {
        notices.push({ message, type });
      },
    },
  };
  return {
    entries,
    notices,
    labels,
    /** What Pi would draw for one message of that kind. */
    drawn(markdown: string, messageType: "user" | "assistant" = "user") {
      return transformer(markdown, {
        messageType,
        isStreaming: false,
        availableWidth: 80,
      });
    },
    async emit(event: string) {
      for (const handler of handlers.get(event) ?? [])
        await handler({}, context);
    },
    async command(args: string) {
      await commands.get("calm")?.handler(args, context);
    },
  };
}

const agenda = {
  name: "agenda",
  description: "Show one day of the journal and everything standing around it.",
  input_schema: { type: "object", additionalProperties: true },
};

test("Calm commands are explicit, inspectable, and idempotent", () => {
  assert.deepEqual(resolveCalmCommand("", true), {
    enabled: true,
    changed: false,
    notice: "Calm mode on.",
    warning: false,
  });
  assert.equal(resolveCalmCommand("on", true).changed, false);
  assert.equal(resolveCalmCommand("off", true).enabled, false);
  assert.deepEqual(resolveCalmCommand("toggle", false), {
    enabled: false,
    changed: false,
    notice: "Use /calm on or off.",
    warning: true,
  });
});

test("Calm hides operational rows and restores explicit session state", async () => {
  const app = harness();
  await app.emit("session_start");
  assert.equal(app.labels.at(-1), "");

  const intermediate = assistantMessage(
    [
      { type: "thinking", thinking: "private reasoning" },
      { type: "text", text: "I will inspect it." },
      { type: "toolCall", id: "call", name: "read", arguments: {} },
    ],
    "toolUse",
  );
  const final = assistantMessage(
    [
      { type: "thinking", thinking: "private conclusion" },
      { type: "text", text: "Finished." },
    ],
    "stop",
  );
  const intermediateComponent = new AssistantMessageComponent(intermediate);
  const finalComponent = new AssistantMessageComponent(final);
  const toolComponent = new ToolExecutionComponent(
    "unknown-tool",
    "call",
    {},
    {},
    undefined,
    { requestRender() {} } as any,
    process.cwd(),
  );
  const eventComponent = new CustomMessageComponent({
    role: "custom",
    customType: "scufris-job-event",
    content: "job completed",
    display: true,
    timestamp: 0,
  });

  assert.equal(rendered(intermediateComponent), "");
  assert.equal(rendered(toolComponent), "");
  assert.equal(rendered(eventComponent), "");
  assert.match(rendered(finalComponent), /Finished\./);
  assert.doesNotMatch(rendered(finalComponent), /private conclusion/);

  await app.command("");
  await app.command("on");
  assert.equal(app.entries.length, 0);
  assert.deepEqual(app.notices.slice(-2), [
    { message: "Calm mode on.", type: "info" },
    { message: "Calm mode on.", type: "info" },
  ]);

  await app.command("off");
  await app.command("off");
  assert.equal(app.entries.length, 1);
  assert.equal(app.labels.at(-1), undefined);
  intermediateComponent.invalidate();
  finalComponent.invalidate();
  assert.match(rendered(intermediateComponent), /I will inspect it\./);
  assert.notEqual(rendered(toolComponent), "");
  assert.match(rendered(eventComponent), /job completed/);

  await app.command("toggle");
  assert.deepEqual(app.notices.at(-1), {
    message: "Use /calm on or off.",
    type: "warning",
  });

  const reloaded = harness(app.entries);
  await reloaded.emit("session_start");
  assert.equal(reloaded.labels.at(-1), undefined);
  await reloaded.command("");
  assert.deepEqual(reloaded.notices.at(-1), {
    message: "Calm mode off.",
    type: "info",
  });

  await reloaded.command("on");
  intermediateComponent.invalidate();
  assert.equal(rendered(intermediateComponent), "");

  const freshSession = harness();
  await freshSession.emit("session_start");
  assert.equal(freshSession.labels.at(-1), "");
});

test("Calm draws a surface message as the words that were sent", async () => {
  const app = harness();
  await app.emit("session_start");

  // The envelope is built by the sender, so the test asks the sender for it.
  // Two files agreeing on a format is the only way this can be wrong.
  const asked = surfacePrompt("what is left today?", [agenda], []);
  assert.match(asked, /<widgets>/);
  assert.ok(asked.length > 200);
  assert.equal(app.drawn(asked), "what is left today?");

  // The escaping survives, including the closing tag of the block it is in.
  const tricky = "close </user_message> & <b>, then stop";
  assert.equal(app.drawn(surfacePrompt(tricky, [agenda], [])), tricky);

  // An attachment is content somebody sent, so it keeps its name.
  const photo = {
    id: "01J0000000000000000000A1",
    name: "photo.jpg",
    media_type: "image/jpeg",
    size: 1024,
  };
  assert.equal(
    app.drawn(surfacePrompt("look at this", [agenda], [photo])),
    "look at this\n\n_attached: photo.jpg_",
  );
  assert.equal(
    app.drawn(surfacePrompt("", [agenda], [photo])),
    "_attached: photo.jpg_",
  );

  // Everything else is left alone: what was typed here, an assistant message,
  // and an envelope that does not parse.
  assert.equal(app.drawn("just what I typed"), "just what I typed");
  assert.equal(app.drawn(asked, "assistant"), asked);
  const broken = asked.replace('"what is left today?"', "not json");
  assert.equal(app.drawn(broken), broken);

  // Calm off is the raw truth, and it is one command away.
  await app.command("off");
  assert.equal(app.drawn(asked), asked);
  await app.command("on");
  assert.equal(app.drawn(asked), "what is left today?");
});
