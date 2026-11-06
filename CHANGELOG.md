# Changelog

## [1.0.0] - 2026-11-06

- Introduce Natural Skin, formerly the locally delivered SoftSkin plugin.
- Provide guided skin smoothing with adjustable radius, edge protection, texture retention, and a feathered skin-colour mask.
- Support PhotoCraft ABI v1, RGB 8/16/32-bit processing, alpha preservation, selections, and Undo.
- Fix weaker texture protection on semi-transparent pixels by consistently weighting the centre and neighbouring samples by alpha.
- Add import-free WebAssembly builds, native/WASM regression tests, and optional PhotoCraft CLI integration tests.
- Establish CI, version-checked packaging, and tag-triggered GitHub releases.
- Add synchronized semantic version bumps, changelog preparation, release-note extraction, and prerelease handling.

The plugin ID is now `io.github.wnanbei.photocraft.natural-skin`. This is a renamed initial release, not an automatic migration of existing SoftSkin smart-filter references.
