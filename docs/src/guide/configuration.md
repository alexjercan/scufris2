# Configure it

[Previous: Install it](installation.md)

Home Manager is the deployment interface. Environment variables are the direct
run and development interface.

```text
Home Manager option
        |
        +-> package selection
        +-> systemd user unit
        +-> validated environment
                    |
                    v
             runtime process
```

## Complete option map

```text
programs.scufris
├── enable
├── ctlPackage
├── agent
│   ├── piPackage
│   ├── projectRoots
│   └── package
├── aiToolsApi.{enable,baseUrl}
├── service
│   ├── enable
│   ├── package
│   ├── sessionDirectory
│   ├── serviceName                  (read-only)
│   └── remoteSurface
│       ├── enable
│       ├── port
│       ├── tokenFile
│       ├── serviceName              (read-only)
│       └── tailscaleServiceName     (read-only)
└── desktop
    ├── enable
    ├── package
    ├── aiToolsApi.baseUrl
    ├── speech.{enable,model,voice}
    ├── transcription.{model,language}
    ├── popupKey
    ├── backgroundKey
    ├── abortKey
    ├── terminalCommand
    ├── widgets.{denPath,macrosDatabase}
    ├── serviceName                  (read-only)
    └── restartCommand               (read-only)
```

## Options by task

| Task                                              | Option                                     | Default                                 |
| ------------------------------------------------- | ------------------------------------------ | --------------------------------------- |
| Install the agent                                 | `enable`                                   | `false`                                 |
| Select Pi                                         | `agent.piPackage`                          | pinned `llm-agents` Pi                  |
| Find projects                                     | `agent.projectRoots`                       | `~/personal`, `~/work`, `~/third-party` |
| Replace the complete launcher                     | `agent.package`                            | module-rendered launcher                |
| Run the conversation owner                        | `service.enable`                           | `false`                                 |
| Store its session                                 | `service.sessionDirectory`                 | `$XDG_DATA_HOME/scufris/sessions`       |
| Run the gateway and private Tailscale Serve route | `service.remoteSurface.enable`             | `false`                                 |
| Select gateway port                               | `service.remoteSurface.port`               | `10440`                                 |
| Read gateway secret                               | `service.remoteSurface.tokenFile`          | `null`; required when enabled           |
| Run the Linux surface                             | `desktop.enable`                           | `false`; requires service               |
| Select shared inference host                      | `aiToolsApi.baseUrl`                       | provider URL or `127.0.0.1:10300`       |
| Override the desktop inference host               | `desktop.aiToolsApi.baseUrl`               | shared inference host                   |
| Let Scufris manage the API                        | `aiToolsApi.enable`                        | `false`                                 |
| Enable local speech                               | `desktop.speech.enable`                    | `false`                                 |
| Select speech request                             | `desktop.speech.model`, `.voice`           | `piper-1`, `en_US-lessac-medium`        |
| Select STT request                                | `desktop.transcription.model`, `.language` | `whisper-1`, `auto`                     |
| Open/talk key                                     | `desktop.popupKey`                         | `Super+D`                               |
| Hide/cancel key                                   | `desktop.backgroundKey`                    | derived `Super+Escape`                  |
| Abort key                                         | `desktop.abortKey`                         | derived `Super+Delete`                  |
| Add terminal menu action                          | `desktop.terminalCommand`                  | `null`                                  |
| Locate the journal                                | `desktop.widgets.denPath`                  | `null`                                  |
| Locate food data                                  | `desktop.widgets.macrosDatabase`           | `null`                                  |

Package options (`ctlPackage`, `service.package`, and `desktop.package`) use the
matching package from the pinned Scufris flake. Override them only when composing
packages from another source.

The read-only names are stable generated values:

| Option                                       | Value                                                  |
| -------------------------------------------- | ------------------------------------------------------ |
| `service.serviceName`                        | `scufris-service`                                      |
| `service.remoteSurface.serviceName`          | `scufris-surface-gateway`                              |
| `service.remoteSurface.tailscaleServiceName` | `scufris-tailscale-serve`                              |
| `desktop.serviceName`                        | `scufris-desktop`                                      |
| `desktop.restartCommand`                     | generated command that restarts only the owned service |

For evaluated types, assertions, and descriptions, use the generated
[Home Manager option reference](../reference/options.md). That page is built
from `nix/home-manager.nix`, so it is the source of truth rather than a copied
list.

## Common complete configuration

```nix
programs.scufris = {
  enable = true;

  agent = {
    piPackage = config.programs.agents.pi.finalPackage;
    projectRoots = ["~/personal" "~/work"];
  };

  aiToolsApi = {
    enable = false;
    baseUrl = "http://127.0.0.1:10300";
  };

  service = {
    enable = true;
    sessionDirectory = "${config.xdg.dataHome}/scufris/sessions";
  };

  desktop = {
    enable = true;
    speech.enable = true;
    popupKey = "Super+D";
    backgroundKey = "Super+Escape";
    abortKey = "Super+Delete";
  };
};
```

## Key rules

```text
backgroundKey = null -> derive modifiers + Escape
abortKey      = null -> derive modifiers + Delete
value       = "none" -> do not grab that key
```

## Deprecated aliases

These aliases exist for one compatibility release and map to the ownership tree
above: `piPackage`, `projectRoots`, `finalPackage`, `voice`, `service.agent`,
`service.agentPackage`, `desktop.aiToolsApi.manage`, `desktop.hotkey`,
`desktop.cancelKey`, `desktop.stopKey`, `desktop.chatCommand`,
`desktop.denPath`, and `desktop.macrosDatabase`.

`desktop.todayCommand` and `desktop.widgets.todayCommand` are removed. The
journal widgets read the-den themselves and there is no command to point at.

Use the new names in all new configuration.

## Direct runs

When Home Manager is not creating units, set environment variables directly:

```bash
SCUFRIS_RUNTIME_DIR="$XDG_RUNTIME_DIR/scufris-test" \
SCUFRIS_SERVICE_AGENT="$(nix build --no-link --print-out-paths .#scufris)/bin/scufris" \
  nix run .#scufris-service
```

Do not guess variable names. Use the complete [environment reference](../reference/environment.md).

---

Next: [Use it](using.md)
