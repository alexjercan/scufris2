# Environment variables

[Previous: Maintain and release](../dev/maintenance.md)

Use Home Manager options for deployment. Use these variables for direct runs,
staging, tests, and internal process handoff.

```text
Home Manager -> validated unit environment
shell export -> direct process or staging override
worker wrapper -> private per-execution environment
```

## Runtime paths and agent

| Variable                 | Consumer                                                  | Meaning and default                                                                                            |
| ------------------------ | --------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------- |
| `SCUFRIS_RUNTIME_DIR`    | service, gateway, desktop, agent extension, `scufris-ctl` | Socket directory itself. Default: `$XDG_RUNTIME_DIR/scufris`. One value moves the whole local stack.           |
| `SCUFRIS_AGENT_SOCKET`   | Pi service extension                                      | Exact `agent.sock` override. Default: resolved from `SCUFRIS_RUNTIME_DIR`, then XDG. Expert/test use.          |
| `SCUFRIS_CONTENT_SOCKET` | Pi attachment tool                                        | Exact `content.sock` override. Default: resolved from `SCUFRIS_RUNTIME_DIR`, then XDG. Expert/test use.        |
| `SCUFRIS_ROLE`           | Pi extensions, worker wrapper                             | `orchestrator` for the main agent or `worker` for a delegated Pi. Set by launchers; do not set for normal use. |
| `SCUFRIS_PROJECT_ROOTS`  | launcher, jobs helper                                     | JSON string array searched for Git projects. Packaged default: `["~/personal","~/work","~/third-party"]`.      |
| `SCUFRIS_CALM`           | development/worker environment                            | Reserved launcher value. Calm session state defaults on and is controlled by `/calm`.                          |

## Morning briefings

| Variable                           | Consumer           | Meaning and default                                                                      |
| ---------------------------------- | ------------------ | ---------------------------------------------------------------------------------------- |
| `SCUFRIS_BRIEFING_TIME`            | briefing extension | Local `HH:MM` the unprompted briefing is assembled, or `off`. Launcher default: `08:00`. |
| `SCUFRIS_BRIEFING_PROFILE`         | briefing extension | Which `[briefings.<profile>]` table the schedule asks for. Default: `morning`.           |
| `SCUFRIS_BRIEFING_DEADLINE`        | briefing helper    | Seconds the whole run may take before it publishes with what came back. Default: `1800`. |
| `SCUFRIS_BRIEFING_SOURCE_DEADLINE` | briefing helper    | Seconds one source may take before it is recorded as failed. Default: `900`.             |

A run is written under `$XDG_STATE_HOME/scufris/briefings/<local date>/`. Only
projects declaring the profile in their own `.scufris.toml` contribute, so the
schedule costs nothing until one does.

Socket precedence:

```text
exact socket variable -> SCUFRIS_RUNTIME_DIR/NAME -> XDG_RUNTIME_DIR/scufris/NAME
```

## Background service and gateway

| Variable                       | Consumer          | Meaning and default                                                                                                    |
| ------------------------------ | ----------------- | ---------------------------------------------------------------------------------------------------------------------- |
| `SCUFRIS_SERVICE_AGENT`        | `scufris-service` | Absolute agent launcher. Default: first `scufris` on `PATH`.                                                           |
| `SCUFRIS_SERVICE_SESSION_DIR`  | `scufris-service` | Absolute Pi session directory. Default: `$XDG_DATA_HOME/scufris/sessions`, then `$HOME/.local/share/scufris/sessions`. |
| `SCUFRIS_GATEWAY_LISTEN`       | surface gateway   | Loopback listen address. Default: `127.0.0.1:10440`. CLI `--listen` is equivalent.                                     |
| `SCUFRIS_GATEWAY_TOKEN_FILE`   | surface gateway   | Absolute private token file. Required unless `--token-file` is passed.                                                 |
| `SCUFRIS_GATEWAY_AI_TOOLS_API` | surface gateway   | Loopback inference API base URL. Default: `http://127.0.0.1:10300`. CLI `--ai-tools-api` is equivalent.                |
| `SCUFRIS_GATEWAY_DOCS`         | surface gateway   | Boolean Swagger/OpenAPI switch. Disabled by default; the staging runner sets `1`. Do not enable in production.         |

## Desktop surface

| Variable                          | Default                                                                       | Meaning                                                                                                |
| --------------------------------- | ----------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------ |
| `SCUFRIS_DESKTOP_SOCKET`          | resolved `surface.sock`                                                       | Exact service surface socket override.                                                                 |
| `SCUFRIS_DESKTOP_COMMAND_SOCKET`  | resolved `desktop.sock`                                                       | Exact local window-command socket.                                                                     |
| `SCUFRIS_DESKTOP_STATE_FILE`      | `$XDG_STATE_HOME/scufris-desktop/pending.json`, then `$HOME/.local/state/...` | Pending transcript path. Its directory also holds the stable surface ID.                               |
| `SCUFRIS_DESKTOP_SURFACE_NAME`    | host name                                                                     | Diagnostic registration name. Staging sets the frontend name.                                          |
| `SCUFRIS_DESKTOP_HOTKEY`          | `Super+D`                                                                     | Tap/hold activation key.                                                                               |
| `SCUFRIS_DESKTOP_CANCEL_KEY`      | derived modifiers + `Escape`                                                  | Background/cancel key. `none` disables it.                                                             |
| `SCUFRIS_DESKTOP_STOP_KEY`        | derived modifiers + `Delete`                                                  | Abort key. `none` disables it.                                                                         |
| `SCUFRIS_DESKTOP_CHAT_COMMAND`    | unset                                                                         | Absolute executable for the tray's terminal view. No shell command string.                             |
| `SCUFRIS_DESKTOP_RESTART_COMMAND` | unset                                                                         | Absolute executable for backend restart. Home Manager generates a safe service-specific command.       |
| `SCUFRIS_DESKTOP_SPEAK_COMMAND`   | unset                                                                         | Absolute executable that reads one paragraph on stdin and owns synthesis/playback. Unset means silent. |
| `SCUFRIS_WIDGET_PATH`             | unset                                                                         | Colon-separated roots of external compiled desktop widgets.                                            |
| `DEN_PATH`                        | `~/personal/the-den`                                                          | Journal directory read by the den backend and by `scufris-den`.                                        |
| `MACROS_DATABASE`                 | `$DEN_PATH/Foods.csv`, else `~/.local/share/nvim/macros.csv`                  | Food database used by the macros widget and by `scufris-den`.                                          |
| `EXERCISES_DATABASE`              | `$DEN_PATH/Exercises.csv`                                                     | `split,exercise` rows the macros widget offers under the exercise field.                               |

All three desktop hook variables must name absolute executables. They are not
shell snippets.

## Speech and transcription

| Variable               | Consumer        | Default                                                                             |
| ---------------------- | --------------- | ----------------------------------------------------------------------------------- |
| `SCUFRIS_STT_ENDPOINT` | desktop         | `http://127.0.0.1:10300/v1/audio/transcriptions`                                    |
| `SCUFRIS_STT_MODEL`    | desktop         | `whisper-1`                                                                         |
| `SCUFRIS_STT_LANGUAGE` | desktop         | `auto`                                                                              |
| `SCUFRIS_TTS_ENDPOINT` | `scufris-speak` | Required by the raw helper; the packaged wrapper supplies its configured API route. |
| `SCUFRIS_TTS_MODEL`    | `scufris-speak` | `piper-1`                                                                           |
| `SCUFRIS_TTS_VOICE`    | `scufris-speak` | `en_US-lessac-medium`                                                               |

The Home Manager module derives both endpoint routes from
`desktop.aiToolsApi.baseUrl`. Endpoint variables are mainly for direct runs and
staging.

## Staging inputs

These may be set before `scufris-staging` starts.

| Variable                               | Default                                | Meaning                                                                        |
| -------------------------------------- | -------------------------------------- | ------------------------------------------------------------------------------ |
| `SCUFRIS_STAGING_ROOT`                 | `/tmp/scufris-staging`                 | Disposable state, data, project, and lock root.                                |
| `SCUFRIS_STAGING_SERVICE`              | source build or packaged wrapper value | Service executable.                                                            |
| `SCUFRIS_STAGING_DESKTOP`              | source build or packaged wrapper value | Desktop executable.                                                            |
| `SCUFRIS_STAGING_GATEWAY`              | source build or packaged wrapper value | Gateway executable.                                                            |
| `SCUFRIS_STAGING_SPEAK`                | packaged `scufris-speak` or unset      | Speech helper for each frontend. `SCUFRIS_DESKTOP_SPEAK_COMMAND` has priority. |
| `SCUFRIS_STAGING_AI_TOOLS_API`         | `external`                             | `external` or `managed`.                                                       |
| `SCUFRIS_STAGING_AI_TOOLS_API_PACKAGE` | packaged pinned API or unset           | Executable used by `managed` mode.                                             |
| `SCUFRIS_STAGING_GATEWAY_PORT`         | `10441`                                | Loopback remote-surface port.                                                  |
| `SCUFRIS_STAGING_EXTERNAL_SURFACES`    | `auto`                                 | `auto`, `tailscale`, or `local`.                                               |

Staging also accepts the normal `SCUFRIS_DESKTOP_HOTKEY`, STT endpoint, TTS
endpoint, and speech command overrides.

## Values created by staging

Do not normally set these yourself.

| Variable                          | Meaning                                                |
| --------------------------------- | ------------------------------------------------------ |
| `SCUFRIS_STAGING_FRONTEND`        | Current named frontend profile.                        |
| `SCUFRIS_DESKTOP_SURFACE_NAME`    | Same profile, used in surface registration.            |
| `SCUFRIS_RUNTIME_DIR`             | `$XDG_RUNTIME_DIR/scufris-staging`.                    |
| `SCUFRIS_PROJECT_ROOTS`           | JSON array containing the seeded staging project root. |
| `SCUFRIS_SERVICE_AGENT`           | Working-tree `scripts/scufris-agent`.                  |
| `PI_CODING_AGENT_DIR`             | Isolated staging Pi settings and shared auth location. |
| `XDG_STATE_HOME`, `XDG_DATA_HOME` | Staging backend or named frontend directories.         |
| `SCUFRIS_DESKTOP_COMMAND_SOCKET`  | Per-frontend local command socket.                     |

## Worker-private variables

The jobs helper creates and rotates these. Never persist or share them.

| Variable                    | Meaning                                                   |
| --------------------------- | --------------------------------------------------------- |
| `SCUFRIS_JOB_ID`            | Durable logical job ID.                                   |
| `SCUFRIS_JOB_GENERATION`    | Current execution generation.                             |
| `SCUFRIS_REPORT_CAPABILITY` | Secret capability for authenticated worker reports.       |
| `SCUFRIS_HELPER_READY_LINE` | Internal extension/helper startup synchronization marker. |

Before a worker starts, the wrapper removes inherited role, Calm, old report
capability, retired speech/Piper values, and `PI_SESSION_*` values. It then sets
a fresh worker role, ID, generation, and capability. Project roots remain
inherited.

## Development and diagnostic overrides

| Variable                                           | Meaning                                                                                                                                                 |
| -------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `SCUFRIS_QUICK_REVIEW_PI`                          | Pi executable used by the standalone Quick Review adapter. Default: Pi from `PI_PACKAGE_DIR`, then `pi` on `PATH`.                                      |
| `RUST_LOG`                                         | Rust tracing filter, for example `scufris_service=debug` or `scufris_desktop=debug`. Debug payloads may contain conversation text and widget arguments. |
| `PI_CODING_AGENT_DIR`                              | Pi configuration/auth directory. Staging isolates settings and links auth.                                                                              |
| `PI_PACKAGE_DIR`                                   | Pi package root used to locate its CLI for Quick Review.                                                                                                |
| `PI_SESSION_ID`, `PI_SESSION_FILE`, `PI_SESSION_*` | Pi-owned session values. Worker launch removes inherited copies.                                                                                        |

`PI_STT`, `PI_STT_CONFIG`, `PI_MODEL`, `PI_PROVIDER`, and
`PI_REASONING_LEVEL` belong to Pi, not Scufris. Configure them through Pi's own
interface.

## Standard environment used by Scufris

| Variable          | Use                                                                    |
| ----------------- | ---------------------------------------------------------------------- |
| `HOME`            | Working directory and fallback data/state roots.                       |
| `PATH`            | Finds `pi`, `scufris`, and direct-run helper commands.                 |
| `XDG_RUNTIME_DIR` | Default socket parent; required when `SCUFRIS_RUNTIME_DIR` is unset.   |
| `XDG_DATA_HOME`   | Session data and staging data.                                         |
| `XDG_STATE_HOME`  | Jobs and desktop pending state.                                        |
| `TMPDIR`          | Test sockets and temporary files. Keep it short for Unix socket tests. |

## Test-only variables

These are fixtures, not supported runtime configuration:

| Variable                         | Test owner                     |
| -------------------------------- | ------------------------------ |
| `SCUFRIS_FIXTURE_WAV`            | Nix speech fixture             |
| `SCUFRIS_STAGING_REPORT`         | Staging test stub              |
| `SCUFRIS_TEST_AGENT_PACKAGE`     | Home Manager evaluation check  |
| `PI_MUTATION`, `PI_REVIEW_TOOLS` | Worker/reviewer test harnesses |

## Retired names

`SCUFRIS_DAEMON`, `SCUFRIS_SPEECH`, `SCUFRIS_VOICE_AVAILABLE`, and the
`SCUFRIS_PIPER_*` family are not runtime interfaces. Old changelog entries and
negative closure checks may still mention them.

---

Next: [Home Manager options](options.md)
