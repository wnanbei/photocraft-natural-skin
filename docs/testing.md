# Testing and compatibility

## What is verified

`node --test tests/version.test.mjs` checks semantic version precedence, synchronized metadata, draft/history handling, invalid-date and placeholder rejection, and no mutation on rejected operations. Fixtures live in temporary directories; they do not bump or date the working repository.

The native Rust tests compare sliding-window statistics to a naive reference and cover tiny images, transparent samples, colour gating, parameter decoding, and zero-strength preservation. The compiled WASM tests exercise the actual exports and manifest rather than a reimplementation.

Texture protection is checked for invariance under uniform alpha scaling, using both uniform opacity and varying opacity with transparent holes. The regression compares RGB at alpha scales 0.1 and 0.5, radii 1/6/20, and Texture 60/100, while checking that alpha itself is preserved.

WASM regression checks compare padded whole-image results with independently processed strips at radii 1, 6, and 20. Both use the host's replicated-edge halo convention. The default maximum radius is 20 and required overlap is 40. This specifically catches seams caused by insufficient neighbourhood context.

`tests/verify.mjs` uses only Node.js core APIs, generates deterministic synthetic PNGs, and writes reports under `test-output/`. Set `NATURAL_SKIN_TEST_DIR` to override that destination. Passing a PhotoCraft CLI enables installation/execution, selected-region isolation, Undo, RGB16/32 saves, and a larger image comparison. Its reported performance includes CLI process launch, installation, image processing, and export.

## Initial local environment

- macOS on Apple Silicon; Rust 1.98.1.
- PhotoCraft CLI 0.20.2, commit `7c12f6471`, from the installed desktop application.
- Native kernel tests, the compiled WASM checks, and the PhotoCraft CLI integration suite passed.
- A NASA public-domain astronaut photograph supplied by [scikit-image v0.24.0](https://github.com/scikit-image/scikit-image/blob/v0.24.0/skimage/data/astronaut.png) was processed using the original SoftSkin build and visually inspected. The photograph is not included in this repository. The renamed build is checked separately with synthetic fixtures.

The renamed plugin's machine-readable local verification record is [verification-2026-11-06.json](verification-2026-11-06.json). Performance and visual quality depend on image size, parameters, lighting, and hardware.

## Limits of the evidence

CI uses a single Ubuntu job to build, test, and package the cross-platform WebAssembly module. It does not install PhotoCraft or establish GUI integration on any platform. Local CLI validation does not establish native menu layout, live-preview performance, smart-object round trips, or browser compatibility. Unsupported colour modes are rejected. Skin masking is a colour heuristic and must not be described as face recognition.
