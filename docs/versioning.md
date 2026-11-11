# Native versions

The desktop application and CLI share the version in `native/Cargo.toml` (currently **0.3.0**). `natural-skin-core` retains the inherited algorithm version **1.1.1**; it is not a second product or WASM release. Both workspace crates have `publish = false`.

Native release tags use **`native-vMAJOR.MINOR.PATCH`**, optionally with a SemVer prerelease suffix such as `native-v0.4.0-rc.1`. Historical WASM tags such as `v1.0.0` stay unchanged and do not trigger native publication.

For a new version:

1. Update the application version in `native/Cargo.toml`.
2. Run Cargo to refresh the matching `Cargo.lock` entry and commit both files.
3. Add a nonempty `## Native <version> — YYYY-MM-DD` section to `CHANGELOG.md` and update current-version README text.
4. Test and commit, then create an annotated `native-v<version>` tag on that commit and push it.

`build.sh` and `build.ps1` read the native version using `cargo pkgid`. Archive names follow that version automatically. macOS `CFBundleShortVersionString` and `CFBundleVersion` use its numeric major/minor/patch component (prerelease/build suffixes are excluded from the plist); the full version is retained in the archive name and CLI version.

The Native Release workflow validates the existing tag, tagged application manifest, lockfile and changelog before starting platform builds. All packages are built from the resolved tag commit, even when a manual backfill runs from a newer workflow. An inconsistent version, missing package, mismatched bundle or moved tag blocks publication.

Stable native releases are marked Latest unless a newer stable native release already exists. Prereleases and older backfills do not displace a newer stable native release. Retrying an already published tag leaves its release assets unchanged; historical tags are never moved.

See [release packaging and downloads](releasing.md).
