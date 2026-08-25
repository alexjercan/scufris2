{
  resourcesFor,
  voiceResourcesFor,
  piPackageFor,
  dashboardctlPackageFor,
}: {
  config,
  lib,
  pkgs,
  ...
}: let
  cfg = config.programs.scufris;
  system = pkgs.stdenv.hostPlatform.system;
  voiceRuntime = import ./voice.nix {inherit pkgs;};
  launcher = import ./launcher.nix {
    inherit pkgs;
    resources =
      if cfg.voice.enable
      then voiceResourcesFor system
      else resourcesFor system;
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
      default = piPackageFor system;
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
        default = dashboardctlPackageFor system;
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
  };

  config = lib.mkMerge [
    {
      programs.scufris.finalPackage = launcher;
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
  ];
}
