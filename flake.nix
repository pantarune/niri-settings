{
  description = "niri-settings-app: a safe graphical settings app for niri";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forAll = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in {
      packages = forAll (pkgs: {
        niri-settings-app = pkgs.callPackage ./nix/package.nix { };
        default = self.packages.${pkgs.system}.niri-settings-app;
      });

      devShells = forAll (pkgs: {
        default = pkgs.mkShell {
          nativeBuildInputs = with pkgs; [
            pkg-config
            rustc
            cargo
            rustfmt
            clippy
            rust-analyzer
            rpm
            desktop-file-utils
            appstream
          ];
          buildInputs = with pkgs; [
            gtk4
            libadwaita
            glib
            cairo
            pango
            gdk-pixbuf
            graphene
            harfbuzz
            libepoxy
            fontconfig
            freetype
            libpng
            expat
            zlib
            libffi
            pcre2
            util-linux
            libselinux
            libsepol
            adwaita-icon-theme
            hicolor-icon-theme
            gsettings-desktop-schemas
            dbus
            niri
          ];
          shellHook = ''
            export RUST_SRC_PATH="${pkgs.rustPlatform.rustLibSrc}"
            export NIRI_SRC="${pkgs.niri.src}"
            export NIRI_VERSION="${pkgs.niri.version}"
            export GSETTINGS_SCHEMA_DIR="${pkgs.gsettings-desktop-schemas}/share/gsettings-schemas/${pkgs.gsettings-desktop-schemas.name}/glib-2.0/schemas"
            export XDG_DATA_DIRS="${pkgs.adwaita-icon-theme}/share:${pkgs.hicolor-icon-theme}/share:$XDG_DATA_DIRS"
            export PKG_CONFIG_PATH="${pkgs.lib.makeSearchPathOutput "dev" "lib/pkgconfig" (with pkgs; [ gtk4 libadwaita glib cairo pango gdk-pixbuf graphene harfbuzz libepoxy fontconfig freetype libpng expat zlib libffi pcre2 util-linux libselinux libsepol ])}:$PKG_CONFIG_PATH"
            echo "niri-settings-app dev shell: cargo test --workspace"
          '';
        };
      });
    };
}
