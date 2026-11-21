{
  lib,
  rustPlatform,
  pkg-config,
  wrapGAppsHook4,
  gtk4,
  libadwaita,
  glib,
  adwaita-icon-theme,
  hicolor-icon-theme,
  gsettings-desktop-schemas,
  niri,
}:

rustPlatform.buildRustPackage {
  pname = "niri-settings-app";
  version = (lib.importTOML ../Cargo.toml).workspace.package.version;

  src = lib.cleanSource ../.;
  cargoLock.lockFile = ../Cargo.lock;

  nativeBuildInputs = [
    pkg-config
    wrapGAppsHook4
  ];

  # The integration tests open a session and run `niri validate`.
  nativeCheckInputs = [ niri ];

  buildInputs = [
    gtk4
    libadwaita
    glib
    adwaita-icon-theme
    hicolor-icon-theme
    gsettings-desktop-schemas
  ];

  preFixup = ''
    gappsWrapperArgs+=(--prefix XDG_DATA_DIRS : "${adwaita-icon-theme}/share")
    gappsWrapperArgs+=(--prefix XDG_DATA_DIRS : "${hicolor-icon-theme}/share")
    # Session::open fails closed when niri is not on PATH.
    gappsWrapperArgs+=(--prefix PATH : "${niri}/bin")
  '';

  postInstall = ''
    # Build tool, not part of the application.
    rm -f $out/bin/niri-settings-schema-gen
    install -Dm644 ${../data/io.github.niri_settings.NiriSettings.desktop} \
      $out/share/applications/io.github.niri_settings.NiriSettings.desktop
    install -Dm644 ${../data/io.github.niri_settings.NiriSettings.metainfo.xml} \
      $out/share/metainfo/io.github.niri_settings.NiriSettings.metainfo.xml
    install -Dm644 ${../data/io.github.niri_settings.NiriSettings.svg} \
      $out/share/icons/hicolor/scalable/apps/io.github.niri_settings.NiriSettings.svg
    install -Dm644 ${../LICENSE} $out/share/licenses/niri-settings-app/LICENSE
  '';

  meta = {
    description = "Safe graphical settings app for the niri compositor";
    homepage = "https://github.com/pantarune/niri-settings";
    license = lib.licenses.gpl3Plus;
    mainProgram = "niri-settings-app";
    platforms = lib.platforms.linux;
  };
}
