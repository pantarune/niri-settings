#!/usr/bin/env bash
# Check the distro recipes. With --strict, also fail when a release-time
# placeholder is still present.
set -eu
cd "$(dirname "$0")/.."

strict=${1:-}
fail=0

note() {
  printf '%s\n' "$*"
}

bad() {
  printf 'FAIL: %s\n' "$*" >&2
  fail=1
}

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)
[ -n "$version" ] || bad "no workspace version in Cargo.toml"

grep -q "pkgver=$version" packaging/aur/PKGBUILD ||
  bad "AUR pkgver does not match $version"
grep -q "pkgver = $version" packaging/aur/.SRCINFO ||
  bad "AUR .SRCINFO pkgver does not match $version"
grep -q "Version:        $version" packaging/fedora/niri-settings-app.spec ||
  bad "Fedora Version does not match $version"
grep -q "version = \"$version\";" packaging/nixpkgs/niri-settings-app.nix ||
  bad "nixpkgs version does not match $version"

# Shell and spec syntax.
bash -n packaging/aur/PKGBUILD || bad "PKGBUILD syntax"
if command -v rpmspec >/dev/null 2>&1; then
  rpmspec --parse packaging/fedora/niri-settings-app.spec >/dev/null ||
    bad "Fedora spec does not parse"
else
  note "skip: rpmspec is not installed"
fi

if command -v desktop-file-validate >/dev/null 2>&1; then
  desktop-file-validate \
    data/io.github.niri_settings.NiriSettings.desktop ||
    bad "desktop file validation"
else
  note "skip: desktop-file-validate is not installed"
fi

# Build and test the nixpkgs recipe against the working tree.
if command -v nix >/dev/null 2>&1; then
  nix build --impure --no-link -f packaging/nixpkgs/local-build.nix --arg root "$PWD" ||
    bad "nixpkgs recipe build"
else
  bad "nix is not installed"
fi

if [ "$strict" = "--strict" ]; then
  grep -q 'Your Name <you@example.com>' packaging/aur/PKGBUILD &&
    bad "AUR Maintainer is still a placeholder"
  grep -q 'Your Name <you@example.com>' packaging/fedora/niri-settings-app.spec &&
    bad "Fedora changelog identity is still a placeholder"
  grep -q '0000000000000000000000000000000000000000000000000000000000000000' \
    packaging/aur/PKGBUILD &&
    bad "AUR sha256sums is still a placeholder"
  grep -q 'lib.fakeHash' packaging/nixpkgs/niri-settings-app.nix &&
    bad "nixpkgs src.hash is still lib.fakeHash"
  grep -q 'maintainers = \[ \];' packaging/nixpkgs/niri-settings-app.nix &&
    bad "nixpkgs maintainers is still empty"
  [ -f packaging/aur/.SRCINFO ] ||
    bad "AUR .SRCINFO is missing"
fi

if [ "$fail" -ne 0 ]; then
  exit 1
fi
note "packaging checks passed${strict:+ ($strict)}"
