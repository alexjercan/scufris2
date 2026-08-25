# Each group asserts one subject and hides its Linux-only members itself.
{
  inputs,
  self,
  pkgs,
  scufris,
}: let
  fixtures = import ./fixtures.nix {inherit pkgs;};
  args = {inherit inputs self pkgs scufris fixtures;};
in
  import ./launcher.nix args
  // import ./resources.nix args
  // import ./home.nix args
  // import ./voice.nix args
