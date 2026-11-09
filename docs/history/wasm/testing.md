> 历史 WASM 版本记录：已停止维护，当前开发目标为原生伴随程序。

# Testing and compatibility

## What is verified

`node --test tests/version.test.mjs` checks semantic version precedence, synchronized metadata, draft/history handling, invalid-date and placeholder rejection, and no mutation on rejected operations. Fixtures live in temporary directories; they do not bump or date the working repository.

The native Rust tests compare sliding-window statistics to a naive reference and cover tiny images, transparent samples, colour gating, parameter decoding, and zero-strength preservation. The compiled WASM tests exercise the actual exports and manifest rather than a reimplementation.

`src/regions.rs` tests that separated rectangles retain disjoint context, use the original filter on dense/unrestricted input, and match the original single-rectangle kernel with only the planned scratch. The background rejection shortcut is checked against a 33³ colour cube at four skin ranges. `tests/regions.mjs` runs automatically from `tests/verify.mjs`: it covers dense, single, separated, ring-shaped, fragmented and empty masks, RGB/RGBA, alpha/HDR/NaN, radius 1/6/20, tone adjustment and independent strips. A two-patch fixture must allocate at most 16 scratch pages instead of the former 63; the optional previous-WASM oracle also covers these region cases.

The paired moment kernel is compared to independent box means on tiny images and widths crossing the 64-column block boundary. The opaque fast path is compared to the alpha-weighted path. Mask tests check both pixel preservation and that repeated `pc_filter` mask calls do not grow WASM memory at radii 1/6/20.

To compare the optimized kernel to a previous build, set `NATURAL_SKIN_REFERENCE_WASM=/path/to/previous.wasm` when running `tests/verify.mjs`. The oracle covers mask/effect modes, radius extremes, varying alpha, transparent payloads, and HDR/NaN pixels, with a maximum allowed RGB difference of `2e-6`.

Version 1.1.0 adds tests for independent skin brightening and positive/negative rosiness. They check luminance direction, redness direction, bounded finite outputs at gamut edges, unchanged background/HDR/transparent pixels, original skin-gate reuse after smoothing, neutral-control compatibility with previous WASM builds, and unchanged mask previews. Tone-only processing at `Strength=0` must not grow WASM memory after input/JSON allocation. Combined tone/smoothing is tested for alpha-scale invariance and independently padded-band agreement. The optional PhotoCraft CLI suite exercises the new controls for selection isolation, Undo, RGB16/32 saves, and a large-image comparison against unsplit WASM. The local 1.1.0 result is recorded in [verification-1.1.0-2026-11-07.json](verification-1.1.0-2026-11-07.json).

Texture protection is checked for invariance under uniform alpha scaling, using both uniform opacity and varying opacity with transparent holes. The regression compares RGB at alpha scales 0.1 and 0.5, radii 1/6/20, and Texture 60/100, while checking that alpha itself is preserved.

WASM regression checks compare padded whole-image results with independently processed strips at radii 1, 6, and 20. Both use the host's replicated-edge halo convention. The default maximum radius is 20 and required overlap is 40. This specifically catches seams caused by insufficient neighbourhood context.

`tests/verify.mjs` uses only Node.js core APIs, generates deterministic synthetic PNGs, and writes reports under `test-output/`. Set `NATURAL_SKIN_TEST_DIR` to override that destination. Passing a PhotoCraft CLI enables installation/execution, selected-region isolation, Undo, RGB16/32 saves, and a larger image comparison. Its reported performance includes CLI process launch, installation, image processing, and export.

## Measuring processing performance

Version 1.1.1 checks trimmed processing regions against full-buffer filtering at image corners and the centre, radii 1/6/20, Texture 0/60, RGB/RGBA, and invalid/HDR/transparent neighbours. The optional previous-WASM oracle includes these sparse cases. WASM tests assert one scratch page for a 48×40 processing region inside a 256×192 input, and zero scratch pages when no pixels match. Seam checks exercise both skin-only ROI and unrestricted processing. The CLI suite also compares two confirmed passes (smoothing, then Strength=0 colour adjustment) with an independently quantized WASM reference. Results are in [verification-1.1.1-2026-11-07.json](verification-1.1.1-2026-11-07.json).

`tests/benchmark.mjs` measures `plugin.run` using a persistent PhotoCraft CLI session and its actual wasmi interpreter. It excludes image opening, module installation, Undo, and export. Results still include the host's band padding, pixel conversion, dispatch, and selection blending; this is not an isolated inner-kernel measurement or a GUI interaction test.

```sh
NATURAL_SKIN_BENCH_REFERENCE=/path/to/previous.wasm \
NATURAL_SKIN_BENCH_SIZES=1536x1024 NATURAL_SKIN_BENCH_RUNS=3 \
node tests/benchmark.mjs \
  target/wasm32-unknown-unknown/release/photocraft_natural_skin.wasm \
  /path/to/photocraft-cli test-output/benchmark-comparison.json
```

For the 24 MP case use `NATURAL_SKIN_BENCH_SIZES=6000x4000` and `NATURAL_SKIN_BENCH_RUNS=1`. The script records processing-limit failures explicitly rather than counting them as a completed run. It tests the same deterministic opaque skin fixture with mask mode, default effect, and radius 20 with skin gating disabled. Reference and candidate order alternates AB/BA each round on the same document; Undo restores the input between samples. Run without other CPU-heavy work and retain the raw timings and module hashes. Timing results vary with host version, hardware, image content, and parameters.

Use `NATURAL_SKIN_BENCH_FIXTURE=sparse` for a central skin patch on a blue background (about 6% of input pixels); the default `dense` fixture covers the image with skin. `NATURAL_SKIN_BENCH_MODES=effect-default,effect-texture0,tone-only` selects smoothing, the zero-texture branch, and the second-pass colour-only workflow. Existing modes `mask` and `effect-radius20` remain available.

Region fixtures also include `islands` (two separated patches, about 4.8%), `ring` and `fragmented`. `effect-radius20-skin` keeps skin gating enabled at radius 20; the existing `effect-radius20` mode still disables it. Final region adoption measurements are recorded in [skin-region-performance-1.1.1.zh-CN.md](skin-region-performance-1.1.1.zh-CN.md).

## Initial local environment

- macOS on Apple Silicon; Rust 1.98.1.
- PhotoCraft CLI 0.20.2, commit `7c12f6471`, from the installed desktop application.
- Native kernel tests, the compiled WASM checks, and the PhotoCraft CLI integration suite passed.
- A NASA public-domain astronaut photograph supplied by [scikit-image v0.24.0](https://github.com/scikit-image/scikit-image/blob/v0.24.0/skimage/data/astronaut.png) was processed using the original SoftSkin build and visually inspected. The photograph is not included in this repository. The renamed build is checked separately with synthetic fixtures.

The renamed plugin's machine-readable local verification record is [verification-2026-11-06.json](verification-2026-11-06.json). Performance and visual quality depend on image size, parameters, lighting, and hardware.

## Limits of the evidence

CI uses a single Ubuntu job to build, test, and package the cross-platform WebAssembly module. It does not install PhotoCraft or establish GUI integration on any platform. Local CLI validation does not establish native menu layout, live-preview performance, smart-object round trips, or browser compatibility. Unsupported colour modes are rejected. Skin masking is a colour heuristic and must not be described as face recognition.
