{
  resourcesFor,
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
  launcher = import ./launcher.nix {
    inherit pkgs;
    resources = resourcesFor system;
    piPackage = cfg.piPackage;
    dashboardctlPackage = cfg.widgets.dashboardctlPackage;
    delegation = cfg.delegation.enable;
    widgets = cfg.widgets.enable;
    projectRoots = cfg.projectRoots;
  };
in {
  options.programs.scufris = {
    enable = lib.mkEnableOption "Scufris Pi launcher";

    piPackage = lib.mkOption {
      type = lib.types.package;
      default = piPackageFor system;
      defaultText = lib.literalExpression "inputs.pi.packages.${system}.default";
      description = "Pi package used by the Scufris launcher.";
    };

    delegation.enable =
      lib.mkEnableOption "delegated Pi and Claude workers"
      // {
        default = true;
      };

    projectRoots = lib.mkOption {
      type = lib.types.listOf lib.types.str;
      default = ["~/personal" "~/work" "~/third-party"];
      description = "Directories recursively searched for delegation projects.";
    };

    widgets = {
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
  };

  config = lib.mkIf cfg.enable {
    home.packages = [launcher];
  };
}
