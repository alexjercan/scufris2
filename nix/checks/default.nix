# Each group asserts one subject and hides its Linux-only members itself.
{
  inputs,
  self,
  pkgs,
  scufris,
}: let
  fixtures = import ./fixtures.nix {inherit pkgs;};
  homes = import ./homes.nix {inherit inputs self pkgs fixtures;};
  args = {inherit inputs self pkgs scufris fixtures homes;};
in
  import ./launcher.nix args
  // import ./resources.nix args
  // import ./home.nix args
  // import ./voice.nix args
  // import ./desktop.nix args
  // import ./service.nix args
