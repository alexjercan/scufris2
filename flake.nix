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
          }
          // lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
            scufris-speak = scufris.speak;
            scufris-desktop = scufris.desktop;
            scufris-service = scufris.service;
            scufris-ctl = scufris.ctl;
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
            desktopPackage = self.packages.${system}.scufris-desktop;
            servicePackage = self.packages.${system}.scufris-service;
            ctlPackage = self.packages.${system}.scufris-ctl;
          };
        };
      };
    };
}
