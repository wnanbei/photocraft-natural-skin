# Releases

The release workflow is triggered by a pushed tag matching `v*`. It builds and validates the plugin before publishing assets to a GitHub release. Only the release job has `contents: write`; normal CI uses read-only repository permissions.

## Prepare

1. For the initial release, retain the existing 1.0.0 draft. For a subsequent version, run `node scripts/version.mjs bump patch`, `minor`, `major`, or an explicit higher version such as `1.1.0-rc.1`. This synchronizes Cargo, the root lockfile entry, manifest, and changelog draft. See [the version policy](versioning.md).
2. Describe the changes in the current changelog section. Update both READMEs and compatibility evidence when relevant. Run `node scripts/version.mjs prepare` to stamp the release date, or pass `--date YYYY-MM-DD` explicitly.
3. Run the complete local checks, including `node scripts/version.mjs check` and `node --test tests/version.test.mjs`, and the actual PhotoCraft CLI test when available. Review the generated report and run the plugin in PhotoCraft's GUI before claiming GUI compatibility.
4. Run `sh build.sh` or `./build.ps1` to create `dist/`. Then run `node scripts/package.mjs v<version>`: a release package requires matching versions, a dated changelog, real release notes, and a fresh compiled manifest.
5. Commit and push the reviewed source to `main`, then ensure CI passes. The version script does not perform Git operations.

## Publish

After review, create an annotated tag and push it:

```sh
git tag -a v1.0.0 -m "Natural Skin 1.0.0"
git push origin v1.0.0
```

Creating/pushing a tag is the publishing action; the commands above are documentation, not executed by repository initialization. The workflow attaches the `.wasm`, manifest, license, documentation, changelog, release notes, and SHA-256 checksum list. GitHub also supplies source archives. It never pushes changes or creates a tag on your behalf.

Release notes come from the current version's changelog section. A stable tag creates a regular release marked Latest; a prerelease tag such as `v1.1.0-rc.1` creates a GitHub prerelease without replacing Latest. A mismatch, undated draft, or stale embedded manifest stops the workflow before publication. Never replace a published tag to repair a release; bump the version instead.

Verify the published `.wasm` with `SHA256SUMS.txt` and test the downloaded artifact in PhotoCraft. Windows PowerShell can inspect its hash with `Get-FileHash .\NaturalSkin-1.0.0.wasm -Algorithm SHA256`; macOS can use `shasum -a 256 NaturalSkin-1.0.0.wasm`.

## GitHub settings

The suggested About description is: **Natural skin retouching plugin for PhotoCraft with texture preservation and edge-aware smoothing.** Suggested topics: `photocraft`, `photocraft-plugin`, `skin-retouching`, `image-processing`, `webassembly`, `rust`.

After the first CI run, a maintainer may protect `main` by requiring the single `Verify` check and pull-request review. Repository initialization does not change branch protection, visibility, collaborator access, or account settings.
