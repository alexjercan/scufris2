{
  pkgs,
  endpoint ? "http://127.0.0.1:10300/v1/audio/speech",
  model ? "piper-1",
  voice ? "en_US-lessac-medium",
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
      : "''${SCUFRIS_TTS_MODEL:=${lib.escapeShellArg model}}"
      : "''${SCUFRIS_TTS_VOICE:=${lib.escapeShellArg voice}}"
      export SCUFRIS_TTS_ENDPOINT SCUFRIS_TTS_MODEL SCUFRIS_TTS_VOICE
      exec python3 ${../tools/voice/scufris-speak} "$@"
    '';
    meta = {
      description = "Synthesise one stdin paragraph through ai-tools-api and play it locally";
      mainProgram = "scufris-speak";
    };
  }
