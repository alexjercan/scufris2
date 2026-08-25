# Architecture

## Layout

Repository ownership follows the runtime architecture:

- `extensions/scufris/` contains the Pi extensions: `workflow/`, `voice/`,
  `dashboard/`, and the small independent `calm.ts`. Extensions own lifecycle
  events, native tools, session state, and notifications.
- `tools/` contains deterministic executables called by extensions:
  `jobs/scufris-jobs`, `jobs/scufris-report`,
  `quick-review-agent/scufris-quick-review-agent`,
  `dashboard/scufris-dashboard`, and `voice/scufris-speak`.
- `scripts/` contains commands called directly by people:
  `scufris-jobs` (inspection CLI), `scufris-artifacts-prune`, and the
  development launcher `scufris-dev`.
- `skills/` contains the distributed model-facing `workflow` and `dashboard`
  skills. Development-only skills live in `.agents/skills/`.
- `nix/` contains one file per build concern: `resources.nix`, `launcher.nix`,
  `popup.nix`, `voice.nix`, `dev-shell.nix`, `docs.nix`, and
  `home-manager.nix`. `nix/scufris.nix` composes them into the component set
  for one system, `nix/checks/` asserts that composition, and `flake.nix`
  only selects the outputs.

Keep orchestration narrow. Extensions route and validate; deterministic
process and filesystem work lives in the small owning helper scripts.

## Roles

One codebase serves two roles selected by `SCUFRIS_ROLE`:

- `orchestrator`: the foreground Scufris session. The launcher sets this. The
  identity, orchestration, response, speech, and dashboard modules activate
  only in this role.
- `worker`: a delegated job execution. The jobs helper sets this when it
  launches a harness inside a tmux pane. Pi workers load only the
  `worker-report.ts` extension, which registers the `scufris_report` tool.
- A standalone Quick Review is not a worker. A private adapter starts one Pi RPC
  child with only the pinned npm extension and read-only built-in tools, then
  relays its terminal outcome to the orchestrator.

Before the harness starts, the launch wrapper removes `SCUFRIS_ROLE`, the
speech and Calm variables, the Piper paths, the report capability, and the
`PI_*` session variables from the inherited environment, then sets
`SCUFRIS_ROLE=worker`, the job ID, the generation, and a fresh report
capability. `SCUFRIS_PROJECT_ROOTS` and `SCUFRIS_VOICE_AVAILABLE` are
inherited unchanged.

## Helper protocol

Extensions never shell out ad hoc. Each capability calls its private helper
through `runPrivateHelper` in `extensions/scufris/shared/runtime.ts`:

- One subprocess per request: `helper COMMAND` with a JSON request on stdin.
- One JSON envelope on stdout: `{"ok": true, "result": ...}` or
  `{"ok": false, "error": "..."}` with an optional `error_code`.
- Output is bounded at 2 MiB and every call has a deadline. Helpers validate
  request fields strictly and reject unknown fields.

The helpers are `tools/jobs/scufris-jobs` (job lifecycle, see
[Jobs](jobs.md)) and `tools/dashboard/scufris-dashboard` (a bounded
`dashboardctl` adapter). `tools/voice/scufris-speak` uses the same subprocess
shape with text on stdin and exit codes instead of JSON.

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
- `<session>.jsonl.scufris/` sidecars beside each Pi session hold private
  response detail artifacts.
- Each active job can hold `quick-review-agent/`, the private artifact and
  completion root for its standalone review.
- `$XDG_STATE_HOME/scufris/dev-sessions/` holds resumable development
  sessions; the popup uses its configured session directory.

## Package composition

`package.json` declares the extension entry points and the skills directory
under the `pi` key, so a checkout also works as a Pi package. Pi APIs are
`peerDependencies`; pinned copies are `devDependencies`.

The flake builds a `resources` derivation that copies `extensions`, `scripts`,
`skills`, and `tools` into the store. The normal variant removes the speech
module and voice tool so they cannot enter the closure. The launcher is a
shell application that:

1. Sets `SCUFRIS_PROJECT_ROOTS` when unset and `SCUFRIS_ROLE=orchestrator`.
2. Exports the Piper model paths for voice variants.
3. Prefers a system `pi` from `PATH` and falls back to the pinned flake Pi.
4. Passes `--extension` and `--skill` flags pointing into the resources.

The Home Manager module renders the same launcher from its options and adds
the optional popup service. The check groups under `nix/checks/` assert the
exact rendered arguments (`launcher.nix`), the distributed files
(`resources.nix`), the module interface (`home.nix`), and closure separation
with a real Piper synthesis fixture (`voice.nix`).
