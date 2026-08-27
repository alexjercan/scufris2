# Scufris

Scufris is a Pi-based assistant with a project workflow engine, delegated agents, Calm mode, optional local voice, and a desktop voice pill.

## Quickstart

Scufris is a background service with clients. `scufris-service` owns the
conversation; a terminal and the desktop companion are two ways to reach it.

Start the service in one terminal. It supervises the first `scufris` on `PATH`;
from a fresh checkout, name one:

```bash
nix run .#scufris-service
nix run .#scufris-service -- \
  --agent "$(nix build --no-link --print-out-paths .#scufris)/bin/scufris"
```

Talk to it from any other terminal:

```bash
nix run .#scufris-ctl -- state              # what Scufris is doing
nix run .#scufris-ctl -- send "hello"       # say something
nix run .#scufris-ctl -- watch              # follow the conversation
nix run .#scufris-ctl -- debug              # open the session in this terminal
```

`debug` takes the agent from the service and gives it back when the terminal
closes, so there is no way to be left detached with nothing to put it back.

The rest of the flake:

```bash
nix run .#scufris-desktop   # the voice pill and tray companion, a client
nix run .#scufris           # one Pi session with the Scufris extensions
nix build .#docs
nix develop
```

Home Manager:

```nix
programs.scufris = {
  enable = true;
  piPackage = inputs.llm-agents.packages.${pkgs.system}.pi;

  voice.enable = true;
  service.enable = true;

  desktop.enable = true;
};
```

`service.enable` gives the service a systemd user unit wanted by
`default.target`, so a machine with no display keeps the conversation.
`desktop.enable` adds the voice pill and tray companion, which is a client of
the service and requires it; `Super+D` opens the pill and starts recording.
`voice.enable` lets the agent decide what is worth saying aloud and hands the
companion the synthesiser that says it.

The service logs to journald: `journalctl --user -t scufris-service -f`.

See the [installation guide](docs/src/guide/installation.md) for the options,
and the [background service](docs/src/dev/service.md) chapter for the socket.
