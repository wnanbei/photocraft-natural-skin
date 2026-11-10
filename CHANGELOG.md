# Changelog

## Native 0.3.0 — 2026-11-11

- Align the workbench with PhotoCraft Pro: Dark/Gray/Light palettes, system appearance, Inter typography, flat section strips and compact adjustment rows.
- Add independent, instant and persistent Simplified Chinese/English selection, including existing task messages, setup, known errors and application-owned macOS menus. Preserve legacy connection/recovery settings and user filenames.
- Add physical-pixel 1:1 crop inspection with full filter context, mouse/keyboard panning and source/result/mask comparison.
- Convert bounded SDR RGB matrix/TRC ICC input through sRGB working space and back to source encoding; preserve unchanged and protected samples; reject unsupported profiles. Add explicit canvas display-ICC output selection.
- Share immutable-source 256-pixel tile processing between GUI and CLI exports, add filter/cache/tone cancellation checkpoints and throttle background progress.
- Keep status geometry stable so numeric progress cannot repeatedly invalidate detail previews.
- Add synthetic skin-shade quality fixtures, ICC reference tests and reproducible 24 MP timing/RSS/cancellation measurements. Portrait, calibrated-monitor and Windows runtime acceptance remain separate.

## Native 0.2.0 — 2026-11-09

- Add persistent PhotoCraft connection setup and generated one-time Send action installer.
- Restore the receiver on normal launch; activate the existing instance on repeated launches without opening a control port.
- Add opt-in per-user login startup, explicit Quit, minimize-on-close while connected, last-result recovery and folder access.
- Handle macOS Finder document opening and wait for file commits before quitting.
- Show per-stage numeric progress when supplied; bound pending capture notifications and atomically publish TIFF exports.
- Keep Windows/macOS shared UI and platform CI; Windows runtime validation and distribution signing remain outstanding.


## Native companion 0.1.0 - Unreleased desktop development build

- Switch development to a native Rust companion; retire the WASM exports, manifest, build/release tools and dual-version plan.
- Reuse the guided retouching core with bounded native execution and immutable-source preview caching.
- Add stage/elapsed events, cancellation checkpoints, high-precision TIFF exchange, authenticated loopback control and source revision guards.
- Add native pcraft read/write with raw sample, off-canvas tile and metadata preservation; validate six multi-tile desktop round trips with zero sample differences.
- Add the shared Windows/macOS egui frontend, background worker, cached previews, source/result comparison, mask inspection, task status/cancellation and full-resolution export.
- Check the Windows GUI target from macOS; real Windows runtime acceptance and automated host handoff remain unfinished.

- Add experimental resident file handoff, a native JSON action installer, and Apply to PhotoCraft; verify macOS action-button capture, hidden-window reopening and new-document return with Agent Control disabled.
- Move background events into eframe App::logic so hidden windows keep receiving requests; add regression tests.

## Historical WASM work

## [1.1.1] - Archived WASM draft

- Split separated skin areas inside each input band into disjoint regions with the original two-radius context, reuse scratch sized for the largest region, and retain the original path for dense or fragmented plans.
- Add shipping-WASM region, memory, alpha/HDR, oracle and seam regressions, plus alternating interpreter benchmarks for separated, ring-shaped and fragmented masks.
- Trim unaffected band borders before filtering, retaining the complete two-radius neighbourhood and allocating scratch only for the resulting region.
- Avoid texture-neighbour calculations and channel-source reconstruction when Texture is zero; skip unused weight-mean initialization and checks for opaque regions.
- Document and verify a two-pass workflow: confirm smoothing first, then reopen with Strength zero for scratch-free brightening and rosiness.
- Add sparse-region/corner/alpha oracle checks, ROI memory-allocation assertions, and dense/sparse interpreter benchmarks.
- Add independent Skin Brightening (0–100) and Skin Rosiness (-100–100) controls, both neutral by default.
- Brighten skin midtones with gentle desaturation; adjust redness while preserving luminance, with uniform chroma compression at gamut boundaries.
- Reuse the original skin gate after smoothing, preserve mask inspection mode, and support scratch-free tone-only processing with Strength set to 0.
- Add native/WASM regressions and PhotoCraft CLI integration for skin tone, selections, Undo, RGB bit depths, and combined-filter band seams.
- Generate mask previews directly without allocating seven full-size scratch planes.
- Fuse paired guided-filter moment passes and improve locality of vertical averaging.
- Shortcut the constant parts of skin-colour feather ramps without changing transition weights.
- Use an equivalent fast path for opaque images and skip filtering bands with no affected pixels.
- Use English consistently for plugin-authored UI text, including the name and description. Preserve all ABI parameter keys and saved settings; PhotoCraft 0.20.2 still translates some generated labels using its own UI language and provides no plugin-level override.
- Add mask memory-growth checks, paired/weighted-kernel regressions, an optional previous-release pixel oracle, and reproducible benchmarks using PhotoCraft's actual interpreter.

## [1.0.0] - 2026-11-06

- Introduce Natural Skin, formerly the locally delivered SoftSkin plugin.
- Provide guided skin smoothing with adjustable radius, edge protection, texture retention, and a feathered skin-colour mask.
- Support PhotoCraft ABI v1, RGB 8/16/32-bit processing, alpha preservation, selections, and Undo.
- Fix weaker texture protection on semi-transparent pixels by consistently weighting the centre and neighbouring samples by alpha.
- Add import-free WebAssembly builds, native/WASM regression tests, and optional PhotoCraft CLI integration tests.
- Establish CI, version-checked packaging, and tag-triggered GitHub releases.
- Add synchronized semantic version bumps, changelog preparation, release-note extraction, and prerelease handling.

The plugin ID is now `io.github.wnanbei.photocraft.natural-skin`. This is a renamed initial release, not an automatic migration of existing SoftSkin smart-filter references.
