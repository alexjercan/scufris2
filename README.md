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
      todayCommand = null;
      denPath = null;
      macrosDatabase = null;
    };
  };
};
```

The module supplies the `scufris-ctl`, service, remote surface gateway,
Tailscale client, agent launcher, and desktop packages from the pinned flake.
Enabling `remoteSurface` starts both the loopback gateway and a declaratively
reconciled Tailscale Serve route at `/`; the user must be allowed to run
`tailscale serve`.
Their package options remain available for advanced overrides. The service,
remote gateway, and Tailscale route have read-only service names; the desktop
also has a generated read-only `serviceName` and `restartCommand`.
