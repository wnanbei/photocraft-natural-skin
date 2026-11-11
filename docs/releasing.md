# Native release packaging

The **Native Release** workflow in `.github/workflows/release.yml` publishes versioned software to GitHub Releases when an existing `native-v*` tag is pushed. It also has a manual `workflow_dispatch` input for tags created before this workflow existed. The native application/CLI is currently **0.3.0**.

## Published packages

Every release requires all three packages:

| Package suffix | Architecture | Contents |
| --- | --- | --- |
| `macOS-arm64.zip` | Apple Silicon | `Natural Skin.app` |
| `macOS-x86_64.zip` | Intel Mac | `Natural Skin.app` |
| `Windows.zip` | Windows x64 | GUI and CLI executables, docs, licenses and binary checksums |

Each filename starts with `Natural-Skin-<version>-`. The Release also includes **`SHA256SUMS.txt`**, covering the exact downloadable ZIP files. Release notes come from that version's tagged CHANGELOG section and record the source tag and commit.

Release assets are stored with the GitHub Release without the temporary Actions artifact expiry; they remain available unless the release/assets or repository are deleted. Platform jobs upload directly to the unpublished Release; release publication uses no Actions artifact storage. Regular main/PR development artifacts still expire after 14 days.

## Publishing a new version

Follow [native versioning](versioning.md), push the release commit and its annotated tag:

```sh
git push origin main
git tag -a native-v0.3.1 -m 'Natural Skin Native 0.3.1'
git push origin native-v0.3.1
```

The example assumes the committed native manifest, lockfile and CHANGELOG have already been updated to 0.3.1. Do not retag an existing version.

The workflow:

1. Resolves the existing native tag to an immutable source commit and validates its version/changelog. It rejects historical WASM tags.
2. Runs formatting, strict Clippy, all Rust tests and release builds on Windows x64, macOS Apple Silicon and macOS Intel.
3. Checks the executable version, produces a versioned package for each platform and uploads it directly to the unpublished draft. macOS bundles also undergo signature verification.
4. After every platform succeeds, downloads all three ZIPs from the draft, checks archive integrity, expected executables and macOS bundle versions, and generates ZIP checksums.
5. Confirms the remote tag still points to the validated commit, attaches the checked assets and checksum file, then publishes the complete Release.

A failed build/upload never publishes a partial new release. Failed uploads can leave a private draft, which the workflow can resume. Published releases are not overwritten on retry. Release jobs use `contents: write` for draft creation, package upload and final publication; the token is exposed only to the steps that access Releases. Checkout does not persist credentials, and actions are pinned. The workflow uses its short-lived `GITHUB_TOKEN`; no personal access token or new secret is needed.

## Backfilling an existing tag

For an already pushed tag such as **`native-v0.3.0`**, open **Actions → Native Release → Run workflow**, select the main branch, and enter the existing tag in the `tag` field. This uses the current publishing workflow but builds the exact existing tag commit. It does not move the tag or release the latest main code under an old version.

## Local builds and checks

After `cargo fetch --locked`, `sh build.sh` builds the native binaries and an ad-hoc-signed macOS application ZIP. The app includes its license notices and, for packages produced by the updated script, the CLI in `Contents/MacOS/natural-skin-companion`. `build.ps1` builds a Windows portable ZIP containing an explicit file list, excluding stale files from earlier local builds. Both scripts read their version from Cargo and do not publish, install or modify PhotoCraft.

Release helper tests need Python 3.11+ on the CI/tooling machine; ordinary Rust builds and the delivered software do not need Python:

```sh
python3 -m unittest discover -s scripts/tests -v
python3 scripts/native_release.py prepare --tag native-v0.3.0
```

macOS software currently uses ad-hoc development signing, without Apple notarization. Windows packages are portable executables without a signed installer. Hosted tests/builds do not establish native Windows interaction, real-portrait or calibrated-monitor acceptance. These boundaries remain documented in the software's release notes.
