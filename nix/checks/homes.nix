# One Home Manager evaluation of the Scufris module, so a check states only the
# options it is about. `settings` are `programs.scufris` definitions; `modules`
# read the resulting configuration back the way a consumer would.
{
  inputs,
  self,
  pkgs,
  fixtures,
}: {
  mkHome = {
    settings ? {},
    modules ? [],
  }:
    inputs.home-manager.lib.homeManagerConfiguration {
      inherit pkgs;
      modules =
        [
          self.homeModules.default
          {
            home = {
              username = "scufris-test";
              homeDirectory = "/home/scufris-test";
              stateVersion = "25.05";
            };
            programs.scufris = {
              enable = true;
              piPackage = fixtures.systemPi;
            };
          }
          {programs.scufris = settings;}
        ]
        ++ modules;
    };
}
