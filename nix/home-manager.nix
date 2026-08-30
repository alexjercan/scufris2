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
  agentCfg = cfg.agent;
  serviceCfg = cfg.service;
  managedApiCfg = cfg.aiToolsApi;
  desktopCfg = cfg.desktop;
  desktopApiCfg = desktopCfg.aiToolsApi;
  speechCfg = desktopCfg.speech;
  transcriptionCfg = desktopCfg.transcription;
  widgetCfg = desktopCfg.widgets;
  transcriptionEndpoint = "${desktopApiCfg.baseUrl}/v1/audio/transcriptions";
  speechEndpoint = "${desktopApiCfg.baseUrl}/v1/audio/speech";
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
    model = speechCfg.model;
    voice = speechCfg.voice;
  };
in {
  imports = [
    (lib.mkRenamedOptionModule ["programs" "scufris" "piPackage"] ["programs" "scufris" "agent" "piPackage"])
    (lib.mkRenamedOptionModule ["programs" "scufris" "projectRoots"] ["programs" "scufris" "agent" "projectRoots"])
    (lib.mkRenamedOptionModule ["programs" "scufris" "finalPackage"] ["programs" "scufris" "agent" "package"])
    (lib.mkRenamedOptionModule ["programs" "scufris" "service" "agentPackage"] ["programs" "scufris" "agent" "package"])
    (lib.mkRenamedOptionModule ["programs" "scufris" "service" "agent"] ["programs" "scufris" "agent"])
    (lib.mkRenamedOptionModule ["programs" "scufris" "voice"] ["programs" "scufris" "desktop" "speech"])
    (lib.mkRenamedOptionModule ["programs" "scufris" "desktop" "aiToolsApi" "manage"] ["programs" "scufris" "aiToolsApi" "enable"])
    (lib.mkRenamedOptionModule ["programs" "scufris" "desktop" "hotkey"] ["programs" "scufris" "desktop" "popupKey"])
    (lib.mkRenamedOptionModule ["programs" "scufris" "desktop" "cancelKey"] ["programs" "scufris" "desktop" "backgroundKey"])
    (lib.mkRenamedOptionModule ["programs" "scufris" "desktop" "stopKey"] ["programs" "scufris" "desktop" "abortKey"])
    (lib.mkRenamedOptionModule ["programs" "scufris" "desktop" "chatCommand"] ["programs" "scufris" "desktop" "terminalCommand"])
    (lib.mkRenamedOptionModule ["programs" "scufris" "desktop" "todayCommand"] ["programs" "scufris" "desktop" "widgets" "todayCommand"])
    (lib.mkRenamedOptionModule ["programs" "scufris" "desktop" "denPath"] ["programs" "scufris" "desktop" "widgets" "denPath"])
    (lib.mkRenamedOptionModule ["programs" "scufris" "desktop" "macrosDatabase"] ["programs" "scufris" "desktop" "widgets" "macrosDatabase"])
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

    agent = {
      piPackage = lib.mkOption {
        type = lib.types.package;
        default = defaults.piPackage;
        defaultText = lib.literalExpression "inputs.llm-agents.packages.${system}.pi";
        description = "Pi package used by the default Scufris agent launcher.";
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
          Interactive Scufris agent launcher installed as `scufris` and run by
          the service in RPC mode. Override it with another compatible harness.
        '';
      };
    };

    aiToolsApi.enable = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = ''
        Manage the pinned complete ai-tools-api service. Leave false when the
        API is provided by services.ai-tools-api or outside Home Manager.
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
        baseUrl = lib.mkOption {
          type = lib.types.strMatching "https?://.*[^/]";
          default = providerBaseUrl;
          defaultText = lib.literalExpression "the enabled provider URL, otherwise http://127.0.0.1:10300";
          description = "Base URL of the shared bounded speech inference API.";
        };
      };

      speech = {
        enable = lib.mkEnableOption "local speech synthesised through ai-tools-api";

        model = lib.mkOption {
          type = lib.types.strMatching "[A-Za-z0-9._-]+";
          default = "piper-1";
          description = "Speech model sent to ai-tools-api.";
        };

        voice = lib.mkOption {
          type = lib.types.strMatching "[A-Za-z0-9._-]+";
          default = "en_US-lessac-medium";
          description = "Speech voice sent to ai-tools-api.";
        };
      };

      transcription = {
        model = lib.mkOption {
          type = lib.types.strMatching "[A-Za-z0-9._-]+";
          default = "whisper-1";
          description = "Transcription model sent to ai-tools-api.";
        };

        language = lib.mkOption {
          type = lib.types.strMatching "[A-Za-z0-9_-]+";
          default = "auto";
          description = "Transcription language sent to ai-tools-api.";
        };
      };

      popupKey = lib.mkOption {
        type = lib.types.strMatching "[A-Za-z0-9+]+";
        default = "Super+D";
        description = "Key that shows the pill: tap for the workspace, hold to talk.";
      };

      backgroundKey = lib.mkOption {
        type = lib.types.nullOr (lib.types.strMatching "[A-Za-z0-9+]+|none");
        default = null;
        description = ''
          Key that puts the pill in the background and discards a current take.
          When null it is derived from the popup key's modifiers, so `Super+D`
          gives `Super+Escape`. Set it to `"none"` to leave the key to the
          desktop. The companion holds it only while the pill is visible.
        '';
      };

      abortKey = lib.mkOption {
        type = lib.types.nullOr (lib.types.strMatching "[A-Za-z0-9+]+|none");
        default = null;
        description = ''
          Key that aborts the current Scufris run. When null it is derived from
          the popup key's modifiers, so `Super+D` gives `Super+Delete`. Set it
          to `"none"` to leave the key to the desktop. The companion holds it
          only while the pill is visible.
        '';
      };

      terminalCommand = lib.mkOption {
        type = lib.types.nullOr lib.types.package;
        default = null;
        description = ''
          Executable that opens a deployment-specific terminal view from the
          tray. Scufris ships no terminal session handoff protocol.
        '';
      };

      widgets = {
        todayCommand = lib.mkOption {
          type = lib.types.nullOr lib.types.package;
          default = null;
          description = ''
            Executable that reads and writes the-den journal for the agenda,
            macros, and notes widgets. Widgets that need it report when it is
            absent.
          '';
        };

        denPath = lib.mkOption {
          type = lib.types.nullOr lib.types.str;
          default = null;
          example = "/home/you/personal/the-den";
          description = ''
            Journal directory when it is not where today looks by default. A
            systemd user service does not inherit the login shell's DEN_PATH.
          '';
        };

        macrosDatabase = lib.mkOption {
          type = lib.types.nullOr lib.types.str;
          default = null;
          example = "/home/you/.local/share/nvim/macros.csv";
          description = "Food database used by the macros widget.";
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
      programs.scufris.desktop.restartCommand = backendRestart;
    }
    (lib.mkIf cfg.enable {
      home.packages = [agentCfg.package];
    })
    (lib.mkIf (cfg.enable && managedApiCfg.enable) {
      assertions = [
        (lib.hm.assertions.assertPlatform "programs.scufris.aiToolsApi" pkgs lib.platforms.linux)
        {
          assertion = !providerEnabled;
          message = "programs.scufris.aiToolsApi.enable conflicts with an enabled services.ai-tools-api provider";
        }
      ];
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
          assertion = !managedApiCfg.enable || desktopApiCfg.baseUrl == "http://127.0.0.1:10300";
          message = "the managed Scufris ai-tools-api fallback uses http://127.0.0.1:10300; set programs.scufris.aiToolsApi.enable=false for another base URL";
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
              "SCUFRIS_STT_ENDPOINT=${transcriptionEndpoint}"
              "SCUFRIS_STT_MODEL=${transcriptionCfg.model}"
              "SCUFRIS_STT_LANGUAGE=${transcriptionCfg.language}"
              "SCUFRIS_DESKTOP_HOTKEY=${desktopCfg.popupKey}"
              "SCUFRIS_DESKTOP_RESTART_COMMAND=${lib.getExe backendRestart}"
            ]
            ++ lib.optional (desktopCfg.backgroundKey != null)
            "SCUFRIS_DESKTOP_CANCEL_KEY=${desktopCfg.backgroundKey}"
            ++ lib.optional (desktopCfg.abortKey != null)
            "SCUFRIS_DESKTOP_STOP_KEY=${desktopCfg.abortKey}"
            ++ lib.optional (desktopCfg.terminalCommand != null)
            "SCUFRIS_DESKTOP_CHAT_COMMAND=${lib.getExe desktopCfg.terminalCommand}"
            ++ lib.optional (widgetCfg.todayCommand != null)
            "SCUFRIS_TODAY_COMMAND=${lib.getExe widgetCfg.todayCommand}"
            ++ lib.optional (widgetCfg.denPath != null)
            "DEN_PATH=${widgetCfg.denPath}"
            ++ lib.optional (widgetCfg.macrosDatabase != null)
            "MACROS_DATABASE=${widgetCfg.macrosDatabase}"
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
    (lib.mkIf (cfg.enable && managedApiCfg.enable) {
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
