{
  description = "Scufris personal assistant for Pi";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";

    home-manager = {
      url = "github:nix-community/home-manager";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    llm-agents = {
      url = "github:numtide/llm-agents.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    dashboardd = {
      url = "github:alexjercan/dashboardd/v0.2.0";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = inputs @ {
    self,
    flake-parts,
    ...
  }:
    flake-parts.lib.mkFlake {inherit inputs;} {
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];

      perSystem = {
        pkgs,
        system,
        ...
      }: let
        inherit (pkgs) lib;
        scufris = import ./nix/scufris.nix {inherit inputs self pkgs system;};
      in {
        formatter = pkgs.alejandra;

        packages =
          {
            default = scufris.launcher;
            scufris = scufris.launcher;
            inherit (scufris) docs resources;
            voice-resources = scufris.voiceResources;
          }
          // lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
            scufris-voice = scufris.voiceLauncher;
          };

        apps =
          {
            default = {
              type = "app";
              program = "${scufris.launcher}/bin/scufris";
              meta.description = "Run Pi with Scufris";
            };
          }
          // lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
            scufris-voice = {
              type = "app";
              program = "${scufris.voiceLauncher}/bin/scufris";
              meta.description = "Run voice-capable Scufris without enabling speech by default";
            };
          };

        checks =
          import ./nix/checks {inherit inputs self pkgs scufris;}
          // {inherit (scufris) docs;};

        devShells.default = scufris.devShell;
      };

      flake = {
        homeModules.default = import ./nix/home-manager.nix {
          defaultsFor = system: {
            resources = self.packages.${system}.resources;
            voiceResources = self.packages.${system}.voice-resources;
            piPackage = inputs.llm-agents.packages.${system}.pi;
            dashboardctlPackage = inputs.dashboardd.packages.${system}.dashboardd-desktop;
          };
        };
      };
    };
}
