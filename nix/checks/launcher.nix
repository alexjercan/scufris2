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
  inherit (scufris) resources launcher piPackage;
  inherit (fixtures) systemPi;
in {
  launcher-normal =
    pkgs.runCommand "scufris-launcher-normal-check" {
      nativeBuildInputs = [launcher systemPi];
    } ''
      scufris user-argument > actual
      cat > expected <<'EOF'
      ["~/personal","~/work","~/third-party"]
      orchestrator
      08:00


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

  launcher-fallback-pi = pkgs.runCommand "scufris-launcher-fallback-pi-check" {} ''
    export HOME="$TMPDIR/home"
    mkdir -p "$HOME"
    expected="$(${piPackage}/bin/pi --version)"
    actual="$(PATH=/nonexistent ${launcher}/bin/scufris --version)"
    test "$actual" = "$expected"
    touch "$out"
  '';
}
