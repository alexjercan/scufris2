{defaultsFor}: {
  config,
  lib,
  pkgs,
  options,
  ...
}: let
  cfg = config.programs.scufris;
  system = pkgs.stdenv.hostPlatform.system;
  defaults = defaultsFor system;
  providerAvailable = lib.hasAttrByPath ["services" "ai-tools-api" "enable"] options;
  providerEnabled = providerAvailable && config.services.ai-tools-api.enable;
  providerBaseUrl =
    if providerAvailable
    then "http://${config.services.ai-tools-api.host}:${toString config.services.ai-tools-api.port}"
    else "http://127.0.0.1:10300";
  serviceCfg = cfg.service;
  agentCfg = serviceCfg.agent;
  desktopCfg = cfg.desktop;
  apiCfg = desktopCfg.aiToolsApi;
  speechCfg = desktopCfg.speech;
  transcriptionCfg = desktopCfg.transcription;
  resolvedEndpoint =
    if transcriptionCfg.endpoint != null
    then transcriptionCfg.endpoint
    else "${apiCfg.baseUrl}/v1/audio/transcriptions";
  speechEndpoint = "${apiCfg.baseUrl}/v1/audio/speech";
  # The companion may only restart the backend service this module owns, so the
  # hook is generated here instead of accepting a command from the model or the
  # environment.
  backendRestart = pkgs.writeShellApplication {
    name = "scufris-restart-backend";
    runtimeInputs = [pkgs.systemd];
    text = ''
      exec systemctl --user restart ${lib.escapeShellArg "${serviceCfg.serviceName}.service"}
    '';
    meta.mainProgram = "scufris-restart-backend";
  };
  launcher = import ./launcher.nix {
    inherit pkgs;
    resources = defaults.resources;
    piPackage = agentCfg.piPackage;
    projectRoots = agentCfg.projectRoots;
  };
  # The frontend owns the speaker, so the synthesiser is bound here and handed
  # to the companion. A deployment with no speech hands it nothing and the
  # companion stays silent.
  speak = import ./speak.nix {
    inherit pkgs;
    endpoint = speechEndpoint;
  };
in {
  imports = [
    (lib.mkRenamedOptionModule ["programs" "scufris" "piPackage"] ["programs" "scufris" "service" "agent" "piPackage"])
    (lib.mkRenamedOptionModule ["programs" "scufris" "projectRoots"] ["programs" "scufris" "service" "agent" "projectRoots"])
    (lib.mkRenamedOptionModule ["programs" "scufris" "finalPackage"] ["programs" "scufris" "service" "agent" "package"])
    (lib.mkRenamedOptionModule ["programs" "scufris" "service" "agentPackage"] ["programs" "scufris" "service" "agent" "package"])
    (lib.mkRenamedOptionModule ["programs" "scufris" "voice"] ["programs" "scufris" "desktop" "speech"])
    (lib.mkRenamedOptionModule ["programs" "scufris" "desktop" "stt"] ["programs" "scufris" "desktop" "transcription"])
  ];

  options.programs.scufris = {
    enable = lib.mkEnableOption "Scufris Pi launcher";

    ctlPackage = lib.mkOption {
      type = lib.types.package;
      default = defaults.ctlPackage;
      defaultText = lib.literalExpression "self.packages.\${system}.scufris-ctl";
      description = ''
        scufris-ctl package. Installed by whichever of the service and the
        companion is enabled, because a window manager binding and a terminal
        both reach Scufris by name and neither wants the other's package.
      '';
    };

    service = {
      enable = lib.mkEnableOption "the headless scufris-service background service";

      package = lib.mkOption {
        type = lib.types.package;
        default = defaults.servicePackage;
        defaultText = lib.literalExpression "self.packages.\${system}.scufris-service";
        description = "scufris-service package.";
      };

      agent = {
        piPackage = lib.mkOption {
          type = lib.types.package;
          default = defaults.piPackage;
          defaultText = lib.literalExpression "inputs.llm-agents.packages.${system}.pi";
          description = "Pi package used by the service's Scufris agent.";
        };

        projectRoots = lib.mkOption {
          type = lib.types.listOf lib.types.str;
          default = ["~/personal" "~/work" "~/third-party"];
          description = "Directories recursively searched for workflow projects.";
        };

        package = lib.mkOption {
          type = lib.types.package;
          default = launcher;
          defaultText = lib.literalExpression "the Scufris agent launcher rendered by this module";
          description = ''
            Launcher the service runs as its one Pi agent. The service starts it
            in RPC mode and accepts exactly one protocol v4 agent connection.
          '';
        };
      };

      sessionDirectory = lib.mkOption {
        type = lib.types.strMatching "/.*";
        default = "${config.xdg.dataHome}/scufris/sessions";
        defaultText = lib.literalExpression "\"\${config.xdg.dataHome}/scufris/sessions\"";
        description = ''
          Absolute directory the service keeps its Pi conversation in. The
          service is its only owner.
        '';
      };

      serviceName = lib.mkOption {
        type = lib.types.str;
        default = "scufris-service";
        readOnly = true;
        description = "Stable systemd user service identity for the background service, without the unit suffix.";
      };
    };

    desktop = {
      enable = lib.mkEnableOption "the scufris-desktop voice pill and tray companion";

      package = lib.mkOption {
        type = lib.types.package;
        default = defaults.desktopPackage;
        defaultText = lib.literalExpression "self.packages.\${system}.scufris-desktop";
        description = "scufris-desktop companion package.";
      };

      aiToolsApi = {
        manage = lib.mkOption {
          type = lib.types.bool;
          default = !providerEnabled;
          defaultText = lib.literalExpression "the inverse of an enabled services.ai-tools-api provider";
          description = ''
            Run the pinned complete ai-tools-api package as a fallback service.
            This defaults off when another imported Home Manager module already
            enables services.ai-tools-api. Set false for any other externally
            managed endpoint.
          '';
        };

        baseUrl = lib.mkOption {
          type = lib.types.strMatching "https?://.*[^/]";
          default = providerBaseUrl;
          defaultText = lib.literalExpression "the enabled provider URL, otherwise http://127.0.0.1:10300";
          description = "Base URL of the shared bounded speech inference API.";
        };
      };

      speech.enable = lib.mkEnableOption "local speech synthesised through ai-tools-api";

      hotkey = lib.mkOption {
        type = lib.types.strMatching "[A-Za-z0-9+]+";
        default = "Super+D";
        description = "Accelerator the pill answers: tap for the workspace, hold to talk.";
      };

      cancelKey = lib.mkOption {
        type = lib.types.nullOr (lib.types.strMatching "[A-Za-z0-9+]+|none");
        default = null;
        description = ''
          Accelerator that puts the pill away and throws away the take. When
          null it is derived from the hotkey's own modifiers, so `Super+D`
          gives `Super+Escape`. Set it to `"none"` to leave the key to the
          desktop; the tray puts the pill away without it.

          The companion holds this key only while the pill is on screen.
        '';
      };

      stopKey = lib.mkOption {
        type = lib.types.nullOr (lib.types.strMatching "[A-Za-z0-9+]+|none");
        default = null;
        description = ''
          Accelerator that stops what Scufris is doing. When null it is derived
          from the hotkey's own modifiers, so `Super+D` gives `Super+Delete`.
          Set it to `"none"` to leave the key to the desktop.

          The companion holds this key only while the pill is on screen.
        '';
      };

      chatCommand = lib.mkOption {
        type = lib.types.nullOr lib.types.package;
        default = null;
        description = ''
          Executable that opens a deployment-specific terminal view from the
          tray. Scufris ships no terminal session handoff protocol.
        '';
      };

      todayCommand = lib.mkOption {
        type = lib.types.nullOr lib.types.package;
        default = null;
        description = ''
          Executable that reads and writes the-den journal, for the agenda,
          macros, and notes panels. Supplied by the deployment rather than
          taken as a flake input: the journal is personal data, and Scufris
          does not depend on the repository that holds it. Panels that need it
          say so on the panel when it is absent.
        '';
      };

      denPath = lib.mkOption {
        type = lib.types.nullOr lib.types.str;
        default = null;
        example = "/home/you/personal/the-den";
        description = ''
          The journal directory, when it is not where the today command looks
          by default. A systemd user service does not inherit the login
          shell's `DEN_PATH`, so a shell that sets one must say so here too.
        '';
      };

      macrosDatabase = lib.mkOption {
        type = lib.types.nullOr lib.types.str;
        default = null;
        example = "/home/you/.local/share/nvim/macros.csv";
        description = ''
          The food database the macros panel names a food out of, when it is
          not where the today command looks by default. Logging a food is a
          name and an amount, and the database is what turns those into a row.
        '';
      };

      transcription = {
        endpoint = lib.mkOption {
          type = lib.types.nullOr (lib.types.strMatching "https?://.*");
          default = null;
          description = ''
            OpenAI-compatible transcription endpoint. When null, the
            transcription route under desktop.aiToolsApi.baseUrl is used.
          '';
        };

        resolvedEndpoint = lib.mkOption {
          type = lib.types.str;
          readOnly = true;
          description = "Transcription endpoint the companion actually uses.";
        };
      };

      serviceName = lib.mkOption {
        type = lib.types.str;
        default = "scufris-desktop";
        readOnly = true;
        description = "Stable systemd user service identity for desktop consumers, without the unit suffix.";
      };

      restartCommand = lib.mkOption {
        type = lib.types.package;
        readOnly = true;
        description = "Generated hook that restarts only the Scufris backend service this module owns.";
      };
    };
  };

  config = lib.mkMerge [
    {
      programs.scufris.desktop = {
        transcription.resolvedEndpoint = resolvedEndpoint;
        restartCommand = backendRestart;
      };
    }
    (lib.mkIf cfg.enable {
      home.packages = [agentCfg.package];
    })
    (lib.mkIf (cfg.enable && speechCfg.enable) {
      assertions = [
        (lib.hm.assertions.assertPlatform "programs.scufris.desktop.speech" pkgs lib.platforms.linux)
      ];
    })
    (lib.mkIf (cfg.enable && serviceCfg.enable) {
      assertions = [
        (lib.hm.assertions.assertPlatform "programs.scufris.service" pkgs lib.platforms.linux)
      ];

      home.packages = [serviceCfg.package cfg.ctlPackage];

      systemd.user.services.${serviceCfg.serviceName} = {
        Unit = {
          # No graphical session, and nothing ordered after one. The
          # service is the half that keeps the conversation whether or not
          # anything is on screen, and a terminal over ssh reaches it with
          # scufris-ctl.
          Description = "Scufris background service";
        };
        Service = {
          Type = "simple";
          ExecStart = lib.getExe serviceCfg.package;
          # Nothing about speech. The agent shapes every answer as one
          # prose paragraph whatever is listening, and whether a sound is
          # made is the companion's, which is where the speaker is.
          Environment = [
            "SCUFRIS_SERVICE_AGENT=${lib.getExe agentCfg.package}"
            "SCUFRIS_SERVICE_SESSION_DIR=${serviceCfg.sessionDirectory}"
          ];
          # The service restarts its own agent, so it going down is a fault of
          # the service itself and the conversation is on disk either way.
          Restart = "on-failure";
          RestartSec = 3;
          RuntimeDirectory = serviceCfg.serviceName;
          WorkingDirectory = "%h";
        };
        Install.WantedBy = ["default.target"];
      };
    })
    (lib.mkIf (cfg.enable && desktopCfg.enable) {
      assertions = [
        (lib.hm.assertions.assertPlatform "programs.scufris.desktop" pkgs lib.platforms.linux)
        {
          assertion = serviceCfg.enable;
          message = "programs.scufris.desktop.enable requires programs.scufris.service.enable, because the companion is a client of the service that owns the conversation";
        }
        {
          assertion = !(providerEnabled && apiCfg.manage);
          message = "programs.scufris.desktop.aiToolsApi.manage conflicts with an enabled services.ai-tools-api provider";
        }
        {
          assertion = !apiCfg.manage || apiCfg.baseUrl == "http://127.0.0.1:10300";
          message = "the managed Scufris ai-tools-api fallback uses http://127.0.0.1:10300; set manage=false for another base URL";
        }
      ];

      home.packages = [desktopCfg.package cfg.ctlPackage];

      systemd.user.services.${desktopCfg.serviceName} = {
        Unit = {
          Description = "Scufris voice pill and tray companion";
          PartOf = ["graphical-session.target"];
          After = ["graphical-session.target"];
        };
        Service = {
          Type = "simple";
          ExecStart = lib.getExe desktopCfg.package;
          Environment =
            [
              "SCUFRIS_STT_ENDPOINT=${resolvedEndpoint}"
              "SCUFRIS_DESKTOP_HOTKEY=${desktopCfg.hotkey}"
              "SCUFRIS_DESKTOP_RESTART_COMMAND=${lib.getExe backendRestart}"
            ]
            ++ lib.optional (desktopCfg.cancelKey != null)
            "SCUFRIS_DESKTOP_CANCEL_KEY=${desktopCfg.cancelKey}"
            ++ lib.optional (desktopCfg.stopKey != null)
            "SCUFRIS_DESKTOP_STOP_KEY=${desktopCfg.stopKey}"
            ++ lib.optional (desktopCfg.chatCommand != null)
            "SCUFRIS_DESKTOP_CHAT_COMMAND=${lib.getExe desktopCfg.chatCommand}"
            ++ lib.optional (desktopCfg.todayCommand != null)
            "SCUFRIS_TODAY_COMMAND=${lib.getExe desktopCfg.todayCommand}"
            ++ lib.optional (desktopCfg.denPath != null)
            "DEN_PATH=${desktopCfg.denPath}"
            ++ lib.optional (desktopCfg.macrosDatabase != null)
            "MACROS_DATABASE=${desktopCfg.macrosDatabase}"
            ++ lib.optional speechCfg.enable
            "SCUFRIS_DESKTOP_SPEAK_COMMAND=${lib.getExe speak}";
          # The companion must survive its own faults; a backend crash is
          # reported in the tray instead of taking the companion down.
          Restart = "on-failure";
          RestartSec = 3;
          # Holds the accepted transcript that has not been acknowledged, so a
          # companion restart resumes with it instead of losing it.
          StateDirectory = desktopCfg.serviceName;
          WorkingDirectory = "%h";
        };
        Install.WantedBy = ["graphical-session.target"];
      };
    })
    (lib.mkIf (cfg.enable && desktopCfg.enable && apiCfg.manage) {
      systemd.user.services.scufris-ai-tools-api = {
        Unit = {
          Description = "Scufris fallback AI tools API";
          After = ["network.target"];
        };
        Service = {
          Type = "simple";
          ExecStart = lib.getExe defaults.aiToolsApiPackage;
          Restart = "on-failure";
          RestartSec = 5;
          RuntimeDirectory = "scufris-ai-tools-api";
          WorkingDirectory = "%t/scufris-ai-tools-api";
          NoNewPrivileges = true;
          PrivateTmp = true;
          ProtectSystem = "strict";
          ProtectHome = "tmpfs";
          UMask = "0077";
          TimeoutStopSec = 10;
        };
        Install.WantedBy = ["default.target"];
      };
    })
  ];
}
