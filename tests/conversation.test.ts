import assert from "node:assert/strict";
import test from "node:test";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import {
  DESKTOP_CONTROL_EVENT,
  WidgetCommandError,
} from "../agent/extensions/scufris/service/client.ts";
import conversation from "../agent/extensions/scufris/conversation.ts";

/** The control the service link hands over, with the socket taken out. */
class FakeControl {
  readonly asked: boolean[] = [];
  answer: (up: boolean) => Promise<void> = async () => {};

  async request(): Promise<never> {
    throw new Error("widgets are not this extension's");
  }

  watchWidgets(): void {}

  async conversation(up: boolean): Promise<void> {
    this.asked.push(up);
    await this.answer(up);
  }
}

function harness(role = "orchestrator") {
  const busHandlers = new Map<string, Array<(data: unknown) => void>>();
  const tools = new Map<string, any>();
  const api = {
    events: {
      emit(channel: string, data: unknown) {
        for (const handler of busHandlers.get(channel) ?? []) handler(data);
      },
      on(channel: string, handler: (data: unknown) => void) {
        const listeners = busHandlers.get(channel) ?? [];
        listeners.push(handler);
        busHandlers.set(channel, listeners);
        return () => {};
      },
    },
    on() {},
    registerTool(tool: any) {
      tools.set(tool.name, tool);
    },
    sendMessage() {},
  } as unknown as ExtensionAPI;

  const previous = process.env.SCUFRIS_ROLE;
  process.env.SCUFRIS_ROLE = role;
  try {
    conversation(api);
  } finally {
    if (previous === undefined) delete process.env.SCUFRIS_ROLE;
    else process.env.SCUFRIS_ROLE = previous;
  }

  return {
    tools,
    serve(control: FakeControl | undefined) {
      api.events.emit(DESKTOP_CONTROL_EVENT, { control });
    },
    async call(params: unknown) {
      const tool = tools.get("scufris_conversation");
      assert.ok(tool, "scufris_conversation is registered");
      return (await tool.execute("call-1", params)) as {
        isError?: boolean;
        details: Record<string, unknown>;
      };
    },
  };
}

test("the window is told what to be rather than told to flip", async () => {
  const harnessed = harness();
  const control = new FakeControl();
  harnessed.serve(control);

  const shown = await harnessed.call({ action: "show" });
  assert.ok(!shown.isError);
  assert.deepEqual(shown.details, { state: "shown" });

  const closed = await harnessed.call({ action: "close" });
  assert.ok(!closed.isError);
  assert.deepEqual(closed.details, { state: "closed" });

  // Twice in a row, and the second one asks for the same thing rather than
  // undoing the first. This is the whole reason it is not a toggle.
  await harnessed.call({ action: "show" });
  await harnessed.call({ action: "show" });
  assert.deepEqual(control.asked, [true, false, true, true]);
});

test("a machine with no screen says so rather than claiming the window is up", async () => {
  const harnessed = harness();
  const control = new FakeControl();
  control.answer = async () => {
    throw new WidgetCommandError(
      "no_frontend",
      "there is no frontend connected",
    );
  };
  harnessed.serve(control);

  const result = await harnessed.call({ action: "show" });
  assert.equal(result.isError, true);
  // The service's own code, kept: a desktop that is not running is a different
  // thing to tell the user than a service that is not up.
  assert.equal(result.details.error_code, "no_frontend");
});

test("a link that is not open is refused before anything is sent", async () => {
  const harnessed = harness();
  harnessed.serve(new FakeControl());
  // The service extension emits an empty signal when the link goes away.
  harnessed.serve(undefined);

  const result = await harnessed.call({ action: "show" });
  assert.equal(result.isError, true);
  assert.equal(result.details.error_code, "service_unavailable");
});

test("a worker Pi is offered no window, because it has no screen", () => {
  const harnessed = harness("worker");
  assert.equal(harnessed.tools.size, 0);
});
