import { StringEnum, Type } from "@earendil-works/pi-ai";
import {
  defineTool,
  type ExtensionAPI,
  type ExtensionContext,
} from "@earendil-works/pi-coding-agent";
import { toolResult } from "../shared/runtime.ts";
import {
  MAX_IDENTIFIER_LENGTH,
  type CatalogEntry,
} from "../desktop/protocol.ts";
import {
  WIDGET_CONTROL_EVENT,
  WidgetCommandError,
  type WidgetCommand,
  type WidgetControl,
  type WidgetControlSignal,
  type WidgetNotice,
} from "../desktop/server.ts";

/** The custom message type a surface the person closed arrives as. */
export const WIDGET_EVENT_MESSAGE = "scufris-widget-event";

/** Renders the installed widgets as the line the open tool's description carries. */
export function catalogSummary(widgets: CatalogEntry[]): string {
  return widgets
    .map((widget) => `${widget.id} (${widget.name}): ${widget.description}`)
    .join(" ");
}

/**
 * Turns one refused command into the tool result the model reads.
 *
 * The companion's own code is kept, because it is the difference between a
 * widget name that does not exist, a surface that is already gone, and a
 * desktop that is not running. Each one calls for something different.
 */
function failure(error: unknown) {
  const detail = error instanceof Error ? error.message : String(error);
  const code =
    error instanceof WidgetCommandError ? error.code : "widget_command_failed";
  return toolResult(
    { error: detail, error_code: code },
    true,
    `Widget error: ${code}: ${detail}`,
  );
}

/**
 * Opens, updates, and closes the desktop companion's widgets.
 *
 * This is the only place the daemon originates a request rather than answering
 * one, and the tools are typed from what the companion says it has: the widget
 * names the model can use are the widget names that are installed, and a
 * session that never met a companion offers none of them.
 */
export default function widgets(pi: ExtensionAPI): void {
  // Only the daemon has a companion to command. A worker Pi that registered
  // these tools would offer the model four verbs that answer nothing.
  if (process.env.SCUFRIS_DAEMON !== "1") return;

  let control: WidgetControl | undefined;
  let context: ExtensionContext | undefined;
  /** The widget names the tools were registered with, once they have been. */
  let installed: string[] | undefined;
  const notify = (message: string, level: "info" | "error") => {
    if (context?.hasUI) context.ui.notify(`Scufris widgets: ${message}`, level);
    else if (level === "error") console.error(`scufris widgets: ${message}`);
  };

  const ask = async (command: WidgetCommand) => {
    if (!control) {
      throw new WidgetCommandError(
        "companion_unavailable",
        "The Scufris desktop control socket is not being served.",
      );
    }
    return await control.request(command);
  };

  const surfaceSchema = Type.String({
    pattern: `^[A-Za-z0-9._-]{1,${MAX_IDENTIFIER_LENGTH}}$`,
    description: "The surface identifier scufris_widget_open returned.",
  });
  const dataSchema = {
    description:
      "The widget's own payload. Each widget's description says what it takes.",
  };

  const register = (catalog: CatalogEntry[]) => {
    const ids = catalog.map((widget) => widget.id);

    pi.registerTool(
      defineTool({
        name: "scufris_widget_open",
        label: "Open widget",
        description:
          "Open one widget on the Scufris desktop and return the surface identifier that names it. " +
          "An exhibit is Scufris showing something beside the pill; it ages out on its own, so it needs no closing. " +
          "An instrument is a panel the user asked to keep, in one of four screen-edge slots. " +
          `Installed widgets: ${catalogSummary(catalog)}`,
        parameters: Type.Object(
          {
            widget: StringEnum(ids, {
              description: "Which installed widget to open.",
            }),
            posture: Type.Optional(
              StringEnum(["exhibit", "instrument"] as const, {
                description:
                  "Where it lives. Exhibit unless the user asked to keep it.",
              }),
            ),
            data: Type.Optional(Type.Unknown(dataSchema)),
          },
          { additionalProperties: false },
        ),
        async execute(_id, params) {
          const posture = params.posture ?? "exhibit";
          try {
            const answer = await ask({
              type: "widget_open",
              widget: params.widget,
              posture,
              data: params.data ?? {},
            });
            const surface = answer.surface;
            if (surface === undefined) {
              throw new WidgetCommandError(
                "invalid_answer",
                "The companion opened a widget and did not name its surface.",
              );
            }
            return toolResult(
              { widget: params.widget, posture, surface },
              false,
              `Opened ${params.widget} as ${surface}.`,
            );
          } catch (error) {
            return failure(error);
          }
        },
      }),
    );

    pi.registerTool(
      defineTool({
        name: "scufris_widget_update",
        label: "Update widget",
        description:
          "Hand new data to one open surface. The widget decides what to show for it.",
        parameters: Type.Object(
          { surface: surfaceSchema, data: Type.Unknown(dataSchema) },
          { additionalProperties: false },
        ),
        async execute(_id, params) {
          try {
            await ask({
              type: "widget_update",
              surface: params.surface,
              data: params.data,
            });
            return toolResult(
              { surface: params.surface, state: "updated" },
              false,
              `Updated ${params.surface}.`,
            );
          } catch (error) {
            return failure(error);
          }
        },
      }),
    );

    pi.registerTool(
      defineTool({
        name: "scufris_widget_close",
        label: "Close widget",
        description:
          "Take one surface off the screen. An exhibit retires on its own, so close one only when the user asks.",
        parameters: Type.Object(
          { surface: surfaceSchema },
          { additionalProperties: false },
        ),
        async execute(_id, params) {
          try {
            await ask({ type: "widget_close", surface: params.surface });
            return toolResult(
              { surface: params.surface, state: "closed" },
              false,
              `Closed ${params.surface}.`,
            );
          } catch (error) {
            return failure(error);
          }
        },
      }),
    );

    pi.registerTool(
      defineTool({
        name: "scufris_widget_clear",
        label: "Clear widgets",
        description:
          "Take every widget Scufris opened off the screen. Surfaces the user kept stay where they are.",
        parameters: Type.Object({}, { additionalProperties: false }),
        async execute() {
          try {
            await ask({ type: "widget_clear" });
            return toolResult({ state: "cleared" }, false, "Cleared widgets.");
          } catch (error) {
            return failure(error);
          }
        },
      }),
    );
  };

  const adopt = (catalog: CatalogEntry[]) => {
    const ids = catalog.map((widget) => widget.id);
    if (ids.length === 0) {
      notify("the desktop companion has no widgets installed", "info");
      return;
    }
    if (installed) {
      // Pi cannot withdraw a registered tool, so the names the model sees are
      // the names the first companion announced. A companion that ships a
      // different set is worth saying out loud rather than silently offering
      // names that open nothing.
      if (ids.join(" ") !== installed.join(" ")) {
        notify(
          "the desktop companion now ships different widgets; restart Scufris to use them",
          "info",
        );
      }
      return;
    }
    installed = ids;
    register(catalog);
  };

  const observe = (notice: WidgetNotice) => {
    if (notice.type === "catalog") {
      adopt(notice.widgets);
      return;
    }
    // A surface went away: the person closed it with its own tick, or a fourth
    // exhibit pushed it off the shelf. The conversation is told so its idea of
    // what is on screen does not drift from what is, and so nothing reopens
    // what was just put away.
    pi.sendMessage(
      {
        customType: WIDGET_EVENT_MESSAGE,
        content: `The widget surface ${notice.surface} was closed on the desktop.`,
        display: true,
        details: { surface: notice.surface, event: notice.event },
      },
      { deliverAs: "followUp", triggerTurn: false },
    );
  };

  pi.events.on(WIDGET_CONTROL_EVENT, (value: unknown) => {
    control = (value as Partial<WidgetControlSignal> | undefined)?.control;
    // The listener belongs to the control, and a control is one session's. It
    // is registered again with each one, which is what keeps the catalog and
    // the person's own closes arriving across a restart.
    control?.watchWidgets(observe);
  });

  pi.on("session_start", (_event, ctx) => {
    context = ctx;
  });

  pi.on("session_shutdown", () => {
    control = undefined;
    context = undefined;
  });
}
