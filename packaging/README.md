# Packaging

The package name is `niri-settings-app`. The existing `niri-settings` packages
in Fedora and openSUSE, and the AUR `niri-settings-git`, are a different Qt
application; do not upload these recipes under those names.

The order follows how niri users install software: nixpkgs first, then the AUR,
then Fedora COPR. Debian and Ubuntu come later, if at all: niri is not in
Debian.

## Before any submission

1. Tag the release and let `.github/workflows/release.yml` publish
   `niri-settings-app-<version>.tar.gz` and `SHA256SUMS`.
2. Fill the maintainer identity in `aur/PKGBUILD` and
   `fedora/niri-settings-app.spec`.
3. Fill the source hashes described below.
4. Run `scripts/check-packaging.sh` inside `nix develop`.

`niri` is a hard runtime dependency. `Session::open` fails closed when the
`niri` command is not on `PATH`, because every write is checked with
`niri validate`.

`.gitattributes` keeps `packaging/` out of the release tarball. The AUR recipe
contains the tarball hash, so including the recipe would make the hash
self-referential. Keep that export-ignore rule when you add packaging files.

## nixpkgs

Copy `nixpkgs/niri-settings-app.nix` to
`pkgs/by-name/ni/niri-settings-app/package.nix` in a nixpkgs checkout.

The `cargoHash` in this repository is already the hash of the workspace
dependencies. The `src.hash` is `lib.fakeHash` because it can only be computed
after the `v<version>` tag exists:

```sh
nix build .#niri-settings-app 2>&1 | grep 'got:'
```

Put that hash in `src.hash`, then build the real derivation:

```sh
nix build .#niri-settings-app
nixpkgs-review pr <number>
```

Add your nixpkgs maintainer handle to `meta.maintainers` before opening the
pull request.

## AUR

The `source` line uses the GitHub release asset, the same file the release
workflow builds. After the tag:

```sh
cd packaging/aur
updpkgsums
makepkg --printsrcinfo > .SRCINFO
makepkg -si
namcap PKGBUILD
```

Replace the `Maintainer` line with your name and email. Push `PKGBUILD` and
`.SRCINFO` to the AUR package `niri-settings-app`.

## Fedora COPR

Create a COPR project and add `fedora/niri-settings-app.spec`. COPR downloads
`Source0` from the GitHub release; the source tarball must exist before the
build starts.

```sh
rpmlint fedora/niri-settings-app.spec
fedpkg scratch-build --srpm
```

Replace the `%changelog` name and email, and keep `Release: 1%{?dist}` for the
first COPR build. The spec uses the `rust-packaging` cargo macros.

Fedora's `niri` package provides the `niri` command used by the tests and at
runtime.
