# Operation

## Environment

The launcher and helpers communicate through a small set of variables:

- `SCUFRIS_ROLE`: `orchestrator` in the foreground, `worker` in executions.
  Extensions activate by role.
- `SCUFRIS_PROJECT_ROOTS`: JSON array of directories searched recursively for
  workflow projects. The launcher sets the packaged default when unset.
- `SCUFRIS_VOICE_AVAILABLE=1`: the speech module may load (voice packages).
- `SCUFRIS_SPEECH=1`: the initial speech mode is on. The service unit sets it
  when voice is enabled, and `dev:voice` sets it too.
- `SCUFRIS_CALM`: reserved by the development launcher; Calm itself defaults
  on.
- `SCUFRIS_PIPER_MODEL`, `SCUFRIS_PIPER_CONFIG`: trusted immutable Piper model
  paths. They are bound inside `scufris-speak` by the package, and nothing in
  the agent's process tree sets them.
- `SCUFRIS_DESKTOP_SPEAK_COMMAND`: the synthesiser the companion runs. The
  desktop unit sets it when voice is enabled; a companion without it stays
  silent, which is not a fault.

The worker launch wrapper removes `SCUFRIS_ROLE`, `SCUFRIS_SPEECH`,
`SCUFRIS_CALM`, both Piper paths, `SCUFRIS_REPORT_CAPABILITY`, and the `PI_*`
session variables, then sets `SCUFRIS_ROLE=worker`, `SCUFRIS_JOB_ID`,
`SCUFRIS_JOB_GENERATION`, and a fresh `SCUFRIS_REPORT_CAPABILITY` for that
execution. `SCUFRIS_PROJECT_ROOTS` and `SCUFRIS_VOICE_AVAILABLE` pass through
unchanged.

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
- Speech produces no audio: confirm a voice-capable package and an enabled
  companion, then `/speech on`. The companion is what makes sound, so check
  its log for Piper or PipeWire errors. Speech failures never fail the
  assistant turn.
- Voice input does not work: speech-to-text is Pi configuration, not
  Scufris.
- A job shows `failed: worker execution was lost`: startup reconciliation
  found no live pane for a running record, for example after a reboot. The
  report and conversation survive; steer the job to continue it in a new
  generation.
- A workflow refuses steering with an active cleanup error: a stop or land
  intent is durable. Retry the same cleanup operation; a different one is
  refused until it completes.
