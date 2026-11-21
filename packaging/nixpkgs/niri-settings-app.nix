{
  lib,
  rustPlatform,
  fetchFromGitHub,
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

let
  version = "0.1.0";
in
rustPlatform.buildRustPackage {
  pname = "niri-settings-app";
  inherit version;

  src = fetchFromGitHub {
    owner = "pantarune";
    repo = "niri-settings";
    tag = "v${version}";
    hash = "sha256-Nw9NikE4RLdOb5EfHxiAahL1HH2MOdwMdG+o/BRtpHU=";
  };

  cargoHash = "sha256-xRjUk63RGPjqyDRs07LxnfUpOiIdsIf2YYE94NfWYlg=";

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
    install -Dm644 data/io.github.niri_settings.NiriSettings.desktop \
      $out/share/applications/io.github.niri_settings.NiriSettings.desktop
    install -Dm644 data/io.github.niri_settings.NiriSettings.metainfo.xml \
      $out/share/metainfo/io.github.niri_settings.NiriSettings.metainfo.xml
    install -Dm644 data/io.github.niri_settings.NiriSettings.svg \
      $out/share/icons/hicolor/scalable/apps/io.github.niri_settings.NiriSettings.svg
    install -Dm644 LICENSE $out/share/licenses/niri-settings-app/LICENSE
  '';

  meta = {
    description = "Safe graphical settings app for the niri compositor";
    homepage = "https://github.com/pantarune/niri-settings";
    license = lib.licenses.gpl3Plus;
    mainProgram = "niri-settings-app";
    platforms = lib.platforms.linux;
    # Add your nixpkgs maintainer handle before opening the pull request.
    maintainers = [ ];
  };
}
