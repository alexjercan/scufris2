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
  serviceCfg = cfg.service;
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
      exec systemctl --user restart ${lib.escapeShellArg "${serviceCfg.serviceName}.service"}
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
    voice = cfg.voice.enable;
    projectRoots = cfg.projectRoots;
  };
  # The frontend owns the speaker, so the synthesiser is bound here and handed
  # to the companion. A deployment with no voice hands it nothing and the
  # companion stays silent.
  speak = import ./speak.nix {
    inherit pkgs;
    piperPackage = cfg.voice.piper.package;
    piperModel = cfg.voice.piper.model;
    piperConfig = cfg.voice.piper.config;
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
    };

    service = {
      enable = lib.mkEnableOption "the headless scufris-service background service";

      package = lib.mkOption {
        type = lib.types.package;
        default = defaults.servicePackage;
        defaultText = lib.literalExpression "self.packages.\${system}.scufris-service";
        description = "scufris-service package.";
      };

      agentPackage = lib.mkOption {
        type = lib.types.package;
        default = cfg.finalPackage;
        defaultText = lib.literalExpression "programs.scufris.finalPackage";
        description = ''
          Launcher the service runs as its one Pi agent. The service starts it
          in RPC mode, and `scufris-ctl debug` hands a terminal the same
          session, so there is one Scufris rather than one per surface.
        '';
      };

      sessionDirectory = lib.mkOption {
        type = lib.types.strMatching "/.*";
        default = "${config.xdg.dataHome}/scufris/sessions";
        defaultText = lib.literalExpression "\"\${config.xdg.dataHome}/scufris/sessions\"";
        description = ''
          Absolute directory the service keeps its conversation in. The service
          owns it, and `scufris-ctl debug` hands a terminal the same session
          out of it.
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

      hotkey = lib.mkOption {
        type = lib.types.strMatching "[A-Za-z0-9+]+";
        default = "Super+D";
        description = "Accelerator that opens the pill and starts recording.";
      };

      chatCommand = lib.mkOption {
        type = lib.types.nullOr lib.types.package;
        default = null;
        description = ''
          Executable that opens the conversation in a terminal from the tray.
          Scufris ships no window manager, so the desktop session supplies this
          hook; `scufris-ctl debug` is what it is usually wrapped around.
        '';
      };

      modeCommand = lib.mkOption {
        type = lib.types.nullOr lib.types.package;
        default = null;
        description = ''
          Executable that puts the window manager into one named binding mode,
          run with the mode name as its only argument. The companion asks for
          "scufris" while the pill wants a bare Escape or Return, and "default"
          the rest of the time. On i3 this wraps `i3-msg mode "$1"`, on sway
          `swaymsg mode "$1"`; a session with no binding modes leaves it null
          and answers the pill with the modified accelerators instead.
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
          Environment =
            [
              "SCUFRIS_SERVICE_AGENT=${lib.getExe serviceCfg.agentPackage}"
              "SCUFRIS_SERVICE_SESSION_DIR=${serviceCfg.sessionDirectory}"
            ]
            # Inherited by the agent, which decides what is worth saying aloud.
            # Saying it is the frontend's, and a session with no frontend
            # simply has nowhere for the paragraph to go.
            ++ lib.optional cfg.voice.enable "SCUFRIS_SPEECH=1";
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
          assertion = !(desktopCfg.stt.endpoint != null && whisperCfg.enable);
          message = "programs.scufris.desktop.stt.endpoint conflicts with programs.scufris.desktop.stt.whisper.enable";
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
            ++ lib.optional (desktopCfg.chatCommand != null)
            "SCUFRIS_DESKTOP_CHAT_COMMAND=${lib.getExe desktopCfg.chatCommand}"
            ++ lib.optional (desktopCfg.modeCommand != null)
            "SCUFRIS_DESKTOP_MODE_COMMAND=${lib.getExe desktopCfg.modeCommand}"
            ++ lib.optional cfg.voice.enable
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
