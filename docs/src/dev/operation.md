# Operate it

[Previous: Tmux](tmux.md)

```text
observe -> scufris-ctl state + journals + scufris-jobs
isolate -> SCUFRIS_RUNTIME_DIR + XDG state/data roots
recover -> service restart + durable sessions/jobs
```

## Environment

The launcher and helpers communicate through a small set of variables:

- `SCUFRIS_ROLE`: `orchestrator` in the foreground, `worker` in executions.
  Extensions activate by role.
- `SCUFRIS_PROJECT_ROOTS`: JSON array of directories searched recursively for
  workflow projects. The launcher sets the packaged default when unset.
- `SCUFRIS_CALM`: reserved by the development launcher; Calm itself defaults
  on.
- `SCUFRIS_STT_ENDPOINT`, `SCUFRIS_STT_MODEL`, and
  `SCUFRIS_STT_LANGUAGE`: the OpenAI-compatible transcription request.
- `SCUFRIS_TTS_ENDPOINT`, `SCUFRIS_TTS_MODEL`, and `SCUFRIS_TTS_VOICE`: the
  speech request used by `scufris-speak`.
- `SCUFRIS_DESKTOP_SPEAK_COMMAND`: the HTTP/playback helper the companion runs. The
  desktop unit sets it when voice is enabled; a companion without it stays
  silent, which is not a fault.

Nothing here turns speech on. The agent shapes every answer as one prose
paragraph whatever is listening, and whether a sound is made belongs to the
companion, which owns the speaker.

The worker launch wrapper removes `SCUFRIS_ROLE`, `SCUFRIS_CALM`,
`SCUFRIS_REPORT_CAPABILITY`, and the `PI_*` session variables, then sets
`SCUFRIS_ROLE=worker`, `SCUFRIS_JOB_ID`, `SCUFRIS_JOB_GENERATION`, and a fresh
`SCUFRIS_REPORT_CAPABILITY` for that execution. `SCUFRIS_PROJECT_ROOTS` passes
through unchanged.

## State locations

- `$XDG_STATE_HOME/scufris/jobs/`: active job directories, `jobs.lock`, and
  `jobs/_archive/` with archived workflows.
- `$XDG_STATE_HOME/scufris/dev-sessions/`: resumable `npm run dev` sessions.
- `~/.local/share/scufris/sessions` (default): the conversation the background
  service owns.

## Inspecting jobs

`scripts/scufris-jobs` inspects stored jobs read-only through the helper:

```bash
scripts/scufris-jobs all              # table of active jobs
scripts/scufris-jobs all --archived   # include archived workflows
scripts/scufris-jobs <id-prefix>      # one job in full detail
scripts/scufris-jobs all --json       # structured output
```

The table shows job ID, latest state, worker-pane liveness, project,
workspace, harness/model, and the latest summary in fixed columns, identical
in a terminal and a pipe. Control characters in job content are escaped, so
job output cannot emit terminal control sequences. A unique ID prefix
resolves archived jobs too; missing and ambiguous prefixes fail without
selecting a job. Full detail includes events, the report, the pinned project
context, and the prompt.

## Holding the conversation in a terminal

The background service holds the agent by default. A terminal can take it,
keep it for as long as it runs, and give it back on exit. Nothing is offered
until the service is started with `SCUFRIS_SERVICE_TERMINAL_LEASE=1`
(`terminalLease = true` in Home Manager); without it every request is refused
and the terminal runs as an ordinary Pi.

```bash
scufris-ctl state          # holder, generation, sessions, lineage file
scufris-terminal           # take the agent in this directory
```

`scufris-terminal` asks the service where the sessions are and which file the
next holder continues from, then starts Pi on a fork of that file. Inside a
checkout that carries `.pi/extensions/scufris-terminal`, plain `pi` with
`SCUFRIS_TERMINAL=1` also joins, on a session of its own and with catch-up
rather than the model context.

From inside the terminal, everything is on `/scufris`:

| Command            | Does                                                   |
| ------------------ | ------------------------------------------------------ |
| `/scufris status`  | holder, generation, lineage file, owner, channel       |
| `/scufris release` | give the agent back and keep running independently     |
| `/scufris attach`  | take the agent, forking the lineage file when possible |
| `/scufris hold`    | stop retrying after a loss                             |

On exit the service starts its own agent again from the terminal's session, so
the conversation continues where the terminal left it. A terminal that dies
without releasing is noticed after three missed heartbeats, about fifteen
seconds, and the service recovers the same way.

Each handoff copies the session branch into a new file. Remove the copies
nothing continues from:

```bash
scufris-ctl lineage prune --keep 30 --dry-run
scufris-ctl lineage prune --keep 30
```

Only forked files older than the window are removed, never the file the next
holder would start from.

### Turning it on and off

```nix
programs.scufris.service.terminalLease = true;
```

The option puts `SCUFRIS_SERVICE_TERMINAL_LEASE=1` on the service unit and
installs `scufris-terminal`. Activate the generation and restart the service:

```bash
systemctl --user restart scufris-service.service
scufris-ctl state          # holder: managed
```

Setting it back to `false` is the rollback, and it needs nothing else. A
terminal holding the agent at that moment loses it when the service restarts:
the lease ends with the process that granted it, the service starts its own
agent from the terminal's own session file, and the terminal keeps running as
an ordinary Pi with a notice. No session is lost, because every handoff writes
into the session directory the service reads.

Rolling the whole deployment back to the previous Home Manager generation is
also safe. An older service does not know the lease verbs and refuses them,
which is what a terminal treats as "the lease is not offered". Sessions that
a fork created stay readable: a fork is an ordinary session file with one
extra header field.

The first foreground `session_start` after the change adopts the jobs the
previous owner held, so no migration step is needed. Check that nothing was
left behind:

```bash
echo '{}' | tools/jobs/scufris-jobs orphans
```

An empty list is the expected answer. A row means a worker pane is owned by a
Pi that is not here; [Jobs](jobs.md) says what to do with it.

## Housekeeping

- Finished workflows are archived, not deleted. Remove
  `$XDG_STATE_HOME/scufris/jobs/_archive/<id>` manually when history is no
  longer needed.
- The background service is diagnosed like any user service:

```bash
systemctl --user start scufris-service.service
journalctl --user -u scufris-service.service
```

When remote surfaces are enabled, Home Manager owns the complete private WSS
path as two user services. The Serve unit reconciles the background Tailscale
state on start and removes exactly the production `/` route on stop:

```bash
systemctl --user status scufris-surface-gateway.service
systemctl --user status scufris-tailscale-serve.service
journalctl --user -u scufris-tailscale-serve.service
tailscale serve status
```

The production service owns `/`. Staging owns only `/scufris-staging`, so either
stack can stop without resetting the other's route.

### Logs

Service and desktop logs use structured `tracing` fields. INFO records major
lifecycle events: listener startup, surface names connecting and disconnecting,
the Pi agent connection, desktop identity, service readiness, and shutdown.
DEBUG adds connection IDs, full protocol payloads, replay and recipient counts,
message IDs, widget registrations, retry details, and speech HTTP
request/response metadata. Transcription audio, transcription text, attachment
bytes, bearer tokens, and speech input text are not logged.
DEBUG protocol payloads can contain conversation text and widget arguments, so
enable them only while diagnosing a trusted local run.

For split staging, set the filter on each command whose side is needed:

```bash
RUST_LOG=debug nix run .#staging -- backend
RUST_LOG=debug nix run .#staging -- frontend one
```

The deployed units default to INFO. Read their journals with:

```bash
journalctl --user -u scufris-service.service -f
journalctl --user -t scufris-desktop -f
```

The normal journal view prints the human message. Use `-o verbose` to include
all structured fields such as `F_NAME`, `F_SURFACE`, `F_CONNECTION`, and
`F_PAYLOAD`.

`RUST_LOG=scufris_service=debug` limits verbose output to the backend crate;
`RUST_LOG=scufris_desktop=debug` does the same for the frontend crate.

## Troubleshooting

- Desktop evaluation fails: `desktop.enable` requires `service.enable`,
  because the companion is a client of the service that owns the conversation.
  Voice, the service, and the companion are all Linux-only.
- Speech inference is unreachable: confirm either the enabled
  `services.ai-tools-api` provider or the `scufris-ai-tools-api` fallback is
  active and listening on the configured base URL.
- Speech produces no audio: confirm `programs.scufris.desktop.speech` is
  enabled, the tray does not say "Unmute Scufris", and read the companion log
  for API or PipeWire errors. Speech failures never fail the assistant turn.
- Voice input does not work: check the ai-tools-api transcription route and the
  companion log.
- The remote gateway is active but WSS is unavailable: inspect
  `scufris-tailscale-serve.service`. The login user must be connected to the
  tailnet and allowed to run `tailscale serve`.
- A job shows `failed: worker execution was lost`: startup reconciliation
  found no live pane for a running record, for example after a reboot. The
  report and conversation survive; steer the job to continue it in a new
  generation.
- A workflow refuses steering with an active cleanup error: a stop or land
  intent is durable. Retry the same cleanup operation; a different one is
  refused until it completes.

For the complete list, including staging and internal handoff values, see
[Environment variables](../reference/environment.md).

---

Next: [Run staging](staging.md)
