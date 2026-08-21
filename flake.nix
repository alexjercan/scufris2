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
        resources = pkgs.runCommand "scufris-resources" {} ''
          mkdir -p "$out/share/scufris"
          cp -R ${./extensions} "$out/share/scufris/extensions"
          cp -R ${./scripts} "$out/share/scufris/scripts"
          cp -R ${./skills} "$out/share/scufris/skills"
          cp -R ${./prompts} "$out/share/scufris/prompts"
        '';
        launcher = import ./nix/launcher.nix {
          inherit pkgs resources;
          piPackage = inputs.pi.packages.${system}.default;
          dashboardctlPackage = inputs.dashboardd.packages.${system}.dashboardd-desktop;
        };
      in {
        formatter = pkgs.alejandra;

        packages = {
          default = launcher;
          scufris = launcher;
          inherit resources;
        };

        apps.default = {
          type = "app";
          program = "${launcher}/bin/scufris";
          meta.description = "Run Pi with all Scufris extensions";
        };

        checks = {
          launcher-system-pi = let
            systemPi = pkgs.writeShellScriptBin "pi" ''
              printf '%s\n' "$SCUFRIS_PROJECT_ROOTS" "$SCUFRIS_FOREGROUND" system-pi "$@"
            '';
          in
            pkgs.runCommand "scufris-launcher-system-pi-check" {
              nativeBuildInputs = [launcher systemPi];
            } ''
              scufris user-argument > actual
              cat > expected <<'EOF'
              ["~/personal","~/work","~/third-party"]
              1
              system-pi
              --extension
              ${resources}/share/scufris/extensions/scufris/identity.ts
              --extension
              ${resources}/share/scufris/extensions/scufris/calm.ts
              --extension
              ${resources}/share/scufris/extensions/scufris/agents.ts
              --skill
              ${resources}/share/scufris/skills/delegation
              --extension
              ${resources}/share/scufris/extensions/scufris/widgets.ts
              --skill
              ${resources}/share/scufris/skills/widgets
              user-argument
              EOF
              diff -u expected actual
              touch "$out"
            '';

          launcher-fallback-pi = pkgs.runCommand "scufris-launcher-fallback-pi-check" {} ''
            export HOME="$TMPDIR/home"
            mkdir -p "$HOME"
            expected="$(${inputs.pi.packages.${system}.default}/bin/pi --version)"
            actual="$(PATH=/nonexistent ${launcher}/bin/scufris --version)"
            test "$actual" = "$expected"
            touch "$out"
          '';

          resources = pkgs.runCommand "scufris-resources-check" {} ''
            test -f ${resources}/share/scufris/extensions/scufris/identity.ts
            test -f ${resources}/share/scufris/extensions/scufris/calm.ts
            test -f ${resources}/share/scufris/prompts/pair.md
            test -f ${resources}/share/scufris/extensions/scufris/agents.ts
            test -f ${resources}/share/scufris/extensions/scufris/widgets.ts
            test -x ${resources}/share/scufris/scripts/scufris-job
            test -x ${resources}/share/scufris/scripts/scufris-jobs
            test -x ${resources}/share/scufris/scripts/scufris-dashboard
            test -f ${resources}/share/scufris/skills/delegation/SKILL.md
            test -f ${resources}/share/scufris/skills/widgets/SKILL.md
            touch "$out"
          '';

          home-module =
            (inputs.home-manager.lib.homeManagerConfiguration {
              inherit pkgs;
              modules = [
                self.homeModules.default
                {
                  home = {
                    username = "scufris-test";
                    homeDirectory = "/home/scufris-test";
                    stateVersion = "25.05";
                  };
                  programs.scufris = {
                    enable = true;
                    widgets.enable = false;
                  };
                }
              ];
            }).activationPackage;
        };

        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            bashInteractive
            git
            alejandra
            nodejs_22
            python3
            ruff
            shellcheck
            tmux
          ];
        };
      };

      flake = {
        extensions = {
          calm = builtins.path {
            path = ./extensions/scufris/calm.ts;
            name = "scufris-calm-extension";
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
          piPackageFor = system: inputs.pi.packages.${system}.default;
          dashboardctlPackageFor = system: inputs.dashboardd.packages.${system}.dashboardd-desktop;
        };
      };
    };
}
