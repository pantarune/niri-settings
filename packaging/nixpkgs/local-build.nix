# Build the nixpkgs recipe against this working tree, without waiting for a
# release tag. The real recipe fetches the tagged source from GitHub.
{
  root ? ../..,
}:

let
  rootPath = if builtins.isPath root then root else /. + root;
  flake = builtins.getFlake (toString rootPath);
  pkgs = flake.inputs.nixpkgs.legacyPackages.${builtins.currentSystem};
  localSrc = pkgs.lib.fileset.toSource {
    root = rootPath;
    fileset = pkgs.lib.fileset.unions [
      (rootPath + "/Cargo.toml")
      (rootPath + "/Cargo.lock")
      (rootPath + "/LICENSE")
      (rootPath + "/crates")
      (rootPath + "/data")
      (rootPath + "/tests")
    ];
  };
  package = pkgs.callPackage ./niri-settings-app.nix { };
in
package.overrideAttrs (old: {
  src = localSrc;
})
