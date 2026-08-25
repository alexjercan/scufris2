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
  )
