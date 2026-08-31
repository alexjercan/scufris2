# Every Scufris component built for one system. `flake.nix` selects which of
# these become flake outputs; the checks assert their composition.
{
  inputs,
  self,
  pkgs,
  system,
}: let
  inherit (pkgs) lib;
  version = (builtins.fromJSON (builtins.readFile ../package.json)).version;
  rustSource = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [
      ../Cargo.toml
      ../Cargo.lock
      ../host
      ../shared
      ../surfaces
      # The den library the widget backend is compiled with. `build.rs` reads
      # the paths named in surfaces/desktop/backends/den/prelude, so a source
      # tree without it builds a backend missing half of itself.
      ../tools/den
    ];
  };
  resources = import ./resources.nix {inherit pkgs;};
  aiToolsApi = inputs.ai-tools-api.packages.${system}.ai-tools-api;
  piPackage = inputs.llm-agents.packages.${system}.pi;
  # One launcher. There used to be two, and the voice one differed only in
  # shipping a speech module and setting a variable for it. Both are gone: the
  # agent shapes the answer and never decides to make a sound.
  launcher = import ./launcher.nix {
    inherit pkgs resources piPackage den briefing;
  };
  # The synthesiser belongs to whoever is sitting in front of the machine, so
  # it is its own program rather than part of the agent's launcher.
  speak = import ./speak.nix {inherit pkgs;};
  # The journal's command line. The panels compile the library in; this is the
  # same library for whoever is not a panel.
  den = import ./den.nix {inherit pkgs;};
  # The morning, collected and rendered. The agent runs it through the briefing
  # extension; this is the same program for a person and for the checks.
  briefing = import ./briefing.nix {inherit pkgs;};
  desktop = import ./desktop.nix {
    inherit pkgs version;
    source = rustSource;
    lockFile = ../Cargo.lock;
  };
  headless = import ./service.nix {
    inherit pkgs version;
    source = rustSource;
    lockFile = ../Cargo.lock;
  };
  service = headless.service;
  ctl = headless.ctl;
  staging = import ./staging.nix {
    inherit pkgs self service desktop speak aiToolsApi;
  };
  devShell = import ./dev-shell.nix {inherit pkgs;};
  docs = import ./docs.nix {inherit inputs self pkgs;};
in {
  inherit
    rustSource
    aiToolsApi
    resources
    piPackage
    launcher
    speak
    den
    briefing
    desktop
    service
    ctl
    staging
    devShell
    docs
    ;
}
