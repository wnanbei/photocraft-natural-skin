# Version policy

Natural Skin uses [Semantic Versioning 2.0.0](https://semver.org/): `MAJOR.MINOR.PATCH`, optionally followed by a prerelease identifier such as `-alpha.1`, `-beta.1`, or `-rc.1`.

The initial release version is **1.0.0**. Release dates and changes are recorded in [CHANGELOG.md](../CHANGELOG.md); future drafts use `Unreleased` until prepared. PhotoCraft's version (such as 0.20.2) and its plugin ABI version (currently 1) are separate compatibility identifiers.

## Compatibility contract

The public contract includes the plugin ID, parameter names/types/ranges, saved preset and smart-filter interpretation, and documented supported colour/host modes.

| Change | Increment | Example |
| --- | --- | --- |
| Backward-compatible bug fixes | PATCH | 1.0.0 → 1.0.1 |
| Compatible new controls or features; intentional visible algorithm improvements | MINOR | 1.0.1 → 1.1.0 |
| Incompatible plugin identity, removed/renamed controls, or incompatible saved-setting interpretation | MAJOR | 1.1.0 → 2.0.0 |
| Testing a forthcoming release | Explicit prerelease | 1.1.0-alpha.1 → 1.1.0-beta.1 → 1.1.0-rc.1 → 1.1.0 |

A documentation or CI-only update does not need an immediate new plugin release. Include it in the next planned release. Never move or overwrite an already published version tag; fix a published artifact with a new version.

## Authoritative version and synchronized files

`Cargo.toml`'s `[package].version` is authoritative. `manifest.json` and the root package entry in `Cargo.lock` mirror it. The top `CHANGELOG.md` section must use the same version. The embedded WASM manifest, distribution filename, checksum list, release notes, and pushed tag must agree as well.

Use the script to synchronize changes, rather than editing those copies independently. It uses only Node.js core modules, validates every input before writing, and preserves dependency lock entries and existing manifest controls.

```sh
node scripts/version.mjs                       # print current version
node scripts/version.mjs check                 # check all source metadata
node scripts/version.mjs bump patch            # 1.0.0 → 1.0.1
node scripts/version.mjs bump minor            # next minor
node scripts/version.mjs bump major            # next major
node scripts/version.mjs bump 1.1.0-beta.1      # explicit higher prerelease
```

The examples are alternatives; do not run all the bump commands in sequence unless that is intended. With a prerelease as the current version, `bump patch` promotes its core version to stable (for example 1.1.0-rc.1 → 1.1.0). Explicit versions must have greater SemVer precedence; changing only build metadata is not an upgrade. Versions are limited to 32 characters by the PhotoCraft manifest contract.

When the current version is still `Unreleased`, bumping renames that draft and preserves its notes. When it is already dated, bumping creates a new empty draft at the top and preserves previous releases below it. Fill in the new draft before preparing it for release. The script rejects duplicate, unordered, malformed, and mismatched version records.

## Prepare and publish

After editing the current version's changelog:

```sh
node scripts/version.mjs prepare                # use this machine's local calendar date
# Or explicitly choose the release date:
node scripts/version.mjs prepare --date 2026-11-06
node scripts/version.mjs check --tag v1.0.0 --release
node scripts/version.mjs notes                  # print this version's release notes
```

`prepare` replaces `Unreleased` with a validated date and rejects empty or TODO/TBD placeholder notes. It never commits, creates tags, pushes, or publishes anything. Historical dated release notes are preserved when preparing the next release.

Run the full verification and packaging, then commit the reviewed changes. Only after CI passes, create and push the matching annotated `v<version>` tag. See [the release procedure](releasing.md).

The release job rejects mismatched Cargo/manifest/lock/changelog/tag versions, an undated changelog, and a stale compiled manifest. Stable versions are published as regular releases. Versions with prerelease identifiers are marked GitHub prereleases and are not marked Latest. Release descriptions come from the corresponding changelog section.
