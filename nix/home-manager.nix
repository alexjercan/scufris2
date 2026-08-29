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
  agentCfg = serviceCfg.agent;
  desktopCfg = cfg.desktop;
  speechCfg = desktopCfg.speech;
  transcriptionCfg = desktopCfg.transcription;
  whisperCfg = transcriptionCfg.whisper;
  bundledEndpoint = "http://${whisperCfg.host}:${toString whisperCfg.port}${whisperRuntime.inferencePath}";
  resolvedEndpoint =
    if transcriptionCfg.endpoint != null
    then transcriptionCfg.endpoint
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
    resources = defaults.resources;
    piPackage = agentCfg.piPackage;
    projectRoots = agentCfg.projectRoots;
  };
  # The frontend owns the speaker, so the synthesiser is bound here and handed
  # to the companion. A deployment with no speech hands it nothing and the
  # companion stays silent.
  speak = import ./speak.nix {
    inherit pkgs;
    piperPackage = speechCfg.piper.package;
    piperModel = speechCfg.piper.model;
    piperConfig = speechCfg.piper.config;
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
            in RPC mode, and `scufris-ctl debug` hands a terminal the same
            session, so there is one Scufris rather than one per surface.
          '';
        };
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

      speech = {
        enable = lib.mkEnableOption "local Piper speech from the desktop companion";

        piper = {
          package = lib.mkOption {
            type = lib.types.package;
            default = voiceRuntime.piperPackage;
            defaultText = lib.literalExpression "Scufris private patched Piper 1.4.2";
            description = "Trusted Piper 1.4.2 package used only by speech-enabled Scufris.";
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
          Set it to `"none"` to leave the key to the desktop; `scufris-ctl
          abort` stops a run without it.

          The companion holds this key only while the pill is on screen.
        '';
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
            whisper-server-compatible transcription endpoint. When null the
            bundled loopback whisper-server provides one.
          '';
        };

        whisper = {
          enable = lib.mkOption {
            type = lib.types.bool;
            default = transcriptionCfg.endpoint == null;
            defaultText = lib.literalExpression "programs.scufris.desktop.transcription.endpoint == null";
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
      assertions = [
        {
          assertion = !speechCfg.enable || (speechCfg.piper.package.version or null) == "1.4.2";
          message = "programs.scufris.desktop.speech requires Piper 1.4.2";
        }
        {
          assertion = !speechCfg.enable || toString speechCfg.piper.config == "${toString speechCfg.piper.model}.json";
          message = "programs.scufris.desktop.speech requires the Piper config adjacent to the model as model.onnx.json";
        }
      ];

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
          assertion = !(transcriptionCfg.endpoint != null && whisperCfg.enable);
          message = "programs.scufris.desktop.transcription.endpoint conflicts with programs.scufris.desktop.transcription.whisper.enable";
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
