# Releasing

The package name is `niri-settings-app`. The existing `niri-settings` packages
in Fedora and openSUSE, and the AUR `niri-settings-git`, are a different Qt
application.

## Before the tag

1. Update the workspace version in `Cargo.toml`.
2. Update the release entry in
   `data/io.github.niri_settings.NiriSettings.metainfo.xml`.
3. If the niri version changed, regenerate the schema and review the diff:
   `scripts/check-schema.sh --write`.
4. Run the full check inside the dev shell:

   ```sh
   cargo fmt --all -- --check
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
   scripts/check-schema.sh
   scripts/a11y-check.sh
   ```
5. Commit the changes.

## The tag

```sh
git tag v0.1.0
git push origin v0.1.0
```

The release workflow tests the workspace, builds `.#niri-settings-app`, and
creates a GitHub release with:

- `niri-settings-app-<version>.tar.gz`, the source archive
- `SHA256SUMS`

The archive excludes `packaging/` through `.gitattributes`. Otherwise the AUR
recipe would contain the hash of an archive that contains the recipe. The
archive uses fixed ownership and timestamps, so the same tag always produces
the same `SHA256SUMS` entry.

It does not attach a binary tarball. Nix users install the flake; Arch, Fedora
and Debian users get the app from their package manager.

## After the release

- Fill the release hashes and maintainer identities in `packaging/` and run
  `scripts/check-packaging.sh --strict`.
- Follow `packaging/README.md`: nixpkgs pull request first, then the AUR
  `niri-settings-app` package, then the Fedora COPR.
- Only then consider a Debian/Ubuntu package. niri itself is not in Debian.
