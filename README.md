# Scufris

Scufris is a Pi-based assistant with delegated project workflows, a background
service that owns the conversation, and a Linux desktop companion for voice,
conversation, and widgets.

## Quickstart

Run the complete stack from this checkout in an isolated staging environment:

```bash
nix run .#staging -- up
```

Staging uses the current Pi login, keeps its own sessions and runtime paths, and
runs beside any deployed Scufris. It also starts an authenticated external-
surface gateway on loopback port 10441. When Tailscale Serve is available it
publishes the temporary `/scufris-staging` tailnet route and removes only that
route on shutdown. The command prints the WSS URL and private token path. Set
`SCUFRIS_STAGING_EXTERNAL_SURFACES=local` to keep the gateway loopback-only.
Press `Ctrl+C` to stop the complete stack.

For a Home Manager deployment that consumes the existing `ai-tools-api` on
port 10300:

```nix
programs.scufris = {
  enable = true;
  # ctlPackage = inputs.scufris.packages.${pkgs.system}.scufris-ctl;

  # This is the interactive `scufris` command and the agent the service runs.
  agent = {
    piPackage = config.programs.agents.pi.finalPackage;
    projectRoots = ["~/personal" "~/work" "~/third-party"];
    # Leave unset for the launcher rendered from the two options above, or set
    # a package that provides a compatible Scufris agent harness.
    # package = myScufrisAgent;
  };

  # Set true only when Scufris should manage the one API service on this machine.
  aiToolsApi.enable = false;

  service = {
    enable = true;
    # package = inputs.scufris.packages.${pkgs.system}.scufris-service;
    sessionDirectory = "${config.xdg.dataHome}/scufris/sessions";

    remoteSurface = {
      enable = true;
      port = 10440;
      # Generate this as a private file or provide it through SOPS.
      tokenFile = "${config.xdg.dataHome}/scufris/credentials/ios/surface-token";
    };
  };

  desktop = {
    enable = true;
    # package = inputs.scufris.packages.${pkgs.system}.scufris-desktop;

    aiToolsApi.baseUrl = "http://127.0.0.1:10300";

    speech = {
      enable = true;
      model = "piper-1";
      voice = "en_US-lessac-medium";
    };
    transcription = {
      model = "whisper-1";
      language = "auto";
    };

    popupKey = "Super+D";
    backgroundKey = "Super+Escape";
    abortKey = "Super+Delete";

    terminalCommand = null;
    widgets = {
      denPath = null;
      macrosDatabase = null;
    };
  };
};
```

Briefings run on their own systemd timers. Nix owns when each one happens and
what it may spend; each project owns what is in it.

```nix
programs.scufris.agent.briefing = {
  profiles = {
    morning.schedule = "08:00";

    nightly = {
      schedule = "23:00";
      deadline = 28800; # seconds for the whole run
      sourceDeadline = 28800; # seconds for one source, held to the smaller
      parallel = 2; # sources at once, however many declare the profile
      maxOffers = 8; # things one source may say are worth doing next
      maxBody = 65536; # characters of Markdown one source may report
    };
  };

  keepDays = 30; # days of briefings kept on disk

  # Sources with no checkout to belong to, such as one reporting what the jobs
  # helper measured. A project declares its own in its own file.
  sources.morning.jobs = {
    description = "What Scufris did overnight.";
    keywords = {harness = "pi"; thinking = "medium";};
    guidance = ''...''; # what this source is asked to read and report
  };
};
```

A project declares its briefings in its own `.scufris.toml`, so a checkout still
works for someone whose machine has none of this:

```toml
[briefings.morning]
description = "Report CI on master and where the project stands."
keywords = { harness = "claude", model = "Opus", thinking = "medium" }
guidance = """..."""  # what to read, what to report, what not to touch

[briefings.nightly]
description = "Review the day's commits and report what is worth fixing."
keywords = { harness = "claude", model = "Opus", thinking = "xhigh" }
guidance = """..."""  # a source holds every tool; its guidance is the limit
```

The module supplies the `scufris-ctl`, service, remote surface gateway,
Tailscale client, agent launcher, and desktop packages from the pinned flake.
Enabling `remoteSurface` starts both the loopback gateway and a declaratively
reconciled Tailscale Serve route at `/`; the user must be allowed to run
`tailscale serve`.
Their package options remain available for advanced overrides. The service,
remote gateway, and Tailscale route have read-only service names; the desktop
also has a generated read-only `serviceName` and `restartCommand`.
