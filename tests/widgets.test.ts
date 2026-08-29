import assert from "node:assert/strict";
import test from "node:test";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import type { CatalogEntry } from "../agent/extensions/scufris/service/protocol.ts";
import {
  DESKTOP_CONTROL_EVENT,
  WidgetCommandError,
  type WidgetAnswer,
  type DesktopControl,
  type WidgetNotice,
  type WidgetRequest,
} from "../agent/extensions/scufris/service/client.ts";
import widgets, {
  WIDGET_EVENT_MESSAGE,
} from "../agent/extensions/scufris/widgets/index.ts";

type Handler = (event: any, context: any) => any;

/** One recorded custom message, with the options it was delivered under. */
interface Sent {
  message: Record<string, unknown>;
  options: Record<string, unknown> | undefined;
}

const note: CatalogEntry = {
  id: "note",
  name: "Note",
  description: "Show a few lines of text. Data is { text: string }.",
};

const clock: CatalogEntry = {
  id: "clock",
  name: "Clock",
  description: "The time, in one panel.",
};

/** The control the service link hands over, with the socket taken out. */
class FakeControl implements DesktopControl {
  readonly sent: WidgetRequest[] = [];
  listener?: (notice: WidgetNotice) => void;
  answer: (command: WidgetRequest) => Promise<WidgetAnswer> = async () => ({
    surface: "widget-1",
  });

  async request(command: WidgetRequest): Promise<WidgetAnswer> {
    this.sent.push(command);
    return await this.answer(command);
  }

  watchWidgets(listener: (notice: WidgetNotice) => void): void {
    this.listener = listener;
  }

  /** The conversation window is a sibling verb; widgets never ask for it. */
  readonly shown: boolean[] = [];

  async conversation(up: boolean): Promise<void> {
    this.shown.push(up);
  }

  /** Delivers one unsolicited frontend message, as the socket would. */
  notice(notice: WidgetNotice): void {
    assert.ok(this.listener, "the extension is watching the control");
    this.listener(notice);
  }
}

function harness() {
  const handlers = new Map<string, Handler[]>();
  const busHandlers = new Map<string, Array<(data: unknown) => void>>();
  const tools = new Map<string, any>();
  const notices: Array<{ message: string; level: string }> = [];
  const sent: Sent[] = [];
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
    on(event: string, handler: Handler) {
      const eventHandlers = handlers.get(event) ?? [];
      eventHandlers.push(handler);
      handlers.set(event, eventHandlers);
    },
    registerTool(tool: any) {
      tools.set(tool.name, tool);
    },
    sendMessage(message: Record<string, unknown>, options?: unknown) {
      sent.push({ message, options: options as Record<string, unknown> });
    },
  } as unknown as ExtensionAPI;
  const context = {
    hasUI: true,
    ui: {
      notify(message: string, level: string) {
        notices.push({ message, level });
      },
    },
  };

  const role = process.env.SCUFRIS_ROLE;
  process.env.SCUFRIS_ROLE = "orchestrator";
  try {
    widgets(api);
  } finally {
    if (role === undefined) delete process.env.SCUFRIS_ROLE;
    else process.env.SCUFRIS_ROLE = role;
  }

  const emit = async (event: string, value: unknown = {}) => {
    for (const handler of handlers.get(event) ?? [])
      await handler(value, context);
  };

  return {
    tools,
    notices,
    sent,
    emit,
    /** Hands the extension one control, the way the service extension does. */
    serve(control: FakeControl | undefined) {
      api.events.emit(DESKTOP_CONTROL_EVENT, { control });
    },
    /** Runs one registered tool and returns its result. */
    async call(name: string, params: unknown = {}) {
      const tool = tools.get(name);
      assert.ok(tool, `${name} is registered`);
      return (await tool.execute("call-1", params)) as {
        isError?: boolean;
        details: Record<string, unknown>;
      };
    },
  };
}

/** Starts one extension already serving a catalog. */
async function serving(catalog: CatalogEntry[] = [note]) {
  const pi = harness();
  const control = new FakeControl();
  await pi.emit("session_start");
  pi.serve(control);
  control.notice({ type: "catalog", widgets: catalog });
  return { pi, control };
}

test("no widget tool exists until a frontend says what it has", async () => {
  const pi = harness();
  const control = new FakeControl();
  await pi.emit("session_start");
  pi.serve(control);
  // The link is up and nothing has announced anything yet. Registering now
  // would offer the model widget names picked by this repository rather than by
  // the desktop it is talking to.
  assert.equal(pi.tools.size, 0);

  control.notice({ type: "catalog", widgets: [note, clock] });
  assert.deepEqual([...pi.tools.keys()].sort(), [
    "scufris_widget_clear",
    "scufris_widget_close",
    "scufris_widget_open",
    "scufris_widget_update",
  ]);

  const open = pi.tools.get("scufris_widget_open");
  assert.deepEqual(open.parameters.properties.widget.enum, ["note", "clock"]);
  assert.match(open.description, /note \(Note\)/);
  assert.match(open.description, /clock \(Clock\)/);
});

test("an open names its surface and later commands carry that name", async () => {
  const { pi, control } = await serving();

  const opened = await pi.call("scufris_widget_open", {
    widget: "note",
    data: { text: "the harness is green" },
  });
  assert.equal(opened.isError, undefined);
  assert.deepEqual(opened.details, {
    widget: "note",
    posture: "exhibit",
    surface: "widget-1",
  });

  control.answer = async () => ({});
  const updated = await pi.call("scufris_widget_update", {
    surface: "widget-1",
    data: { text: "141 tests pass" },
  });
  assert.equal(updated.isError, undefined);
  assert.deepEqual(control.sent, [
    {
      type: "open",
      widget: "note",
      posture: "exhibit",
      data: { text: "the harness is green" },
    },
    {
      type: "update",
      surface: "widget-1",
      data: { text: "141 tests pass" },
    },
  ]);
});

test("a refused command is a tool error carrying the frontend's own code", async () => {
  const { pi, control } = await serving();
  control.answer = async () => {
    throw new WidgetCommandError("surface_not_found", "widget-9 is not open");
  };

  const result = await pi.call("scufris_widget_update", {
    surface: "widget-9",
    data: {},
  });
  assert.equal(result.isError, true);
  // The code is the whole point: a surface that is gone, a widget that does
  // not exist, and a desktop that is not running each call for something
  // different, and only the code separates them.
  assert.equal(result.details.error_code, "surface_not_found");
  assert.match(String(result.details.error), /widget-9 is not open/);
});

test("a surface the person closed reaches the conversation without starting a turn", async () => {
  const { pi, control } = await serving();
  await pi.call("scufris_widget_open", { widget: "note" });

  control.notice({ type: "closed", surface: "widget-1" });

  assert.equal(pi.sent.length, 1);
  const event = pi.sent[0] as Sent;
  assert.equal(event.message.customType, WIDGET_EVENT_MESSAGE);
  assert.match(String(event.message.content), /widget-1/);
  assert.deepEqual(event.message.details, {
    surface: "widget-1",
    event: "closed",
  });
  // A follow-up, and not a turn: the person closing a panel is not a request.
  assert.deepEqual(event.options, {
    deliverAs: "followUp",
    triggerTurn: false,
  });
});

test("a command with nothing to carry it is refused rather than sent", async () => {
  const { pi } = await serving();
  // What a session shutdown looks like from here: the service extension gives
  // back the control before it closes the link under it.
  pi.serve(undefined);

  const result = await pi.call("scufris_widget_open", { widget: "note" });
  assert.equal(result.isError, true);
  assert.equal(result.details.error_code, "service_unavailable");
});

test("a frontend that announces different widgets is said out loud, not retyped", async () => {
  const { pi, control } = await serving();
  const open = pi.tools.get("scufris_widget_open");

  control.notice({ type: "catalog", widgets: [note, clock] });

  // Pi cannot withdraw a registered tool, so the names the model sees stay the
  // ones the first frontend announced. Silently keeping them would leave the
  // person with no way to learn why a widget they installed cannot be opened.
  assert.equal(pi.tools.get("scufris_widget_open"), open);
  assert.deepEqual(open.parameters.properties.widget.enum, ["note"]);
  assert.equal(pi.notices.length, 1);
  assert.match(String(pi.notices[0]?.message), /different widgets/);
});
