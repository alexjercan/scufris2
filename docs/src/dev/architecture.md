# See the stack

[Previous: Start here](../overview.md)

Scufris has four layers. Read the diagram from the user toward the work.

```mermaid
flowchart TB
    subgraph Surfaces["SURFACES - own local input and presentation"]
        Desktop["Desktop<br/>X11, voice, windows, widgets"]
        IOS["iOS<br/>WSS, text UI"]
        Future["Future machine client"]
    end
    subgraph Host["HOST - owns the conversation"]
        Service["scufris-service<br/>Pi process, session, state, replay, last 200 messages"]
        Sockets["surface.sock | agent.sock | control.sock"]
        Service --- Sockets
    end
    subgraph Agent["AGENT - owns the turn"]
        Pi["Pi + workflow + response + calm + service<br/>one identity, one atomic answer"]
    end
    subgraph Workers["WORKERS - own delegated execution"]
        Helper[jobs helper] --> Tmux[owned tmux session] --> Harness[Pi or Claude]
        Durable["prompt | report | events | workspace | transcript"]
    end
    Desktop -->|protocol v4| Service
    IOS -->|protocol v4| Service
    Future -->|protocol v4| Service
    Service --> Pi
    Pi --> Helper
```

## One user message

```mermaid
sequenceDiagram
    actor User
    participant Surface
    participant Service as scufris-service
    participant Pi as Pi + Scufris
    User->>Surface: type or speak
    Surface->>Service: surface.message {id, text}
    Service-->>Surface: broadcast canonical user message
    Service->>Pi: agent.message {text, widgets}
    Pi->>Pi: answer now or start a durable worker
    Pi->>Service: agent.response {text, details?, widgets?}
    Service-->>Surface: validated canonical response
    Note over Surface: Associated surface may speak and run widget calls
```

A surface crash does not stop Pi. A Pi restart does not erase the session. A
worker does not block the foreground conversation.

## One delegated job

```mermaid
flowchart TB
    Request --> Workflow[workflow extension] --> Helper[jobs helper writes durable state]
    Helper --> Tmux[tmux starts one owned execution]
    Tmux --> Working[working]
    Tmux --> Blocked[blocked]
    Tmux --> Done[done]
    Tmux --> Failed[failed]
    Working -. quiet by default .-> Conversation
    Blocked -->|wake| Conversation
    Done -->|wake| Conversation
    Failed -->|wake| Conversation
```

A logical job can have several generations. Steering stops the old execution,
keeps the workspace and harness session, and starts the next generation.
Landing and cleanup are explicit.

## Ownership map

```text
agent/extensions/scufris/   Pi lifecycle, tools, state, routing
agent/skills/               model-facing workflow policy
host/service/               headless conversation owner and scufris-ctl
shared/control/             protocol v4 types, bounds, and socket paths
surfaces/desktop/           Linux/X11 surface, voice, windows, widgets
surfaces/ios/               SwiftUI remote surface
scripts/                    commands for people
tools/                      deterministic helpers called by extensions
nix/                        packages, module, staging, checks, docs
```

Code belongs with its runtime owner. Do not put window policy in the agent. Do
not put conversation state in a surface. Do not make an extension do process or
filesystem work that a small helper can do.

## Local and remote paths

```mermaid
flowchart LR
    Local[local surface] -->|Unix socket| Socket[surface.sock]
    Remote[remote surface] -->|WSS + bearer token| Tailscale[owned Tailscale Serve route]
    Tailscale -->|loopback HTTP| Gateway[scufris-surface-gateway]
    Gateway --> Socket
    Socket --> Service[scufris-service]
```

Only surface traffic crosses the gateway. `agent.sock` and `control.sock` stay
local.

## Trust boundaries

| Boundary       | Rule                                                             |
| -------------- | ---------------------------------------------------------------- |
| Surface input  | Strict protocol version, type, identifier, and byte bounds       |
| Agent output   | One validated atomic response                                    |
| Worker reports | Fresh capability for each generation                             |
| Job files      | Bounded reads, regular files, `O_NOFOLLOW` where required        |
| Reviewer tools | Harness-specific read allowlist; not an OS sandbox               |
| Remote surface | Loopback gateway, private token, owned Tailscale Serve TLS route |

## Package graph

```mermaid
flowchart TB
    Resources[resources] --> Launcher[scufris launcher]
    Pi[Pi] --> Launcher
    Service["scufris-service + scufris-ctl<br/>Linux, headless"] --> Gateway["optional surface gateway"]
    Gateway --> Tailscale[declarative Tailscale Serve root route]
    Desktop["scufris-desktop<br/>Linux + X11"] --> Speak[optional scufris-speak]
    Desktop --> API[external or managed ai-tools-api]
    Speak --> API
```

The desktop is not in the launcher closure. Graphical libraries are not in the
service closure. Speech is not in the agent process tree.

## State map

```text
$XDG_RUNTIME_DIR/scufris/         sockets; disappears with the login session
$XDG_DATA_HOME/scufris/sessions/  canonical Pi conversation
$XDG_STATE_HOME/scufris/jobs/     jobs and archived workflows
$XDG_STATE_HOME/scufris-desktop/  pending transcript + stable surface ID
```

`SCUFRIS_RUNTIME_DIR` moves the socket group for staging. The complete variable
list is in [Environment variables](../reference/environment.md).

---

Next: [Install it](../guide/installation.md)
