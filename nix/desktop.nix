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
    cp ${source}/scufris-desktop/icons/scufris.svg \
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
