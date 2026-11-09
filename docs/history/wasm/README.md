> 历史 WASM 版本记录：已停止维护，当前开发目标为原生伴随程序。

# Natural Skin

[简体中文](README.zh-CN.md) · English

Natural skin retouching for [PhotoCraft](https://github.com/storytold/photocraft): reduce fine skin unevenness while retaining edges and texture. One small WebAssembly filter, with no model downloads, cloud services, or runtime dependencies.

- Adjustable smoothing strength, pixel radius, edge protection, and fine-texture retention.
- Feathered skin-colour masking, with a mask inspection mode.
- Independent skin brightening and rosiness controls, neutral by default.
- RGB 8-, 16-, and 32-bit documents; preserves alpha and skips out-of-range HDR pixels.
- Works with PhotoCraft selections and Undo.

Uses PhotoCraft **WebAssembly ABI v1**. Initial installation, execution, selection handling, Undo, RGB bit depths, and export were tested with **PhotoCraft 0.20.2 on macOS**. The current 1.1.1 build also passes CLI integration on **PhotoCraft 0.21.0**. Windows, Linux, web builds, and GUI live-preview performance have not been tested locally. A single Ubuntu CI job builds, tests, and packages the cross-platform WASM module; actual PhotoCraft compatibility on each platform requires separate host testing.

## Install

1. Download `NaturalSkin-<version>.wasm` from [Releases](https://github.com/wnanbei/photocraft-natural-skin/releases), or [build it from source](#build).
2. In PhotoCraft, choose **Filter → Plug-ins → Install Plug-in…** and select the `.wasm` file.
3. Run **Filter → Plug-ins → Natural Skin**. Version 1.0.1 uses English consistently for plugin-authored UI text.

To reload it after restarting PhotoCraft, enable **Edit → Preferences → Plug-ins → Use Additional Plug-ins Folder** and set **Additional Plug-ins Folder** to the directory containing the `.wasm`. Put the file directly in that directory, not in a nested folder. See the [PhotoCraft 0.20.2 plug-in contract](https://github.com/storytold/photocraft/blob/v0.20.2/docs/plugins.md).

## Use

Duplicate your photo layer and select its pixel thumbnail. Start with the defaults, optionally restrict processing to a feathered skin selection, and adjust the duplicated layer's opacity afterward.

PhotoCraft 0.20.2 generates this panel from ASCII parameter keys and translates recognized labels using its own UI language. For example, a Chinese host translates `Radius` and `Texture` but leaves unrecognized labels in English. ABI v1 has neither a separate label field nor a plugin-level language override. Parameter keys remain unchanged to preserve saved actions and smart-filter settings. An entirely English dialog requires PhotoCraft's UI language to be English; complete localization requires host support.

| Parameter | Default | Effect |
| --- | --- | --- |
| `Strength` | 45 | Overall smoothing amount, 0–100. |
| `Radius` | 6 px | Neighbourhood radius, 1–20 document pixels. |
| `Edge_Protection` | 70 | Higher values protect stronger edges and reduce smoothing. |
| `Texture` | 60 | Higher values retain more local fine detail. |
| `Skin_Only` | true | Restrict changes using a feathered skin-colour heuristic. |
| `Skin_Range` | 60 | Broaden the accepted skin-colour range, 0–100. |
| `skinBrightening` | 0 | Skin Brightening: lift midtones with gentle desaturation, 0–100. |
| `skinRosiness` | 0 | Skin Rosiness: positive adds redness, negative reduces it, -100–100. |
| `Show_Mask` | false | Inspect the affected area in grayscale. |

For tone adjustments without smoothing, set `Strength=0`. The two tone controls work independently of smoothing strength and share the original skin mask. `Skin_Only=false` applies both smoothing and tone to all valid pixels in the selection. Tone controls at 0 preserve the previous smoothing result exactly; `Strength=0` and both tone controls at 0 leave the input unchanged. To try the new controls, start around Skin Brightening 20–30 and Skin Rosiness 10–20, then adjust for the photo.

**Turn `Show_Mask` off before applying the filter.** If applied with it enabled, the mask visualization is written into the layer; Undo restores the photo. White means more effect, black means none.

The filter does not detect faces, reshape features, or automatically remove every blemish. Brightening and rosiness are colour adjustments, not semantic skin recognition or automatic skin-tone matching. Its colour mask can also match wood, sand, lips, and warm clothing. A skin selection provides better control. Grayscale, CMYK, Lab, alpha-channel targets, and layer-mask targets are rejected; use an RGB photo layer.

Pixels with non-finite RGB values or RGB outside 0–1 are left unchanged, including highlights in floating-point HDR documents. Version 1.0.1 removes filter scratch allocation from mask previews, fuses paired neighbourhood passes, and skips unaffected bands. Large images still take longer. See the [measured before/after results](performance-2026-11-07.md); CLI timings do not establish GUI response times.

## Faster two-step workflow

1. Smooth first, with both `skinBrightening=0` and `skinRosiness=0`, then confirm the filter.
2. Reopen the filter on the already-smoothed layer, explicitly set `Strength=0` and `Show_Mask=false`, then adjust brightening/rosiness and confirm.

The second step performs only pointwise colour adjustments and allocates no filter scratch. Setting `Strength=0` before confirming the first pass does not retain its uncommitted smoothing preview. Each confirmed pass is a separate Undo step; actions or smart filters should likewise use two sequential passes.

Version 1.1.1 trims empty borders and processes separated skin areas independently, retaining the full `2 × radius` context and original neighbour colours. Scratch is reused between regions and sized for the largest; empty skin bounds allocate none. Dense, fragmented or otherwise uneconomical regions automatically use the original path. `Texture=0` skips texture-neighbour calculations. Gains depend on the skin distribution inside each host band; see the [region performance and validation](skin-region-performance-1.1.1.zh-CN.md).

## Implementation languages

All pixel processing and mask generation are written in **Rust**, in `src/lib.rs` and `src/regions.rs`, and compiled to **WebAssembly**. PhotoCraft 0.20.2 executes that module locally using the Rust wasmi interpreter. The plugin has no JavaScript/HTML interface: PhotoCraft generates its parameter panel.

The repository's JavaScript files are developer tools for testing, benchmarking, packaging, and version management. They are not installed with or executed by the filter. GitHub's language percentages include those tools and do not measure the UI/algorithm split.

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

This additionally checks selections, Undo, 16/32-bit processing, and a 1536×1024 image against an unsplit WASM reference. Generated fixtures and reports stay in ignored `test-output/`. See [testing details](testing.md).

## Development and releases

The first release is **1.0.0**; see [CHANGELOG.md](../../../CHANGELOG.md) for release dates and changes. `Cargo.toml` is authoritative; use `node scripts/version.mjs bump patch|minor|major` (choose one increment) or an explicit prerelease version to synchronize the metadata. `node scripts/version.mjs prepare` dates the changelog without creating a commit or tag. See the [version policy](versioning.md) for rules and examples.

See [CONTRIBUTING.md](../../../CONTRIBUTING.md), [CHANGELOG.md](../../../CHANGELOG.md), and the [release procedure](releasing.md). CI runs on pull requests and pushes to `main`; a pushed `v<version>` tag builds, verifies, and publishes release assets. Prerelease tags are marked as GitHub prereleases and do not replace Latest.

This project was initially delivered as **SoftSkin**. The current menu name is **Natural Skin** and the plugin ID is `io.github.wnanbei.photocraft.natural-skin`, unchanged from Natural Skin 1.0.0. Remove an old SoftSkin `.wasm` from your extra plugin directory to avoid duplicate entries. Existing smart filters referencing the SoftSkin ID do not migrate automatically: retain the old plugin while migrating those documents.

## License

[MIT](../../../LICENSE). Copyright © 2026 NanBei.W. This is an independent community plugin, not an official PhotoCraft release.
