import { basename, join } from "node:path";
import type {
  EntryRenderer,
  ExtensionAPI,
  ExtensionContext,
} from "@earendil-works/pi-coding-agent";
import { Text } from "@earendil-works/pi-tui";
import {
  ATTENTION_STATE_EVENT,
  AssistantStateTracker,
  SPEECH_STATE_EVENT,
  type AttentionStateSignal,
  type SpeechStateSignal,
} from "../shared/assistant-state.ts";
import { SOCKET_DIRECTORY_NAME, SOCKET_FILE_NAME } from "./protocol.ts";
import {
  ACCEPTED_ENTRY,
  DISPATCH_ENTRY,
  ReceiptLedger,
  SessionAcceptance,
  landings,
  messageText,
  type SessionView,
  type TranscriptCommit,
  type TranscriptDispatch,
} from "./acceptance.ts";
import {
  ControlServer,
  MAX_REMEMBERED_SUBMISSIONS,
  SocketBusyError,
  WIDGET_CONTROL_EVENT,
  submissionDigest,
  type ControlHost,
  type WidgetControlSignal,
} from "./server.ts";

const maxSessionNameLength = 128;

/** What the popup calls a prompt the companion spoke. */
export const TRANSCRIPT_LABEL = "spoken through the pill";

/** What the popup calls a prompt whose fate the daemon never learned. */
export const DISPATCH_LABEL = "sent from the pill, outcome unknown";

/**
 * Renders one acceptance commit as the note it is.
 *
 * Without a renderer Pi has nothing to show but this daemon's internal type,
 * which says nothing to the person reading the conversation.
 */
export function commitRenderer(): EntryRenderer<TranscriptCommit> {
  return (entry, _options, theme) => {
    if (entry.data?.version !== 1) return undefined;
    return new Text(theme.fg("muted", TRANSCRIPT_LABEL), 0, 0);
  };
}

/**
 * Renders one dispatch, which is only ever seen when its prompt did not follow.
 *
 * A dispatch whose commit arrived is noise beside it, so it renders nothing;
 * one left on its own is the record of a request whose outcome nobody knows,
 * and saying so is the point of writing it.
 */
export function dispatchRenderer(
  committed: (id: string, digest: string) => boolean,
): EntryRenderer<TranscriptDispatch> {
  return (entry, _options, theme) => {
    const data = entry.data;
    if (data?.version !== 1) return undefined;
    if (committed(data.id, data.digest)) return new Text("", 0, 0);
    return new Text(theme.fg("warning", DISPATCH_LABEL), 0, 0);
  };
}

/** Resolves the daemon control socket path for this user session. */
export function resolveSocketPath(
  environment: NodeJS.ProcessEnv = process.env,
): string | undefined {
  const configured = environment.SCUFRIS_DESKTOP_SOCKET;
  if (configured) return configured;
  const runtimeDirectory = environment.XDG_RUNTIME_DIR;
  if (!runtimeDirectory) return undefined;
  return join(runtimeDirectory, SOCKET_DIRECTORY_NAME, SOCKET_FILE_NAME);
}

/** Returns the bounded session identity the daemon reports to companions. */
export function sessionIdentity(sessionFile: string | undefined): string {
  if (!sessionFile) return "ephemeral";
  return basename(sessionFile)
    .replace(/\.jsonl?$/i, "")
    .slice(0, maxSessionNameLength);
}

/**
 * Serves the desktop control protocol from the popup Scufris daemon.
 *
 * The daemon owns the conversation. scufris-desktop owns activation, audio, and
 * transcription, so this module only accepts finished transcripts and reports
 * what the assistant is doing.
 */
export default function desktop(pi: ExtensionAPI): void {
  if (process.env.SCUFRIS_DAEMON !== "1") return;
  const socketPath = resolveSocketPath();
  const tracker = new AssistantStateTracker();
  let context: ExtensionContext | undefined;
  let server: ControlServer | undefined;

  const ledger = new ReceiptLedger();

  const session: SessionView = {
    branch: () => context?.sessionManager.getBranch() ?? [],
    leaf: () => context?.sessionManager.getLeafId() ?? undefined,
    dispatch: (id, digest) => {
      if (!context) throw new Error("the Scufris session is not ready");
      pi.appendEntry<TranscriptDispatch>(DISPATCH_ENTRY, {
        version: 1,
        id,
        digest,
      });
    },
    send: (id, text, digest) => {
      if (!context) throw new Error("the Scufris session is not ready");
      // An ordinary prompt. The turn it starts is the turn a typed prompt
      // starts: the same input handlers, the same pre-send compaction check,
      // the same per-turn Scufris system prompt, and the same steering into a
      // turn that is already running. Nothing about a spoken request should
      // reach the model under different rules from a typed one.
      //
      // Pi announces the prompt from inside this call, so the announcement
      // arrives in this asynchronous context. That is what identifies the send
      // as this daemon's; the source class Pi reports is `extension` for every
      // extension alike and identifies nothing.
      ledger.send({ version: 1, id, digest }, () =>
        pi.sendUserMessage(text, { deliverAs: "steer" }),
      );
    },
    commit: (submission, entry) => {
      try {
        pi.appendEntry<TranscriptCommit>(ACCEPTED_ENTRY, {
          ...submission,
          entry,
        });
      } catch (error) {
        // The words are in the conversation and the proof of it is not. The
        // submission stays uncertain, which is what the person is told.
        notify(
          `the accepted transcript could not be recorded: ${error instanceof Error ? error.message : String(error)}`,
          "error",
        );
      }
    },
  };
  const acceptance = new SessionAcceptance(session);

  pi.registerEntryRenderer<TranscriptCommit>(ACCEPTED_ENTRY, commitRenderer());
  pi.registerEntryRenderer<TranscriptDispatch>(
    DISPATCH_ENTRY,
    dispatchRenderer((id, digest) =>
      Boolean(landings(session.branch()).get(id)?.has(digest)),
    ),
  );

  // Pi emits this for every prompt, whoever sent it, before expansion and
  // before the decision to run it or steer it into a running turn. Every prompt
  // is recorded, because a prompt this daemon did not send is exactly what
  // makes one it did send ambiguous.
  pi.on("input", (event) => {
    ledger.announce(submissionDigest(event.text));
  });

  const notify = (message: string, level: "info" | "error") => {
    if (context?.hasUI) context.ui.notify(`Scufris desktop: ${message}`, level);
    else if (level === "error") console.error(`scufris desktop: ${message}`);
  };

  const host: ControlHost = {
    session: () =>
      sessionIdentity(context?.sessionManager.getSessionFile() ?? undefined),
    deliver: (id, text, digest, force) =>
      acceptance.deliver(id, text, digest, force),
    accepted: () => acceptance.accepted(MAX_REMEMBERED_SUBMISSIONS),
  };

  const publish = () => server?.broadcast(tracker.report());

  pi.events.on(SPEECH_STATE_EVENT, (value: unknown) => {
    tracker.setSpeaking(
      (value as Partial<SpeechStateSignal>)?.playing === true,
    );
    publish();
  });

  pi.events.on(ATTENTION_STATE_EVENT, (value: unknown) => {
    const signal = value as Partial<AttentionStateSignal> | undefined;
    if (
      signal?.state !== "attention" &&
      signal?.state !== "error" &&
      signal?.state !== "clear"
    ) {
      return;
    }
    tracker.setUnattended({
      state: signal.state,
      detail: typeof signal.detail === "string" ? signal.detail : "",
    });
    publish();
  });

  pi.on("message_end", (event) => {
    // Pi appends this entry only after the extensions have seen it
    // (agent-session.js:363-379), so the prompt has no identifier yet. The
    // commit has to name one, and a commit written before its prompt exists
    // would be exactly the orphan this design refuses to trust. What exists
    // now is the place the prompt is about to take, which is what identifies
    // it once it is there.
    //
    // Every prompt is recorded, whoever sent it: a landing this daemon cannot
    // claim is exactly what makes the place ambiguous for one it can.
    if (event.message.role === "user") {
      acceptance.landed(
        ledger.land(submissionDigest(messageText(event.message.content) ?? "")),
      );
    }
    setImmediate(() => acceptance.notify());
  });

  pi.on("session_start", async (_event, ctx) => {
    context = ctx;
    tracker.reset();
    ledger.clear();
    acceptance.reset();
    if (!socketPath) {
      notify("XDG_RUNTIME_DIR is required for the control socket", "error");
      return;
    }
    if (!server) server = new ControlServer(socketPath, host, notify);
    const serving = server;
    if (!serving.listening) {
      try {
        await serving.start();
      } catch (error) {
        server = undefined;
        notify(
          error instanceof SocketBusyError
            ? error.message
            : `cannot serve the control socket: ${error instanceof Error ? error.message : String(error)}`,
          "error",
        );
        return;
      }
    }
    publish();
    // Widgets are commanded over this socket by the extension that owns them,
    // and this is the only way it reaches one: the socket is this extension's,
    // and a server lives no longer than the session that started it.
    pi.events.emit(WIDGET_CONTROL_EVENT, {
      control: serving,
    } satisfies WidgetControlSignal);
  });

  pi.on("agent_start", () => {
    tracker.setRunning(true);
    publish();
  });

  pi.on("agent_settled", () => {
    tracker.setRunning(false);
    publish();
  });

  pi.on("session_shutdown", async () => {
    tracker.reset();
    ledger.clear();
    acceptance.reset();
    // Withdrawn before the socket closes, so nothing sends a command into a
    // connection that is being taken down under it.
    pi.events.emit(WIDGET_CONTROL_EVENT, {} satisfies WidgetControlSignal);
    const stopping = server;
    server = undefined;
    context = undefined;
    await stopping?.stop();
  });
}
