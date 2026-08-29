/**
 * The one window Scufris can put on the desktop itself.
 *
 * Everything else the agent opens is a widget: a panel with a payload, from a
 * catalog the frontend announces. This is the frontend's own conversation
 * window, built in rather than installed, and all it can be told is whether to
 * be up.
 *
 * It says `show` and `close` rather than toggling. A toggle from something that
 * cannot see the screen does one of two opposite things and cannot tell which,
 * so Scufris would have no way to know whether it just showed the conversation
 * or hid it. Asking for what is already there is harmless, which is what makes
 * the explicit verb the cheap one.
 */

import { StringEnum, Type } from "@earendil-works/pi-ai";
import { defineTool, type ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { toolResult } from "./shared/runtime.ts";
import {
  DESKTOP_CONTROL_EVENT,
  WidgetCommandError,
  type DesktopControl,
  type DesktopControlSignal,
} from "./service/client.ts";

/** Registers the tool that shows and closes the conversation window. */
export default function conversation(pi: ExtensionAPI): void {
  // Only the foreground Scufris has a screen. A worker Pi that registered this
  // would offer the model a verb that answers nothing.
  if (process.env.SCUFRIS_ROLE !== "orchestrator") return;

  let control: DesktopControl | undefined;

  pi.registerTool(
    defineTool({
      name: "scufris_conversation",
      label: "Conversation window",
      description:
        "Show or close the Scufris conversation window on the user's desktop. " +
        "It draws what has been said and gives the user a line to type on. " +
        "Show it when the user asks to see the conversation, or when an answer is long enough to be read rather than heard. " +
        "The user can show and close it themselves, so close it only when they ask.",
      parameters: Type.Object(
        {
          action: StringEnum(["show", "close"] as const, {
            description: "Whether the window should be up or away.",
          }),
        },
        { additionalProperties: false },
      ),
      async execute(_id, params) {
        const up = params.action === "show";
        if (!control) {
          return toolResult(
            {
              error: "The Scufris service link is not open.",
              error_code: "service_unavailable",
            },
            true,
            "Conversation error: service_unavailable",
          );
        }
        try {
          await control.conversation(up);
          return toolResult(
            { state: up ? "shown" : "closed" },
            false,
            up ? "Showed the conversation." : "Closed the conversation.",
          );
        } catch (error) {
          // The frontend's own code is kept. `no_frontend` is a machine with no
          // desktop running, which is a different thing to tell the user than a
          // service that is not up.
          const detail = error instanceof Error ? error.message : String(error);
          const code =
            error instanceof WidgetCommandError
              ? error.code
              : "conversation_failed";
          return toolResult(
            { error: detail, error_code: code },
            true,
            `Conversation error: ${code}: ${detail}`,
          );
        }
      },
    }),
  );

  pi.events.on(DESKTOP_CONTROL_EVENT, (value: unknown) => {
    control = (value as Partial<DesktopControlSignal> | undefined)?.control;
  });
}
