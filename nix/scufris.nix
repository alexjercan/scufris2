# Every Scufris component built for one system. `flake.nix` selects which of
# these become flake outputs; the checks assert their composition.
{
  inputs,
  self,
  pkgs,
  system,
}: let
  version = (builtins.fromJSON (builtins.readFile ../package.json)).version;
  voice = import ./voice.nix {inherit pkgs;};
  resources = import ./resources.nix {inherit pkgs;};
  piPackage = inputs.llm-agents.packages.${system}.pi;
  # One launcher. There used to be two, and the voice one differed only in
  # shipping a speech module and setting a variable for it. Both are gone: the
  # agent shapes the answer and never decides to make a sound.
  launcher = import ./launcher.nix {
    inherit pkgs resources piPackage;
  };
  # The synthesiser belongs to whoever is sitting in front of the machine, so
  # it is its own program rather than part of the agent's launcher.
  speak = import ./speak.nix {
    inherit pkgs;
    inherit (voice) piperPackage;
    piperModel = voice.model;
    piperConfig = voice.config;
  };
  desktop = import ./desktop.nix {
    inherit pkgs version;
    source = ../native;
    lockFile = ../native/Cargo.lock;
  };
  native = import ./service.nix {
    inherit pkgs version;
    source = ../native;
    lockFile = ../native/Cargo.lock;
  };
  service = native.service;
  ctl = native.ctl;
  staging = import ./staging.nix {
    inherit pkgs self service desktop speak;
  };
  devShell = import ./dev-shell.nix {inherit pkgs voice;};
  docs = import ./docs.nix {inherit inputs self pkgs;};
in {
  inherit
    voice
    resources
    piPackage
    launcher
    speak
    desktop
    service
    ctl
    staging
    devShell
    docs
    ;
}
