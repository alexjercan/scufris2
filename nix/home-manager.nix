{defaultsFor}: {
  config,
  lib,
  pkgs,
  ...
}: let
  cfg = config.programs.scufris;
  system = pkgs.stdenv.hostPlatform.system;
  defaults = defaultsFor system;
  voiceRuntime = import ./voice.nix {inherit pkgs;};
  whisperRuntime = import ./whisper.nix {inherit pkgs;};
  desktopCfg = cfg.desktop;
  whisperCfg = desktopCfg.stt.whisper;
  bundledEndpoint = "http://${whisperCfg.host}:${toString whisperCfg.port}${whisperRuntime.inferencePath}";
  resolvedEndpoint =
    if desktopCfg.stt.endpoint != null
    then desktopCfg.stt.endpoint
    else bundledEndpoint;
  # The companion may only restart the backend service this module owns, so the
  # hook is generated here instead of accepting a command from the model or the
  # environment.
  backendRestart = pkgs.writeShellApplication {
    name = "scufris-restart-backend";
    runtimeInputs = [pkgs.systemd];
    text = ''
      exec systemctl --user restart ${lib.escapeShellArg "${cfg.voice.popup.serviceName}.service"}
    '';
    meta.mainProgram = "scufris-restart-backend";
  };
  launcher = import ./launcher.nix {
    inherit pkgs;
    resources =
      if cfg.voice.enable
      then defaults.voiceResources
      else defaults.resources;
    piPackage = cfg.piPackage;
    dashboardctlPackage = cfg.dashboard.dashboardctlPackage;
    dashboard = cfg.dashboard.enable;
    voice = cfg.voice.enable;
    piperPackage = cfg.voice.piper.package;
    piperModel = cfg.voice.piper.model;
    piperConfig = cfg.voice.piper.config;
    projectRoots = cfg.projectRoots;
  };
  popupLauncher = import ./popup.nix {
    inherit pkgs;
    scufrisPackage = launcher;
    piperModel = cfg.voice.piper.model;
    piperConfig = cfg.voice.piper.config;
    terminalPackage = cfg.voice.popup.terminalPackage;
    sessionDirectory = cfg.voice.popup.sessionDirectory;
    windowClass = cfg.voice.popup.class;
    windowInstance = cfg.voice.popup.instance;
    initialTitle = cfg.voice.popup.initialTitle;
  };
in {
  options.programs.scufris = {
    enable = lib.mkEnableOption "Scufris Pi launcher";

    piPackage = lib.mkOption {
      type = lib.types.package;
      default = defaults.piPackage;
      defaultText = lib.literalExpression "inputs.llm-agents.packages.${system}.pi";
      description = "Pi package used by the Scufris launcher.";
    };

    finalPackage = lib.mkOption {
      type = lib.types.package;
      readOnly = true;
      description = "Rendered Scufris launcher package for desktop consumers.";
    };

    projectRoots = lib.mkOption {
      type = lib.types.listOf lib.types.str;
      default = ["~/personal" "~/work" "~/third-party"];
      description = "Directories recursively searched for workflow projects.";
    };

    dashboard = {
      enable =
        lib.mkEnableOption "dashboardd widget control"
        // {
          default = true;
        };

      dashboardctlPackage = lib.mkOption {
        type = lib.types.package;
        default = defaults.dashboardctlPackage;
        defaultText = lib.literalExpression "inputs.dashboardd.packages.${system}.dashboardd-desktop";
        description = "Package that provides dashboardctl.";
      };
    };

    voice = {
      enable = lib.mkEnableOption "local Piper speech";

      piper = {
        package = lib.mkOption {
          type = lib.types.package;
          default = voiceRuntime.piperPackage;
          defaultText = lib.literalExpression "Scufris private patched Piper 1.4.2";
          description = "Trusted Piper 1.4.2 package used only by voice-enabled Scufris.";
        };
        model = lib.mkOption {
          type = lib.types.pathInStore;
          default = voiceRuntime.model;
          defaultText = lib.literalExpression "the pinned en_US-lessac-medium model";
          description = "Trusted immutable Piper ONNX model path.";
        };
        config = lib.mkOption {
          type = lib.types.pathInStore;
          default = voiceRuntime.config;
          defaultText = lib.literalExpression "the pinned adjacent en_US-lessac-medium config";
          description = "Trusted immutable Piper model configuration path.";
        };
      };

      popup = {
        enable = lib.mkEnableOption "direct resumable Kitty voice popup";

        sessionDirectory = lib.mkOption {
          type = lib.types.strMatching "/.*";
          default = "${config.home.homeDirectory}/.local/share/scufris-popup/sessions";
          description = "Absolute directory for the dedicated popup Pi sessions.";
        };
        terminalPackage = lib.mkOption {
          type = lib.types.package;
          default = pkgs.kitty;
          defaultText = lib.literalExpression "pkgs.kitty";
          description = "Kitty-compatible terminal package used by the popup launcher.";
        };
        class = lib.mkOption {
          type = lib.types.strMatching "[A-Za-z0-9_-]+";
          default = "Scufris";
          description = "Stable popup Kitty class.";
        };
        instance = lib.mkOption {
          type = lib.types.strMatching "[A-Za-z0-9_-]+";
          default = "scufris-popup";
          description = "Stable popup Kitty instance.";
        };
        initialTitle = lib.mkOption {
          type = lib.types.strMatching "[A-Za-z0-9 _-]+";
          default = "Scufris";
          description = "Initial popup Kitty title. Runtime title changes do not affect identity.";
        };
        serviceName = lib.mkOption {
          type = lib.types.str;
          default = "scufris-popup";
          readOnly = true;
          description = "Stable systemd user service identity for desktop consumers, without the unit suffix.";
        };
        finalLauncher = lib.mkOption {
          type = lib.types.package;
          readOnly = true;
          description = "Rendered direct popup launcher for desktop consumers when the popup is enabled.";
        };
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

      hotkey = lib.mkOption {
        type = lib.types.strMatching "[A-Za-z0-9+]+";
        default = "Super+D";
        description = "Accelerator that opens the pill and starts recording.";
      };

      chatCommand = lib.mkOption {
        type = lib.types.nullOr lib.types.package;
        default = null;
        description = ''
          Executable that opens the full popup chat from the tray. Scufris ships
          no window manager, so the desktop session supplies this hook.
        '';
      };

      stt = {
        endpoint = lib.mkOption {
          type = lib.types.nullOr (lib.types.strMatching "https?://.*");
          default = null;
          description = ''
            whisper-server-compatible transcription endpoint. When null the
            bundled loopback whisper-server provides one.
          '';
        };

        whisper = {
          enable = lib.mkOption {
            type = lib.types.bool;
            default = desktopCfg.stt.endpoint == null;
            defaultText = lib.literalExpression "programs.scufris.desktop.stt.endpoint == null";
            description = "Run the bundled loopback whisper-server for the companion.";
          };

          package = lib.mkOption {
            type = lib.types.package;
            default = whisperRuntime.package;
            defaultText = lib.literalExpression "pkgs.whisper-cpp";
            description = "whisper.cpp package that provides whisper-server.";
          };

          model = lib.mkOption {
            type = lib.types.package;
            default = whisperRuntime.model;
            defaultText = lib.literalExpression "the pinned ggml-base model";
            description = "Pinned whisper.cpp GGML model.";
          };

          host = lib.mkOption {
            type = lib.types.enum ["127.0.0.1"];
            default = whisperRuntime.host;
            description = "Loopback address for the bundled whisper-server.";
          };

          port = lib.mkOption {
            type = lib.types.port;
            default = whisperRuntime.port;
            description = "Loopback port for the bundled whisper-server.";
          };

          serviceName = lib.mkOption {
            type = lib.types.str;
            default = "scufris-whisper";
            readOnly = true;
            description = "Stable systemd user service identity for the bundled whisper-server.";
          };
        };
      };

      endpoint = lib.mkOption {
        type = lib.types.str;
        readOnly = true;
        description = "Transcription endpoint the companion actually uses.";
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
      programs.scufris = {
        finalPackage = launcher;
        desktop = {
          endpoint = resolvedEndpoint;
          restartCommand = backendRestart;
        };
      };
    }
    (lib.mkIf cfg.enable {
      assertions = [
        {
          assertion = !cfg.voice.popup.enable || cfg.voice.enable;
          message = "programs.scufris.voice.popup.enable requires programs.scufris.voice.enable";
        }
        {
          assertion = !cfg.voice.enable || (cfg.voice.piper.package.version or null) == "1.4.2";
          message = "programs.scufris.voice requires Piper 1.4.2";
        }
        {
          assertion = !cfg.voice.enable || toString cfg.voice.piper.config == "${toString cfg.voice.piper.model}.json";
          message = "programs.scufris.voice requires the Piper config adjacent to the model as model.onnx.json";
        }
      ];

      home.packages = [cfg.finalPackage];
    })
    (lib.mkIf (cfg.enable && cfg.voice.enable) {
      assertions = [
        (lib.hm.assertions.assertPlatform "programs.scufris.voice" pkgs lib.platforms.linux)
      ];
    })
    (lib.mkIf (cfg.enable && cfg.voice.enable && cfg.voice.popup.enable) {
      assertions = [
        (lib.hm.assertions.assertPlatform "programs.scufris.voice.popup" pkgs lib.platforms.linux)
      ];
      programs.scufris.voice.popup.finalLauncher = popupLauncher;
      systemd.user.services.${cfg.voice.popup.serviceName} = {
        Unit.Description = "Direct Scufris Kitty voice popup";
        Service = {
          Type = "simple";
          ExecStart = lib.getExe popupLauncher;
          Restart = "no";
          WorkingDirectory = "%h";
        };
      };
    })
    (lib.mkIf (cfg.enable && desktopCfg.enable) {
      assertions = [
        (lib.hm.assertions.assertPlatform "programs.scufris.desktop" pkgs lib.platforms.linux)
        {
          assertion = cfg.voice.popup.enable;
          message = "programs.scufris.desktop.enable requires programs.scufris.voice.popup.enable, because the popup Pi process serves the control socket";
        }
        {
          assertion = !(desktopCfg.stt.endpoint != null && whisperCfg.enable);
          message = "programs.scufris.desktop.stt.endpoint conflicts with programs.scufris.desktop.stt.whisper.enable";
        }
      ];

      home.packages = [desktopCfg.package];

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
            ++ lib.optional (desktopCfg.chatCommand != null)
            "SCUFRIS_DESKTOP_CHAT_COMMAND=${lib.getExe desktopCfg.chatCommand}";
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
    (lib.mkIf (cfg.enable && desktopCfg.enable && whisperCfg.enable) {
      systemd.user.services.${whisperCfg.serviceName} = {
        Unit = {
          Description = "Bundled loopback whisper.cpp server for Scufris";
          After = ["network.target"];
        };
        Service = {
          Type = "simple";
          ExecStart = lib.escapeShellArgs [
            (lib.getExe' whisperCfg.package "whisper-server")
            "--model"
            (toString whisperCfg.model)
            "--host"
            whisperCfg.host
            "--port"
            (toString whisperCfg.port)
            "--inference-path"
            whisperRuntime.inferencePath
            "--language"
            "auto"
          ];
          Restart = "no";
          RuntimeDirectory = whisperCfg.serviceName;
          WorkingDirectory = "%t/${whisperCfg.serviceName}";
          NoNewPrivileges = true;
          PrivateTmp = true;
          ProtectSystem = "strict";
        };
        Install.WantedBy = ["default.target"];
      };
    })
  ];
}
