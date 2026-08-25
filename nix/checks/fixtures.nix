# Stubs shared by the check groups. Each one prints the environment and
# arguments it received so a check can assert the exact rendered command.
{pkgs}: {
  systemPi = pkgs.writeShellScriptBin "pi" ''
    printf '%s\n' \
      "''${SCUFRIS_PROJECT_ROOTS-}" \
      "''${SCUFRIS_ROLE-}" \
      "''${SCUFRIS_SPEECH-}" \
      "''${SCUFRIS_CALM-}" \
      "''${SCUFRIS_PIPER_MODEL-}" \
      "''${SCUFRIS_PIPER_CONFIG-}" \
      "$(type -P piper || true)" \
      "$(type -P pw-play || true)" \
      system-pi \
      "$@"
  '';

  testTerminal = pkgs.writeShellScriptBin "kitty" ''
    printf '%s\n' \
      "speech=''${SCUFRIS_SPEECH-}" \
      "calm=''${SCUFRIS_CALM-}" \
      "daemon=''${SCUFRIS_DAEMON-}" \
      "model=''${SCUFRIS_PIPER_MODEL-}" \
      "config=''${SCUFRIS_PIPER_CONFIG-}" \
      "stt=''${PI_STT_CONFIG-}"
    printf 'arg=%s\n' "$@"
  '';

  fakePlayer = pkgs.writeShellScriptBin "pw-play" ''
    test "$#" -eq 1
    test "$1" = -
    cat > "$SCUFRIS_FIXTURE_WAV"
  '';
}
