# Operation

## Environment

The launcher and helpers communicate through a small set of variables:

- `SCUFRIS_ROLE`: `orchestrator` in the foreground, `worker` in executions.
  Extensions activate by role.
- `SCUFRIS_PROJECT_ROOTS`: JSON array of directories searched recursively for
  workflow projects. The launcher sets the packaged default when unset.
- `SCUFRIS_CALM`: reserved by the development launcher; Calm itself defaults
  on.
- `SCUFRIS_STT_ENDPOINT`: the OpenAI-compatible transcription route.
- `SCUFRIS_TTS_ENDPOINT`: the speech route used by `scufris-speak`.
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
- `<session>.jsonl.scufris/`: private detail artifact sidecars beside each Pi
  session file.

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

## Housekeeping

- Finished workflows are archived, not deleted. Remove
  `$XDG_STATE_HOME/scufris/jobs/_archive/<id>` manually when history is no
  longer needed.
- The background service is diagnosed like any user service:

```bash
systemctl --user start scufris-service.service
journalctl --user -u scufris-service.service
```

### Logs

Service and desktop logs use structured `tracing` fields. INFO records major
lifecycle events: listener startup, surface names connecting and disconnecting,
the Pi agent connection, desktop identity, service readiness, and shutdown.
DEBUG adds connection IDs, full protocol payloads, replay and recipient counts,
message IDs, widget registrations, retry details, and speech HTTP
request/response metadata. Transcription audio, transcription text, and speech
input text are not logged.
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
- A job shows `failed: worker execution was lost`: startup reconciliation
  found no live pane for a running record, for example after a reboot. The
  report and conversation survive; steer the job to continue it in a new
  generation.
- A workflow refuses steering with an active cleanup error: a stop or land
  intent is durable. Retry the same cleanup operation; a different one is
  refused until it completes.
