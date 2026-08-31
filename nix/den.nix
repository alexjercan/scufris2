{pkgs}:
# The-den on the command line. The desktop panels compile the same library into
# their backend and never run this, so what it is for is the agent: one program
# that reads and writes the journal by the same rules a panel does.
pkgs.writeShellApplication {
  name = "scufris-den";
  runtimeInputs = [pkgs.python3];
  text = ''
    exec python3 ${../tools/den}/cli.py "$@"
  '';
  meta = {
    description = "Read and write the-den journal";
    mainProgram = "scufris-den";
  };
}
