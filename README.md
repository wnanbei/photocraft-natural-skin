# Natural Skin

[简体中文](README.zh-CN.md) · English

Natural skin retouching for [PhotoCraft](https://github.com/storytold/photocraft): reduce fine skin unevenness while retaining edges and texture. One small WebAssembly filter, with no model downloads, cloud services, or runtime dependencies.

- Adjustable smoothing strength, pixel radius, edge protection, and fine-texture retention.
- Feathered skin-colour masking, with a mask inspection mode.
- RGB 8-, 16-, and 32-bit documents; preserves alpha and skips out-of-range HDR pixels.
- Works with PhotoCraft selections and Undo.

Uses PhotoCraft **WebAssembly ABI v1**. Installation, execution, selection handling, Undo, RGB bit depths, and export were tested with **PhotoCraft 0.20.2 on macOS**. Windows, Linux, web builds, and GUI live-preview performance have not been tested locally. A single Ubuntu CI job builds, tests, and packages the cross-platform WASM module; actual PhotoCraft compatibility on each platform requires separate host testing.

## Install

1. Download `NaturalSkin-<version>.wasm` from [Releases](https://github.com/wnanbei/photocraft-natural-skin/releases), or [build it from source](#build).
2. In PhotoCraft, choose **Filter → Plug-ins → Install Plug-in…** and select the `.wasm` file.
3. Run **Filter → Plug-ins → Natural Skin 自然肌肤**.

To reload it after restarting PhotoCraft, enable **Edit → Preferences → Plug-ins → Use Additional Plug-ins Folder** and set **Additional Plug-ins Folder** to the directory containing the `.wasm`. Put the file directly in that directory, not in a nested folder. See the [PhotoCraft 0.20.2 plug-in contract](https://github.com/storytold/photocraft/blob/v0.20.2/docs/plugins.md).

## Use

Duplicate your photo layer and select its pixel thumbnail. Start with the defaults, optionally restrict processing to a feathered skin selection, and adjust the duplicated layer's opacity afterward.

| Parameter | Default | Effect |
| --- | --- | --- |
| `Strength` | 45 | Overall smoothing amount, 0–100. |
| `Radius` | 6 px | Neighbourhood radius, 1–20 document pixels. |
| `Edge_Protection` | 70 | Higher values protect stronger edges and reduce smoothing. |
| `Texture` | 60 | Higher values retain more local fine detail. |
| `Skin_Only` | true | Restrict changes using a feathered skin-colour heuristic. |
| `Skin_Range` | 60 | Broaden the accepted skin-colour range, 0–100. |
| `Show_Mask` | false | Inspect the affected area in grayscale. |

**Turn `Show_Mask` off before applying the filter.** If applied with it enabled, the mask visualization is written into the layer; Undo restores the photo. White means more effect, black means none.

The filter does not detect faces, reshape features, whiten skin, or automatically remove every blemish. Its colour mask can also match wood, sand, lips, and warm clothing. A skin selection provides better control. Grayscale, CMYK, Lab, alpha-channel targets, and layer-mask targets are rejected; use an RGB photo layer.

Pixels with non-finite RGB values or RGB outside 0–1 are left unchanged, including highlights in floating-point HDR documents. Large images take longer; the first local 1536×1024 / radius-20 CLI test took approximately 3.3 seconds including install and export. That is not a performance guarantee.

## Build

Requires Rust and its `wasm32-unknown-unknown` standard library. The checked-in `rust-toolchain.toml` selects stable Rust with the required target and development components. Rust 1.85 is the declared minimum, but the initial local validation used Rust 1.98.1 rather than an MSRV matrix. There are no third-party Cargo dependencies.

```sh
rustup target add wasm32-unknown-unknown
cargo build --release --locked --offline --target wasm32-unknown-unknown
```

The resulting plugin is `target/wasm32-unknown-unknown/release/photocraft_natural_skin.wasm`.

For a distributable folder, install Node.js 24+ and run:

```sh
sh build.sh
# Windows PowerShell:
./build.ps1
```

`dist/` contains the versioned `.wasm`, its manifest, documentation, license, release notes, and `SHA256SUMS.txt`. Packaging checks Cargo, lockfile, manifest, changelog, and embedded WASM versions. No npm install is needed.

## Verify

```sh
node scripts/version.mjs check
node --test tests/version.test.mjs
cargo fmt --all --check
cargo clippy --all-targets --locked --offline -- -D warnings
cargo clippy --target wasm32-unknown-unknown --locked --offline -- -D warnings
cargo test --locked --offline
cargo build --release --locked --offline --target wasm32-unknown-unknown
node tests/verify.mjs
```

The WASM checks cover smoothing, skin-mask exclusion, edge/texture retention, transparent/HDR/invalid pixels, error paths, and independent-band seams. For an actual PhotoCraft CLI integration test, pass the executable as the second argument:

```sh
node tests/verify.mjs \
  target/wasm32-unknown-unknown/release/photocraft_natural_skin.wasm \
  /path/to/photocraft-cli
```

This additionally checks selections, Undo, 16/32-bit processing, and a 1536×1024 image against an unsplit WASM reference. Generated fixtures and reports stay in ignored `test-output/`. See [testing details](docs/testing.md).

## Development and releases

The first release is **1.0.0**; see [CHANGELOG.md](CHANGELOG.md) for release dates and changes. `Cargo.toml` is authoritative; use `node scripts/version.mjs bump patch|minor|major` (choose one increment) or an explicit prerelease version to synchronize the metadata. `node scripts/version.mjs prepare` dates the changelog without creating a commit or tag. See the [version policy](docs/versioning.md) for rules and examples.

See [CONTRIBUTING.md](CONTRIBUTING.md), [CHANGELOG.md](CHANGELOG.md), and the [release procedure](docs/releasing.md). CI runs on pull requests and pushes to `main`; a pushed `v<version>` tag builds, verifies, and publishes release assets. Prerelease tags are marked as GitHub prereleases and do not replace Latest.

This project was initially delivered as **SoftSkin**. The new menu name is **Natural Skin 自然肌肤** and the new plugin ID is `io.github.wnanbei.photocraft.natural-skin`. Remove an old SoftSkin `.wasm` from your extra plugin directory to avoid duplicate entries. Existing smart filters referencing the old ID do not migrate automatically: retain the old plugin while migrating those documents.

## License

[MIT](LICENSE). Copyright © 2026 NanBei.W. This is an independent community plugin, not an official PhotoCraft release.
