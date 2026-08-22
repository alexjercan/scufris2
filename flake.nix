{
  description = "Scufris personal assistant for Pi";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";

    home-manager = {
      url = "github:nix-community/home-manager";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    pi = {
      url = "github:lukasl-dev/pi.nix";
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
        "x86_64-darwin"
        "aarch64-darwin"
      ];

      perSystem = {
        pkgs,
        system,
        ...
      }: let
        inherit (pkgs) lib;
        mkResources = voice:
          pkgs.runCommand "scufris-${lib.optionalString voice "voice-"}resources" {} ''
            mkdir -p "$out/share/scufris"
            cp -R ${./extensions} "$out/share/scufris/extensions"
            cp -R ${./scripts} "$out/share/scufris/scripts"
            cp -R ${./skills} "$out/share/scufris/skills"
            chmod -R u+w "$out/share/scufris"
            rm "$out/share/scufris/scripts/scufris-dev"
            ${lib.optionalString (!voice) ''
              rm "$out/share/scufris/extensions/scufris/speech.ts"
              rm "$out/share/scufris/scripts/scufris-speak"
            ''}
          '';
        resources = mkResources false;
        voiceResources = mkResources true;
        voice = import ./nix/voice.nix {inherit pkgs;};
        docs = import ./nix/docs.nix {
          inherit inputs self pkgs;
        };
        launcher = import ./nix/launcher.nix {
          inherit pkgs resources;
          piPackage = inputs.pi.packages.${system}.default;
          dashboardctlPackage = inputs.dashboardd.packages.${system}.dashboardd-desktop;
        };
        voiceLauncher = import ./nix/launcher.nix {
          inherit pkgs;
          resources = voiceResources;
          piPackage = inputs.pi.packages.${system}.default;
          dashboardctlPackage = inputs.dashboardd.packages.${system}.dashboardd-desktop;
          voice = true;
          inherit (voice) piperPackage;
          piperModel = voice.model;
          piperConfig = voice.config;
        };
        devShell = pkgs.mkShell (
          {
            packages =
              (with pkgs; [
                bashInteractive
                git
                alejandra
                mdbook
                nodejs_22
                python3
                ruff
                shellcheck
                tmux
              ])
              ++ lib.optionals pkgs.stdenv.hostPlatform.isLinux [voice.piperPackage pkgs.pipewire];
          }
          // lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
            SCUFRIS_DEV_VOICE = "1";
            SCUFRIS_PIPER_MODEL = voice.model;
            SCUFRIS_PIPER_CONFIG = voice.config;
          }
        );
      in {
        formatter = pkgs.alejandra;

        packages =
          {
            default = launcher;
            scufris = launcher;
            inherit docs resources;
            voice-resources = voiceResources;
          }
          // lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
            scufris-voice = voiceLauncher;
          };

        apps =
          {
            default = {
              type = "app";
              program = "${launcher}/bin/scufris";
              meta.description = "Run Pi with Scufris";
            };
          }
          // lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
            scufris-voice = {
              type = "app";
              program = "${voiceLauncher}/bin/scufris";
              meta.description = "Run voice-capable Scufris without enabling speech by default";
            };
          };

        checks =
          import ./nix/checks.nix {
            inherit inputs self pkgs system resources voiceResources launcher voiceLauncher voice devShell;
          }
          // {inherit docs;};

        devShells.default = devShell;
      };

      flake = {
        extensions = {
          response = builtins.path {
            path = ./extensions/scufris/response.ts;
            name = "scufris-response-extension";
          };
          calm = builtins.path {
            path = ./extensions/scufris/calm.ts;
            name = "scufris-calm-extension";
          };
          speech = builtins.path {
            path = ./extensions/scufris/speech.ts;
            name = "scufris-speech-extension";
          };
          delegation = builtins.path {
            path = ./extensions/scufris/agents.ts;
            name = "scufris-delegation-extension";
          };
          widgets = builtins.path {
            path = ./extensions/scufris/widgets.ts;
            name = "scufris-widgets-extension";
          };
        };

        skills = {
          delegation = builtins.path {
            path = ./skills/delegation;
            name = "scufris-delegation-skill";
          };
          widgets = builtins.path {
            path = ./skills/widgets;
            name = "scufris-widgets-skill";
          };
        };

        homeModules.default = import ./nix/home-manager.nix {
          resourcesFor = system: self.packages.${system}.resources;
          voiceResourcesFor = system: self.packages.${system}.voice-resources;
          piPackageFor = system: inputs.pi.packages.${system}.default;
          dashboardctlPackageFor = system: inputs.dashboardd.packages.${system}.dashboardd-desktop;
        };
      };
    };
}
