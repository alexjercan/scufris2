# Architecture

## Layout

Repository ownership follows the runtime architecture:

- `agent/extensions/scufris/` contains the Pi extensions: `workflow/`,
  `service/`, `response.ts`, and `calm.ts`. Extensions own lifecycle events,
  native tools, session state, and notifications. Extension-local shared code
  stays under `agent/extensions/scufris/shared/`.
- `agent/skills/` contains the distributed model-facing `workflow` skill.
  Development-only skills live in `.agents/skills/`.
- `host/service/` contains the authoritative headless service and its
  `scufris-ctl` client. It builds with no graphical dependency.
- `surfaces/desktop/` contains the Linux Tauri companion, including the pill,
  conversation window, widget runtime, tray, widget sources under `widgets/`,
  and widget backends under `backends/`. Widgets and backends are compiled into
  the companion by its `build.rs`.
- `shared/control/` contains the `scufris-control` crate and protocol encoding
  shared by the service and desktop surface.
- Root `Cargo.toml` and `Cargo.lock` define the workspace across `host/`,
  `shared/`, and `surfaces/`. See [Background service](service.md),
  [Desktop companion](desktop.md), and [Widgets](widgets.md).
- `tools/` contains deterministic executables called by extensions:
  `jobs/scufris-jobs`, `jobs/scufris-report`,
  `quick-review-agent/scufris-quick-review-agent`, and `voice/scufris-speak`.
- `scripts/` contains commands called directly by people:
  `scufris-jobs` (inspection CLI) and the development launcher `scufris-dev`.
- `nix/` contains one file per build concern: `resources.nix`, `launcher.nix`,
  `speak.nix`, `desktop.nix`, `service.nix`, `staging.nix`, `dev-shell.nix`,
  `docs.nix`, and `home-manager.nix`. `nix/scufris.nix`
  composes them into the component set for one system, `nix/checks/` asserts
  that composition, and `flake.nix` only selects the outputs.

Keep orchestration narrow. Extensions route and validate; deterministic
process and filesystem work lives in the small owning helper scripts.

## Roles

One codebase serves two roles selected by `SCUFRIS_ROLE`:

- `orchestrator`: the foreground Scufris session. The launcher sets this. The
  identity, orchestration, and response modules activate only in this role.
- `worker`: a delegated job execution. The jobs helper sets this when it
  launches a harness inside a tmux pane. Pi workers load only the
  `worker-report.ts` extension, which registers the `scufris_report` tool.
- A standalone Quick Review is not a worker. A private adapter starts one Pi RPC
  child with only the pinned npm extension and read-only built-in tools, then
  relays its terminal outcome to the orchestrator.

Before the harness starts, the launch wrapper removes `SCUFRIS_ROLE`, the
Calm variable, the report capability, and the `PI_*` session
variables from the inherited environment, then sets `SCUFRIS_ROLE=worker`, the
job ID, the generation, and a fresh report capability. `SCUFRIS_PROJECT_ROOTS`
is inherited unchanged.

## Helper protocol

Extensions never shell out ad hoc. Each capability calls its private helper
through `runPrivateHelper` in `agent/extensions/scufris/shared/runtime.ts`:

- One subprocess per request: `helper COMMAND` with a JSON request on stdin.
- One JSON envelope on stdout: `{"ok": true, "result": ...}` or
  `{"ok": false, "error": "..."}` with an optional `error_code`.
- Output is bounded at 2 MiB and every call has a deadline. Helpers validate
  request fields strictly and reject unknown fields.

The helper is `tools/jobs/scufris-jobs` (job lifecycle, see [Jobs](jobs.md)).
`tools/voice/scufris-speak` uses the same subprocess shape with text on stdin
and exit codes instead of JSON.

## Trust model

- The model never receives unrestricted commands, paths, or URLs from Scufris
  tools. Native tool schemas are narrow and validated again in the helpers.
- Worker reporting is capability-based. Each execution generation gets fresh
  random capabilities; only their SHA-256 hashes are stored. Workers can
  report only `working`, `blocked`, and `done`. Only trusted orchestration
  can publish `failed`. See [Jobs](jobs.md).
- Job artifacts are opened by descriptor with `O_NOFOLLOW`, must be regular
  files, and every read is bounded. Symlinked paths fail closed.
- Repository content, worker reports, and review handoffs are treated as
  untrusted data in prompts, never as instructions. The pinned Quick Review npm
  extension is trusted code with the full permissions of its separate Pi
  process.
- Independent reviewers receive an enforced harness-specific built-in tool
  allowlist. Pi gets only read tools plus authenticated reporting. Claude gets
  only Read, Glob, and Grep; the trusted wrapper records its final response.
  This is not an OS filesystem sandbox. The harness executable is trusted, and
  Claude managed policy is also trusted because policy hooks and plugin hooks
  execute outside the built-in tool list. Review metadata states these
  boundaries explicitly.

## Data at rest

- `$XDG_STATE_HOME/scufris/jobs/<job-id>/` holds each durable job: record,
  prompt, report, status, conversation, authorization, and the harness
  session transcript. Cleanup archives finished workflows into
  `jobs/_archive/` instead of deleting them.
- Each active job can hold `quick-review-agent/`, the private artifact and
  completion root for its standalone review.
- `$XDG_STATE_HOME/scufris/dev-sessions/` holds resumable development
  sessions; the service uses its configured session directory.
- `$XDG_RUNTIME_DIR/scufris/` holds mode-0600 `surface.sock`, `agent.sock`,
  and `control.sock`. `scufris-service` binds all three. The companion keeps
  `desktop.sock` beside them for window manager bindings; it is a separate
  surface-local protocol.
- `$XDG_STATE_HOME/scufris-desktop/surface-id` holds the desktop's private,
  stable registered surface ID.

## Package composition

`package.json` declares the extension entry points and the skills directory
under the `pi` key, so a checkout also works as a Pi package. Pi APIs are
`peerDependencies`; pinned copies are `devDependencies`.

The flake builds one `resources` derivation that copies `agent/extensions`,
`scripts`, `agent/skills`, and `tools` into their compatible installed paths,
then removes the development
launcher and `tools/voice`. Nothing in the agent's process tree makes sound, so
the synthesiser is not among what the agent is handed; `scufris-speak` takes it
from the source tree instead. The launcher is a shell application that:

1. Sets `SCUFRIS_PROJECT_ROOTS` when unset and `SCUFRIS_ROLE=orchestrator`.
2. Prefers a system `pi` from `PATH` and falls back to the pinned flake Pi.
3. Passes `--extension` and `--skill` flags pointing into the resources.

There is one launcher and it has no voice variant. No synthesiser and no
player enter its closure, and nothing it sets turns speech on: the agent
shapes the answer, and the companion owns the speaker.

`nix/speak.nix` builds `scufris-speak`, the bounded HTTP-to-PipeWire adapter the
companion runs. It sends the configured speech model and voice to
`ai-tools-api`; no Piper executable or model enters a Scufris closure.

The Home Manager module renders the top-level agent launcher, which works
interactively and is also the executable the optional background service runs.
API process ownership is explicit and top-level: `aiToolsApi.enable` runs the
pinned complete API package, while false leaves ownership to an enabled
`services.ai-tools-api` provider or another external deployment. The desktop
only consumes its configured base URL. The check groups under `nix/checks/`
assert the rendered launcher,
distributed files, module interface, API/closure separation, resolved companion
configuration, and headless service.

`scufris-surface-gateway` is an optional process in the headless package. It
accepts bearer-authenticated WebSockets on a loopback TCP port and bridges only
strict surface-channel messages to `surface.sock`. Tailscale Serve terminates
TLS and supplies the private network boundary. The gateway has no path to
`agent.sock` or `control.sock`.

`scufris-desktop` is built from the `surfaces/desktop/` workspace member by
`nix/desktop.nix` as a separate package output. It is absent from the launcher
closure, which the desktop closure check enforces.
`scufris-service` and `scufris-ctl` come from the same workspace by
`nix/service.nix`, and the service closure check enforces that neither pulls in
GTK or WebKitGTK.
