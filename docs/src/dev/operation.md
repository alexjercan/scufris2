# Operation

## Environment

The launcher and helpers communicate through a small set of variables:

- `SCUFRIS_ROLE`: `orchestrator` in the foreground, `worker` in executions.
  Extensions activate by role.
- `SCUFRIS_PROJECT_ROOTS`: JSON array of directories searched recursively for
  workflow projects. The launcher sets the packaged default when unset.
- `SCUFRIS_CALM`: reserved by the development launcher; Calm itself defaults
  on.
- `SCUFRIS_PIPER_MODEL`, `SCUFRIS_PIPER_CONFIG`: trusted immutable Piper model
  paths. They are bound inside `scufris-speak` by the package, and nothing in
  the agent's process tree sets them.
- `SCUFRIS_DESKTOP_SPEAK_COMMAND`: the synthesiser the companion runs. The
  desktop unit sets it when voice is enabled; a companion without it stays
  silent, which is not a fault.

Nothing here turns speech on. The agent shapes every answer as one prose
paragraph whatever is listening, and whether a sound is made belongs to the
companion, which owns the speaker.

The worker launch wrapper removes `SCUFRIS_ROLE`, `SCUFRIS_CALM`, both Piper
paths, `SCUFRIS_REPORT_CAPABILITY`, and the `PI_*` session variables, then sets
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
- `scripts/scufris-artifacts-prune` removes detail artifact sidecars whose
  owning session file is gone. It touches only sidecars with the exact
  private layout, ownership, and modes, and stops beyond a bounded scan.
- The background service is diagnosed like any user service:

```bash
systemctl --user start scufris-service.service
journalctl --user -u scufris-service.service
```

## Troubleshooting

- Desktop evaluation fails: `desktop.enable` requires `service.enable`,
  because the companion is a client of the service that owns the conversation.
  Voice, the service, and the companion are all Linux-only.
- Piper assertion fails: overrides must keep Piper 1.4.2 and the
  configuration adjacent to the model as `model.onnx.json`.
- Speech produces no audio: the companion is the only thing that makes sound,
  so everything to check is on its side. Confirm
  `programs.scufris.desktop.speech` is enabled so it has a synthesiser, confirm
  the tray does not say "Unmute Scufris", and read its log for Piper or
  PipeWire errors. Speech failures
  never fail the assistant turn.
- Voice input does not work: check `programs.scufris.desktop.transcription`
  and the companion log.
- A job shows `failed: worker execution was lost`: startup reconciliation
  found no live pane for a running record, for example after a reboot. The
  report and conversation survive; steer the job to continue it in a new
  generation.
- A workflow refuses steering with an active cleanup error: a stop or land
  intent is durable. Retry the same cleanup operation; a different one is
  refused until it completes.
