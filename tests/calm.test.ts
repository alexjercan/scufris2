import assert from "node:assert/strict";
import test from "node:test";
import type { AssistantMessage } from "@earendil-works/pi-ai";
import {
  AssistantMessageComponent,
  CustomMessageComponent,
  ToolExecutionComponent,
  initTheme,
  type ExtensionAPI,
} from "@earendil-works/pi-coding-agent";
import calm from "../extensions/scufris/calm.ts";

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

test("Calm hides operational transcript rows and restores them on toggle", async (t) => {
  const originalCalm = process.env.SCUFRIS_CALM;
  delete process.env.SCUFRIS_CALM;
  t.after(() => {
    if (originalCalm === undefined) delete process.env.SCUFRIS_CALM;
    else process.env.SCUFRIS_CALM = originalCalm;
  });

  const commands = new Map<
    string,
    { handler: (args: string, context: any) => Promise<void> }
  >();
  const sessionStarts: Array<(event: unknown, context: any) => void> = [];
  const api = {
    on(event: string, handler: (event: unknown, context: any) => void) {
      if (event === "session_start") sessionStarts.push(handler);
    },
    registerCommand(
      name: string,
      command: { handler: (args: string, context: any) => Promise<void> },
    ) {
      commands.set(name, command);
    },
  } as unknown as ExtensionAPI;
  calm(api);

  const labels: Array<string | undefined> = [];
  const notices: string[] = [];
  const context = {
    ui: {
      setHiddenThinkingLabel(label?: string) {
        labels.push(label);
      },
      notify(message: string) {
        notices.push(message);
      },
    },
  };
  sessionStarts[0]?.({}, context);
  assert.equal(labels.at(-1), "");

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
  const aborted = assistantMessage(
    [{ type: "toolCall", id: "aborted", name: "read", arguments: {} }],
    "aborted",
  );
  const intermediateComponent = new AssistantMessageComponent(intermediate);
  const finalComponent = new AssistantMessageComponent(final);
  const abortedComponent = new AssistantMessageComponent(aborted);
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
  assert.match(rendered(abortedComponent), /Operation aborted/);
  assert.equal(intermediate.content.length, 3);

  await commands.get("calm")?.handler("", context);
  intermediateComponent.invalidate();
  finalComponent.invalidate();
  assert.equal(labels.at(-1), undefined);
  assert.equal(notices.at(-1), "Calm mode off.");
  assert.match(rendered(intermediateComponent), /I will inspect it\./);
  assert.notEqual(rendered(toolComponent), "");
  assert.match(rendered(eventComponent), /job completed/);

  const reloadedCommands = new Map<
    string,
    { handler: (args: string, context: any) => Promise<void> }
  >();
  calm({
    on() {},
    registerCommand(name: string, command: any) {
      reloadedCommands.set(name, command);
    },
  } as unknown as ExtensionAPI);
  assert.match(rendered(intermediateComponent), /I will inspect it\./);

  await reloadedCommands.get("calm")?.handler("", context);
  intermediateComponent.invalidate();
  finalComponent.invalidate();
  assert.equal(notices.at(-1), "Calm mode on.");
  assert.equal(rendered(intermediateComponent), "");

  await reloadedCommands.get("calm")?.handler("", context);
  process.env.SCUFRIS_CALM = "1";
  const popupStarts: Array<(event: unknown, context: any) => void> = [];
  calm({
    on(event: string, handler: (event: unknown, context: any) => void) {
      if (event === "session_start") popupStarts.push(handler);
    },
    registerCommand() {},
  } as unknown as ExtensionAPI);
  popupStarts[0]?.({}, context);
  intermediateComponent.invalidate();
  assert.equal(labels.at(-1), "");
  assert.equal(rendered(intermediateComponent), "");
});
