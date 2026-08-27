import { join } from "node:path";
import type {
  ExtensionAPI,
  ExtensionContext,
} from "@earendil-works/pi-coding-agent";
import { SPOKEN_EVENT, type SpokenSignal } from "../shared/spoken.ts";
import { SERVICE_FILE_NAME, SOCKET_DIRECTORY_NAME } from "./protocol.ts";
import {
  ServiceClient,
  DESKTOP_CONTROL_EVENT,
  type DesktopControlSignal,
} from "./client.ts";

/** Resolves the service socket path for this user session. */
export function resolveSocketPath(
  environment: NodeJS.ProcessEnv = process.env,
): string | undefined {
  const configured = environment.SCUFRIS_SERVICE_SOCKET;
  if (configured) return configured;
  const runtimeDirectory = environment.XDG_RUNTIME_DIR;
  if (!runtimeDirectory) return undefined;
  return join(runtimeDirectory, SOCKET_DIRECTORY_NAME, SERVICE_FILE_NAME);
}

/**
 * Connects this agent to the Scufris service.
 *
 * The inversion, from this side: the popup Pi process used to serve the desktop
 * socket and the companion connected to it. Now `scufris-service` owns the
 * conversation and this is one of its clients, in the `agent` role.
 *
 * What travels is what the service cannot know by itself. It reads Pi's event
 * stream for the state and the transcript, so neither is here; what is here is
 * the answer Scufris gives through a tool call, the paragraph it wants spoken,
 * and the widgets it asks for.
 *
 * Only the foreground Scufris connects. A worker Pi has no conversation to
 * report and no screen to ask for, and a second agent would take this one's
 * place on the socket.
 */
export default function service(pi: ExtensionAPI): void {
  if (process.env.SCUFRIS_ROLE !== "orchestrator") return;
  const socketPath = resolveSocketPath();
  let context: ExtensionContext | undefined;
  let client: ServiceClient | undefined;

  const notify = (message: string, level: "info" | "error") => {
    if (context?.hasUI) context.ui.notify(`Scufris service: ${message}`, level);
    else if (level === "error") console.error(`scufris service: ${message}`);
  };

  pi.events.on(SPOKEN_EVENT, (value: unknown) => {
    const signal = value as Partial<SpokenSignal> | undefined;
    if (typeof signal?.said === "string") client?.said(signal.said);
    if (typeof signal?.speak === "string") client?.speak(signal.speak);
  });

  pi.on("session_start", (_event, ctx) => {
    context = ctx;
    if (!socketPath) {
      notify("XDG_RUNTIME_DIR is required to reach the service", "error");
      return;
    }
    if (!client) {
      client = new ServiceClient({
        socketPath,
        // The link comes and goes with the service, which is ordinary and not
        // worth a dialog. Only what a person can act on is raised.
        log: (message, level) => {
          if (level === "error") notify(message, level);
        },
      });
      client.start();
    }
    // Widgets are commanded over this link by the extension that owns them, and
    // this is the only way it reaches one: the link is this extension's, and it
    // lives no longer than the session that started it.
    pi.events.emit(DESKTOP_CONTROL_EVENT, {
      control: client,
    } satisfies DesktopControlSignal);
  });

  pi.on("session_shutdown", () => {
    // Withdrawn before the link closes, so nothing sends a command into a
    // connection that is being taken down under it.
    pi.events.emit(DESKTOP_CONTROL_EVENT, {} satisfies DesktopControlSignal);
    const stopping = client;
    client = undefined;
    context = undefined;
    stopping?.stop();
  });
}
