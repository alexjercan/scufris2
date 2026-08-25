{
  pkgs,
  voice,
}: let
  inherit (pkgs) lib;
in
  pkgs.mkShell (
    {
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
          voice.piperPackage
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
    // lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
      SCUFRIS_DEV_VOICE = "1";
      SCUFRIS_PIPER_MODEL = voice.model;
      SCUFRIS_PIPER_CONFIG = voice.config;
    }
  )
