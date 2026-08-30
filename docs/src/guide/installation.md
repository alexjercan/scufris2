# Installation

## Prerequisite

Install Nix with flakes enabled.

## Run a release

Run the normal package from the current release tag:

```bash
nix run github:alexjercan/scufris2/v0.5.0#scufris
```

There is one launcher and no voice variant of it. Nothing in the agent's
process tree makes sound; hearing Scufris means running the desktop companion,
which owns the speaker.

## Run a checkout

```bash
git clone https://github.com/alexjercan/scufris2.git
cd scufris2
nix run .#scufris
```

## Flake interface

Pin a release tag and share `nixpkgs` with the parent flake:

```nix
{
  inputs.scufris = {
    url = "github:alexjercan/scufris2/v0.5.0";
    inputs.nixpkgs.follows = "nixpkgs";
  };
}
```

Release tags are immutable inputs. Update the tag deliberately.

Packages:

- `default` and `scufris`: the launcher.
- `scufris-service`: the background service that owns the conversation. It
  builds with no graphical dependency, so a machine with no display can run it.
- `scufris-ctl`: the terminal client of that service.
- `scufris-desktop`: the Linux-only companion - the voice pill, the
  conversation window, the widget runtime, and the tray. It is a separate
  output, so nothing else pulls Tauri into its closure.
- `scufris-speak`: the Linux-only synthesiser the companion runs, with the
  voice pinned by the package.
- `resources`: extensions, skills, and deterministic tools. No synthesiser is
  among them, because the agent runs none.
- `docs`: this manual, including the generated option reference.

Resource packages are composition inputs. Most users need only a launcher or
the Home Manager module.

## Home Manager

Import the module from the pinned flake input:

```nix
{inputs, ...}: {
  imports = [inputs.scufris.homeModules.default];

  programs.scufris.enable = true;
}
```

This installs the rendered launcher in `home.packages`. All six extensions are
always present: workflow, response, Calm, service, widgets, and conversation.

The default Pi package comes from the pinned `llm-agents.nix` input. A
configuration that manages Pi itself can pass its own package:

```nix
programs.scufris = {
  enable = true;
  service.agent.piPackage = inputs.llm-agents.packages.${pkgs.system}.pi;
};
```

Set `programs.scufris.service.agent.projectRoots` to control which directories
Scufris searches for workflow projects. The default is `~/personal`, `~/work`,
and `~/third-party`.

The former top-level `piPackage`, `projectRoots`, `finalPackage`, and `voice`
options, plus `service.agentPackage` and `desktop.stt`, remain deprecated aliases
for one release. New configurations should use the architecture-owned paths
shown here.

## The background service

```nix
programs.scufris = {
  enable = true;

  service.enable = true;
};
```

The service owns the conversation and always supervises one agent. There is no
separate agent service to enable. Configure its Pi package, project roots, or
complete launcher under `service.agent`. Enabling the service defines the
`scufris-service.service` user service, wants it from `default.target` rather
than from a graphical session, and installs `scufris-ctl` beside it. A machine
with no display keeps the conversation, and a terminal over ssh reaches it.
See [Background service](../dev/service.md).

`service.sessionDirectory` says where the conversation lives; the default is
`$XDG_DATA_HOME/scufris/sessions`.

## Desktop speech

```nix
programs.scufris = {
  enable = true;

  desktop.speech.enable = true;
};
```

Desktop speech requires Linux. The companion sends bounded text to the shared
`ai-tools-api` speech route and plays its validated WAV response through
PipeWire. The API owns Piper and the pinned voice; Scufris owns playback.

Speech means one thing: the companion gets a synthesiser. Nothing about it
reaches the service or the agent, which shape the same prose answer whatever is
listening. Speech configured for a disabled companion has nowhere for the
paragraph to go, which is not a fault, and silencing Scufris is the tray's
"Mute Scufris" rather than anything in the conversation.

## The desktop companion

```nix
programs.scufris = {
  enable = true;

  service.enable = true;

  desktop = {
    enable = true;
    speech.enable = true;
  };
};
```

The companion requires the service, because the service owns the conversation
it talks to. Enabling it defines the `scufris-desktop.service` user service,
wants it from `graphical-session.target`, and restarts it on failure so a
backend crash never takes the tray down.

The module detects an enabled `services.ai-tools-api` option supplied by the
surrounding Home Manager composition and uses its host and port. If none is
enabled, `desktop.aiToolsApi.manage` defaults true and runs the pinned complete
API package as one fallback service. To consume an API owned outside Home
Manager instead:

```nix
programs.scufris.desktop.aiToolsApi = {
  manage = false;
  baseUrl = "http://127.0.0.1:10300";
};
```

Transcription defaults to `BASE/v1/audio/transcriptions`; speech defaults to
`BASE/v1/audio/speech`. `desktop.transcription.endpoint` remains an explicit
transcription-route override.

The conversation window ships with the companion and needs no configuration;
bind `scufris-ctl hud` in your window manager to reach it. Protocol v4 does not
provide terminal session handoff. `desktop.chatCommand` remains available for
a deployment-specific terminal view that does not take over Pi.

The backend restart hook is generated by the module and restarts only
`scufris-service.service`. Change the activation accelerator with
`programs.scufris.desktop.hotkey`; the default is `Super+D`.

Two more keys answer the pill while it is on screen: one that puts it away and
one that stops Scufris. Both are derived from the hotkey's own modifiers, so
`Super+D` gives `Super+Escape` and `Super+Delete`, and both are yours to name:

```nix
programs.scufris.desktop = {
  hotkey = "Super+D";
  cancelKey = "Super+Escape";
  stopKey = "none";
};
```

`"none"` takes a key off the companion, which is the answer where your desktop
already means something by it. The tray puts the pill away without the cancel
key.

The agenda, macros, and notes panels read the-den journal, and Scufris does not
depend on the repository that holds it. Name the command that reads it, the
journal directory when it is not where that command looks by default, and the
food database if you log food from the macros panel:

```nix
programs.scufris.desktop = {
  todayCommand = inputs.today.packages.${pkgs.system}.default;
  denPath = "/home/you/personal/the-den";
  macrosDatabase = "/home/you/.local/share/nvim/macros.csv";
};
```

A user service does not inherit your login shell, so a `DEN_PATH` you export
there is not one the companion has. None of the three is required: without the
command those three panels open and say what is missing, and without the
database a food is logged only if `today` finds one where it looks by default.

See the [option reference](../reference/options.md) for evaluated types,
defaults, and descriptions.

### Try it without Home Manager

Run the companion package directly to see it before you adopt the module:

```bash
nix run .#scufris-desktop -- --print-config
nix run .#scufris-desktop -- --foreground
```

`--print-config` prints the resolved configuration and exits without opening a
window. `--foreground` runs the companion with pretty logs on stderr instead
of journald, which is the view for watching it work; without the flag, read
the logs with `journalctl --user -t scufris-desktop`. Every value comes from
the environment:

| Variable                          | Default                                          |
| --------------------------------- | ------------------------------------------------ |
| `SCUFRIS_RUNTIME_DIR`             | `$XDG_RUNTIME_DIR/scufris`                       |
| `SCUFRIS_DESKTOP_SOCKET`          | `$SCUFRIS_RUNTIME_DIR/surface.sock`              |
| `SCUFRIS_DESKTOP_COMMAND_SOCKET`  | `$SCUFRIS_RUNTIME_DIR/desktop.sock`              |
| `SCUFRIS_DESKTOP_STATE_FILE`      | `$XDG_STATE_HOME/scufris-desktop/pending.json`   |
| `SCUFRIS_STT_ENDPOINT`            | `http://127.0.0.1:10300/v1/audio/transcriptions` |
| `SCUFRIS_TTS_ENDPOINT`            | `http://127.0.0.1:10300/v1/audio/speech`         |
| `SCUFRIS_DESKTOP_HOTKEY`          | `Super+D`                                        |
| `SCUFRIS_DESKTOP_CANCEL_KEY`      | derived from the hotkey                          |
| `SCUFRIS_DESKTOP_STOP_KEY`        | derived from the hotkey                          |
| `SCUFRIS_DESKTOP_CHAT_COMMAND`    | none                                             |
| `SCUFRIS_DESKTOP_RESTART_COMMAND` | none                                             |
| `SCUFRIS_DESKTOP_SPEAK_COMMAND`   | none, and the companion stays silent             |

`SCUFRIS_RUNTIME_DIR` is the socket directory, used as named with no `scufris`
below it. The service and `scufris-ctl` read the same variable, so one export
moves the whole stack together; a socket named outright still outranks it.
Nothing sets it in an ordinary session. It is what runs a second Scufris
beside this one, which is [staging](../dev/staging.md).

The companion starts without a backend and reports it as unavailable in the
tray. It answers the pill only when `scufris-service` is on the same socket;
see [maintenance](../dev/maintenance.md) to run one from a working tree.

Transcription and speech need `ai-tools-api` on port 10300. A Home Manager
installation manages it by default. For a manual package run, start the pinned
API in another terminal:

```bash
nix run .#ai-tools-api
```

Then start the desktop. `SCUFRIS_STT_ENDPOINT` can name another compatible
transcription route. The packaged `scufris-speak` helper similarly accepts
`SCUFRIS_TTS_ENDPOINT` when a non-default speech route is needed.

The companion is Linux and X11 only.
