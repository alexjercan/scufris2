# The launcher renders an exact Pi command line and falls back to the pinned
# Pi only when no system Pi is on PATH.
#
# There is one launcher. The voice variant is gone with the speech module it
# existed to ship: the agent shapes every answer as prose whatever is
# listening, so there is nothing left for a second build to turn on.
{
  pkgs,
  scufris,
  fixtures,
  ...
}: let
  inherit (pkgs) lib;
  inherit (scufris) resources launcher piPackage terminal ctl den briefing;
  inherit (fixtures) systemPi;
  python = import ../python.nix {inherit pkgs;};
  agentRuntime = import ../agent-runtime.nix {inherit pkgs den briefing;};
in {
  launcher-normal =
    pkgs.runCommand "scufris-launcher-normal-check" {
      nativeBuildInputs = [launcher systemPi];
    } ''
      scufris user-argument > actual
      cat > expected <<'EOF'
      ["~/personal","~/work","~/third-party"]
      orchestrator


      system-pi
      --extension
      ${resources}/share/scufris/extensions/scufris/workflow/index.ts
      --skill
      ${resources}/share/scufris/skills/workflow
      --extension
      ${resources}/share/scufris/extensions/scufris/briefing/index.ts
      --extension
      ${resources}/share/scufris/extensions/scufris/response.ts
      --extension
      ${resources}/share/scufris/extensions/scufris/calm.ts
      --extension
      ${resources}/share/scufris/extensions/scufris/service/index.ts
      --skill
      ${resources}/share/scufris/skills/den
      user-argument
      EOF
      diff -u expected actual
      touch "$out"
    '';

  # The briefing extension runs `tools/briefing` by resource path, so the
  # `python3` the launcher puts on PATH is the one that has to import what
  # those scripts import. A plain `pkgs.python3` there is a launcher that
  # builds, installs, and then fails at the first briefing and nowhere earlier.
  launcher-briefing-python = pkgs.runCommand "scufris-launcher-briefing-python-check" {} ''
    grep -q ${python} ${launcher}/bin/scufris
    ${python}/bin/python3 ${resources}/share/scufris/tools/briefing/cli.py --help > /dev/null
    touch "$out"
  '';

  # The other launcher. It is normal interactive Pi plus the two programs it
  # has to find, and it composes nothing: the checkout it runs in owns the
  # extension, because two compositions would be two agents.
  terminal-launcher =
    pkgs.runCommand "scufris-terminal-launcher-check" {
      nativeBuildInputs = [pkgs.shellcheck];
    } ''
      wrapper=${terminal}/bin/scufris-terminal
      grep -q ${ctl}/bin "$wrapper"
      grep -q ${piPackage}/bin "$wrapper"
      # The text is one file in the store rather than a copy of itself here,
      # so the working tree and the deployment answer the same way.
      script="$(grep -o '/nix/store/[^ ]*/scripts/scufris-terminal' "$wrapper")"
      shellcheck "$script"
      grep -q 'SCUFRIS_TERMINAL=1' "$script"
      ! grep -q -- '--extension' "$script"
      touch "$out"
    '';

  # The two launchers start the same composition, so they put the same
  # programs on its PATH. The terminal used to carry none of them: the
  # briefing failed at its first import, in a terminal only, and the
  # interpreter it needed was on the other launcher all along.
  launcher-runtime = pkgs.runCommand "scufris-launcher-runtime-check" {} ''
    for program in ${lib.concatMapStringsSep " " toString agentRuntime}; do
      grep -q "$program" ${launcher}/bin/scufris
      grep -q "$program" ${terminal}/bin/scufris-terminal
    done
    touch "$out"
  '';

  launcher-fallback-pi = pkgs.runCommand "scufris-launcher-fallback-pi-check" {} ''
    export HOME="$TMPDIR/home"
    mkdir -p "$HOME"
    expected="$(${piPackage}/bin/pi --version)"
    actual="$(PATH=/nonexistent ${launcher}/bin/scufris --version)"
    test "$actual" = "$expected"
    touch "$out"
  '';
}
