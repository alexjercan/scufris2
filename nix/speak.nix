{
  pkgs,
  endpoint ? "http://127.0.0.1:10300/v1/audio/speech",
}: let
  inherit (pkgs) lib;
in
  pkgs.writeShellApplication {
    name = "scufris-speak";
    # Synthesis belongs to ai-tools-api. This frontend helper keeps only the
    # bounded HTTP adapter and local PipeWire playback in its process tree.
    runtimeInputs = [pkgs.python3 pkgs.pipewire];
    text = ''
      : "''${SCUFRIS_TTS_ENDPOINT:=${lib.escapeShellArg endpoint}}"
      export SCUFRIS_TTS_ENDPOINT
      exec python3 ${../tools/voice/scufris-speak} "$@"
    '';
    meta = {
      description = "Synthesise one stdin paragraph through ai-tools-api and play it locally";
      mainProgram = "scufris-speak";
    };
  }
