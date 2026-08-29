{
  pkgs,
  source,
  lockFile,
  version,
}: let
  inherit (pkgs) lib;
  # WebKitGTK and the tray indicator are dlopened, so they must stay on the
  # runtime library path even though pkg-config satisfies the build.
  tauriLibraries = [
    pkgs.webkitgtk_4_1
    pkgs.gtk3
    pkgs.libayatana-appindicator
    pkgs.librsvg
  ];
  unwrapped = pkgs.rustPlatform.buildRustPackage {
    pname = "scufris-desktop-unwrapped";
    inherit version;
    src = source;
    cargoLock = {inherit lockFile;};
    # Only the companion. The workspace also holds the headless service, and
    # that one must never need GTK to build.
    cargoBuildFlags = ["-p" "scufris-desktop"];
    cargoTestFlags = ["-p" "scufris-desktop"];
    # typescript compiles the pill frontend from build.rs.
    nativeBuildInputs = [pkgs.pkg-config pkgs.typescript pkgs.wrapGAppsHook3];
    buildInputs =
      tauriLibraries
      ++ [
        pkgs.alsa-lib
        pkgs.glib
        pkgs.openssl
      ];
    env.OPENSSL_NO_VENDOR = "1";
    preFixup = ''
      gappsWrapperArgs+=(--prefix LD_LIBRARY_PATH : ${lib.makeLibraryPath tauriLibraries})
      # Widget backends are Python 3 programs the companion spawns. The
      # interpreter belongs to the package: a widget whose numbers depend on
      # what the person's PATH happens to carry is a widget that works on the
      # machine it was written on.
      gappsWrapperArgs+=(--prefix PATH : ${lib.makeBinPath [pkgs.python3]})
    '';
    meta = {
      description = "Scufris voice pill and tray companion";
      mainProgram = "scufris-desktop";
      platforms = lib.platforms.linux;
    };
  };
in
  pkgs.runCommand "scufris-desktop-${version}" {
    inherit (unwrapped) meta;
    passthru = {inherit unwrapped;};
  } ''
    mkdir -p "$out/bin" "$out/share/applications" \
      "$out/share/icons/hicolor/scalable/apps"
    ln -s ${unwrapped}/bin/scufris-desktop "$out/bin/scufris-desktop"
    cp ${source}/surfaces/desktop/icons/scufris.svg \
      "$out/share/icons/hicolor/scalable/apps/scufris.svg"
    cat > "$out/share/applications/scufris-desktop.desktop" <<EOF
    [Desktop Entry]
    Type=Application
    Name=Scufris
    Comment=Scufris voice pill and tray companion
    Exec=$out/bin/scufris-desktop
    Icon=scufris
    Terminal=false
    Categories=Utility;
    EOF
  ''
