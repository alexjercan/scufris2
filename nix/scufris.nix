# Every Scufris component built for one system. `flake.nix` selects which of
# these become flake outputs; the checks assert their composition.
{
  inputs,
  self,
  pkgs,
  system,
}: let
  voice = import ./voice.nix {inherit pkgs;};
  resources = import ./resources.nix {inherit pkgs;};
  voiceResources = import ./resources.nix {
    inherit pkgs;
    voice = true;
  };
  piPackage = inputs.llm-agents.packages.${system}.pi;
  dashboardctlPackage = inputs.dashboardd.packages.${system}.dashboardd-desktop;
  launcher = import ./launcher.nix {
    inherit pkgs resources piPackage dashboardctlPackage;
  };
  voiceLauncher = import ./launcher.nix {
    inherit pkgs piPackage dashboardctlPackage;
    resources = voiceResources;
    voice = true;
    inherit (voice) piperPackage;
    piperModel = voice.model;
    piperConfig = voice.config;
  };
  devShell = import ./dev-shell.nix {inherit pkgs voice;};
  docs = import ./docs.nix {inherit inputs self pkgs;};
in {
  inherit
    voice
    resources
    voiceResources
    piPackage
    dashboardctlPackage
    launcher
    voiceLauncher
    devShell
    docs
    ;
}
