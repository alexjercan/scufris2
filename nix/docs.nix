{
  inputs,
  self,
  pkgs,
}: let
  inherit (pkgs) lib;
  package = builtins.fromJSON (builtins.readFile ../package.json);
  docsHome = inputs.home-manager.lib.homeManagerConfiguration {
    inherit pkgs;
    modules = [
      self.homeModules.default
      {
        home = {
          username = "scufris-docs";
          homeDirectory = "/home/scufris-docs";
          stateVersion = "25.05";
        };
      }
    ];
  };
  optionDocs = pkgs.nixosOptionsDoc {
    options = {programs.scufris = docsHome.options.programs.scufris;};
  };
  optionNames = builtins.attrNames optionDocs.optionsNix;
  optionScopeIsExact =
    optionNames
    != []
    && lib.all (lib.hasPrefix "programs.scufris") optionNames
    && builtins.hasAttr "programs.scufris.enable" optionDocs.optionsNix
    && builtins.hasAttr "programs.scufris.service.enable" optionDocs.optionsNix
    && builtins.hasAttr "programs.scufris.desktop.enable" optionDocs.optionsNix;
in
  assert optionScopeIsExact;
    pkgs.stdenvNoCC.mkDerivation {
      pname = "scufris-docs";
      inherit (package) version;
      src = ../docs;
      nativeBuildInputs = [pkgs.mdbook pkgs.mdbook-mermaid];

      buildPhase = ''
        runHook preBuild
        cp -R "$src" source
        chmod -R u+w source
        mkdir -p source/src/reference
        cat > source/src/reference/options.md <<'EOF'
        # Home Manager options

        [Previous: Environment variables](environment.md)

        This page is generated from the evaluated `programs.scufris` module.
        Use [Configure it](../guide/configuration.md) for the guided option map.

        ```text
        option -> evaluated type + default + description -> generated systemd/package configuration
        ```
        EOF
        cat ${optionDocs.optionsCommonMark} >> source/src/reference/options.md
        mdbook build source
        runHook postBuild
      '';

      installPhase = ''
        runHook preInstall
        mkdir -p "$out"
        cp -R source/book/. "$out/"
        runHook postInstall
      '';
    }
