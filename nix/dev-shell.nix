{pkgs}: let
  inherit (pkgs) lib;
in
  pkgs.mkShell {
    packages =
      (with pkgs; [
        bashInteractive
        git
        alejandra
        mdbook
        nodejs_22
        python3
        typescript
        ruff
        shellcheck
        tmux
      ])
      ++ lib.optionals pkgs.stdenv.hostPlatform.isLinux [
        pkgs.pipewire
        pkgs.cargo
        pkgs.clippy
        pkgs.pkg-config
        pkgs.rustc
        pkgs.rustfmt
      ];
    buildInputs = lib.optionals pkgs.stdenv.hostPlatform.isLinux [
      pkgs.alsa-lib
      pkgs.glib
      pkgs.gtk3
      pkgs.libayatana-appindicator
      pkgs.librsvg
      pkgs.openssl
      pkgs.webkitgtk_4_1
    ];
  }
