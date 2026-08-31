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

    ai-tools-api = {
      url = "github:alexjercan/ai-tools-api/v0.1.1";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.flake-parts.follows = "flake-parts";
      inputs.home-manager.follows = "home-manager";
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
            scufris-den = scufris.den;
            inherit (scufris) docs resources;
          }
          // lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
            ai-tools-api = scufris.aiToolsApi;
            scufris-speak = scufris.speak;
            scufris-desktop = scufris.desktop;
            scufris-service = scufris.service;
            scufris-ctl = scufris.ctl;
            scufris-staging = scufris.staging;
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
            scufris-desktop = {
              type = "app";
              program = "${scufris.desktop}/bin/scufris-desktop";
              meta.description = "Run the Scufris voice pill and tray companion";
            };
            scufris-service = {
              type = "app";
              program = "${scufris.service}/bin/scufris-service";
              meta.description = "Run the Scufris background service in the foreground";
            };
            scufris-ctl = {
              type = "app";
              program = "${scufris.ctl}/bin/scufris-ctl";
              meta.description = "Talk to Scufris from a terminal";
            };
            ai-tools-api = {
              type = "app";
              program = "${scufris.aiToolsApi}/bin/ai-tools-api";
              meta.description = "Run the pinned shared speech inference API";
            };
            staging = {
              type = "app";
              program = "${scufris.staging}/bin/scufris-staging";
              meta.description = "Run this source tree's Scufris beside the deployed one";
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
            piPackage = inputs.llm-agents.packages.${system}.pi;
            denPackage = self.packages.${system}.scufris-den;
            desktopPackage = self.packages.${system}.scufris-desktop;
            servicePackage = self.packages.${system}.scufris-service;
            ctlPackage = self.packages.${system}.scufris-ctl;
            aiToolsApiPackage = inputs.ai-tools-api.packages.${system}.ai-tools-api;
          };
        };
      };
    };
}
