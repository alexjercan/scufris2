/** This repository's Pi, offered the Scufris agent.
 *
 * Off unless `SCUFRIS_TERMINAL=1` is set: an ordinary `pi` here loads this
 * file and gets nothing from it. With the variable, this Pi takes the lease
 * from the background service, which stops its managed `pi --mode rpc` child,
 * and joins the agent channel in its place. Typed turns and final answers then
 * reach the desktop and the phone, surface messages and wakes reach this
 * terminal, and the job and briefing tools are the same ones. On exit the
 * lease is released and the service starts its own agent again.
 *
 * The service has to be started with `--terminal-lease` or
 * `SCUFRIS_SERVICE_TERMINAL_LEASE=1`. `scufris-terminal` is the launcher that
 * sets the variable and continues the conversation's session lineage; see
 * "Take the agent into a terminal" in docs/src/dev/operation.md.
 *
 * `SCUFRIS_TERMINAL_LOG` names a file that gets one JSON line per lifecycle
 * event, which is how an unexplained handoff is read afterwards rather than
 * guessed at.
 *
 * This file is the gate and the composition. Everything it wires lives under
 * `agent/extensions/scufris/`, so it is packaged with the rest.
 */
import { resolve } from "node:path";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import workflow from "../../../agent/extensions/scufris/workflow/index.ts";
import briefings from "../../../agent/extensions/scufris/briefing/index.ts";
import response from "../../../agent/extensions/scufris/response.ts";
import calm from "../../../agent/extensions/scufris/calm.ts";
import {
  bindTerminal,
  fileTrace,
  terminalEnabled,
  TERMINAL_LOG_VARIABLE,
} from "../../../agent/extensions/scufris/terminal/index.ts";
import {
  LeaseClient,
  resolveControlSocketPath,
} from "../../../agent/extensions/scufris/service/lease.ts";

const repository = resolve(new URL("../../..", import.meta.url).pathname);

export default function scufrisTerminal(pi: ExtensionAPI): void {
  if (!terminalEnabled()) return;
  process.env.SCUFRIS_ROLE = "orchestrator";
  process.env.SCUFRIS_PROJECT_ROOTS ??=
    '["~/personal","~/work","~/third-party"]';
  const trace = fileTrace(process.env[TERMINAL_LOG_VARIABLE]);
  // Order matters: the response extension must see `agent_settled` before the
  // service binding clears the turn the answer belongs to.
  workflow(pi);
  briefings(pi);
  response(pi, { terminal: true });
  calm(pi);
  const controlSocket = resolveControlSocketPath();
  if (!controlSocket) {
    trace("lease_refused", { error: "no control socket path" });
    pi.on("session_start", (_event, context) => {
      context.ui.notify(
        "Scufris: XDG_RUNTIME_DIR is required to reach control.sock",
        "error",
      );
    });
    return;
  }
  bindTerminal(pi, { lease: new LeaseClient(controlSocket), trace });
  pi.on("resources_discover", () => ({
    skillPaths: [
      resolve(repository, "agent/skills/workflow"),
      resolve(repository, "agent/skills/den"),
    ],
  }));
}
