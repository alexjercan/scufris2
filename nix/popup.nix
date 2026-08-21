{
  pkgs,
  scufrisPackage,
  piperModel,
  piperConfig,
  terminalPackage,
  sessionDirectory,
  windowClass,
  windowInstance,
  initialTitle,
}: let
  inherit (pkgs) lib;
in
  pkgs.writeShellApplication {
    name = "scufris-popup";
    runtimeInputs = [pkgs.coreutils terminalPackage];
    text = ''
      install -d -m 0700 ${lib.escapeShellArg sessionDirectory}
      export SCUFRIS_SPEECH=1
      export SCUFRIS_CALM=1
      export SCUFRIS_PIPER_MODEL=${lib.escapeShellArg (toString piperModel)}
      export SCUFRIS_PIPER_CONFIG=${lib.escapeShellArg (toString piperConfig)}

      exec ${lib.getExe terminalPackage} \
        --class ${lib.escapeShellArg windowClass} \
        --name ${lib.escapeShellArg windowInstance} \
        --title ${lib.escapeShellArg initialTitle} \
        ${lib.getExe scufrisPackage} \
        --session-dir ${lib.escapeShellArg sessionDirectory} \
        --continue
    '';
    meta = {
      description = "Direct Kitty launcher for the resumable Scufris voice popup";
      mainProgram = "scufris-popup";
    };
  }
