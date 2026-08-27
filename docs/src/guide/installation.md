# Installation

## Prerequisite

Install Nix with flakes enabled.

## Run a release

Run the normal package from the current release tag:

```bash
nix run github:alexjercan/scufris2/v0.3.0#scufris
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
    url = "github:alexjercan/scufris2/v0.3.0";
    inputs.nixpkgs.follows = "nixpkgs";
  };
}
```

Release tags are immutable inputs. Update the tag deliberately.

Packages:

- `default` and `scufris`: the launcher.
- `scufris-desktop`: the Linux-only voice pill and tray companion. It is a
  separate output, so nothing else pulls Tauri into its closure.
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

This installs the rendered launcher in `home.packages`. The workflow,
response, Calm, and desktop extensions are always present.

The default Pi package comes from the pinned `llm-agents.nix` input. A
configuration that manages Pi itself can pass its own package:

```nix
programs.scufris = {
  enable = true;
  piPackage = inputs.llm-agents.packages.${pkgs.system}.pi;
};
```

Set `programs.scufris.projectRoots` to control which directories Scufris
searches for workflow projects. The default is `~/personal`, `~/work`, and
`~/third-party`.

## The background service

```nix
programs.scufris = {
  enable = true;

  service.enable = true;
};
```

The service owns the conversation. Enabling it defines the
`scufris-service.service` user service, wants it from `default.target` rather
than from a graphical session, and installs `scufris-ctl` beside it. A machine
with no display keeps the conversation, and a terminal over ssh reaches it.
See [Background service](../dev/service.md).

`service.sessionDirectory` says where the conversation lives; the default is
`$XDG_DATA_HOME/scufris/sessions`.

## Voice

```nix
programs.scufris = {
  enable = true;

  voice.enable = true;
};
```

Voice requires Linux. It selects a private patched Piper 1.4.2 package, the
pinned `en_US-lessac-medium` model, and PipeWire playback. Overrides must keep
Piper version 1.4.2 and an immutable store model with its configuration
adjacent as `model.onnx.json`.

Voice means one thing: the companion gets a synthesiser. Nothing about it
reaches the service or the agent, which shape the same prose answer whatever is
listening. A deployment with voice and no companion has nowhere for the
paragraph to go, which is not a fault, and silencing Scufris is the tray's
"Mute Scufris" rather than anything in the conversation.

## The desktop companion

```nix
programs.scufris = {
  enable = true;

  voice.enable = true;
  service.enable = true;

  desktop.enable = true;
};
```

The companion requires the service, because the service owns the conversation
it talks to. Enabling it defines the `scufris-desktop.service` user service,
wants it from `graphical-session.target`, and restarts it on failure so a
backend crash never takes the tray down.

Transcription needs an endpoint. With none configured the module also runs a
bundled loopback `whisper-server` on `127.0.0.1:10302` with a pinned model, so
voice input works on any Nix system. Point it at an existing server instead:

```nix
programs.scufris.desktop.stt.endpoint = "http://127.0.0.1:10301/inference";
```

A configured endpoint turns the bundled server off, because
`stt.whisper.enable` defaults to `stt.endpoint == null`. Reuse the server you
already run rather than a second copy of the same model. Setting both
`stt.endpoint` and `stt.whisper.enable` is an error.

The conversation window ships with the companion and needs no configuration;
bind `scufris-ctl hud` in your window manager to reach it. The terminal
session is the deeper tool and is not a fallback for it, and Scufris ships no
window manager, so the tray cannot open a terminal by itself. Give it the
command your desktop session already uses:

```nix
programs.scufris.desktop.chatCommand = pkgs.writeShellScriptBin "scufris-chat" ''
  exec kitty --class Scufris scufris-ctl debug
'';
```

`scufris-ctl debug` takes the agent from the service and opens its session in
the terminal that asked, and gives it back when that terminal closes. See
[Background service](../dev/service.md).

The backend restart hook is generated by the module and restarts only
`scufris-service.service`. Change the activation accelerator with
`programs.scufris.desktop.hotkey`; the default is `Super+D`.

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

| Variable                          | Default                                        |
| --------------------------------- | ---------------------------------------------- |
| `SCUFRIS_DESKTOP_SOCKET`          | `$XDG_RUNTIME_DIR/scufris/service.sock`        |
| `SCUFRIS_DESKTOP_STATE_FILE`      | `$XDG_STATE_HOME/scufris-desktop/pending.json` |
| `SCUFRIS_STT_ENDPOINT`            | `http://127.0.0.1:10301/inference`             |
| `SCUFRIS_DESKTOP_HOTKEY`          | `Super+D`                                      |
| `SCUFRIS_DESKTOP_CHAT_COMMAND`    | none                                           |
| `SCUFRIS_DESKTOP_RESTART_COMMAND` | none                                           |
| `SCUFRIS_DESKTOP_SPEAK_COMMAND`   | none, and the companion stays silent           |

The companion starts without a backend and reports it as unavailable in the
tray. It answers the pill only when `scufris-service` is on the same socket;
see [maintenance](../dev/maintenance.md) to run one from a working tree.

Transcription still needs an endpoint. A whisper server already listening on
`http://127.0.0.1:10301/inference` is the default, so it needs no override.
Name any other one with the environment variable:

```bash
SCUFRIS_STT_ENDPOINT=http://127.0.0.1:10302/inference nix run .#scufris-desktop
```

With no server anywhere, start one with the model the module pins:

```bash
curl -L -o ggml-base.bin \
  https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin
nix shell nixpkgs#whisper-cpp -c whisper-server \
  --model ggml-base.bin \
  --host 127.0.0.1 --port 10302 --inference-path /inference
```

The companion is Linux and X11 only.
