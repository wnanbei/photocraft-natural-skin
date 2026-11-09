> 历史 WASM 版本记录：已停止维护，当前开发目标为原生伴随程序。

# Natural Skin 1.0.1 performance verification

Measured on 2026-11-07, macOS arm64, PhotoCraft CLI 0.20.2 (`7c12f6471`). The benchmark runs the actual wasmi interpreter used by PhotoCraft, not Node's optimizing WebAssembly runtime.

## Processing time

For a deterministic opaque skin texture at **1536 × 1024**, the following values are medians of three sequential samples per build. Reference and candidate run in the same host session, with Undo restoring the source between samples.

| Mode | 1.0.0 | 1.0.1 | Time reduction |
| --- | ---: | ---: | ---: |
| Mask, default skin range | 553 ms | 357 ms | 35.4% |
| Default effect | 5,449 ms | 3,510 ms | 35.6% |
| Radius 20, skin gating disabled | 3,723 ms | 2,575 ms | 30.8% |

At **6000 × 4000 (24 MP)**, single-run results on the same synthetic skin fixture were:

| Mode | 1.0.0 | 1.0.1 | Time reduction |
| --- | ---: | ---: | ---: |
| Mask, default skin range | 6.96 s | 4.60 s | 34.0% |
| Default effect | 51.47 s | 31.01 s | 39.8% |
| Radius 20, skin gating disabled | 45.99 s | 31.94 s | 30.5% |

Timings measure `plugin.run` only: host band padding, pixel conversion, interpreter execution, and selection blending are included. Opening images, installation, Undo, and exporting are excluded. These are CLI processing measurements, not GUI slider/checkbox response times. Results vary with image content, CPU load, thermal state, host version, and parameters. They do not promise real-time processing of large photos.

## What changed

- Mask previews write grayscale directly into the pixel buffer and allocate **zero filter scratch planes**, instead of seven `f32` planes (28 bytes per padded input pixel). Input buffers and the host's own memory are still required.
- Skin-colour pixels on the constant parts of the feather ramps return exactly 0 or 1 without evaluating the full ramps; feather transitions keep the original formula.
- Guided-filter moments are averaged in pairs, sharing border/index work. The vertical pass walks contiguous 64-column spans with fixed stack storage.
- Opaque input avoids alpha-weighted divisions and the weight-mean blur. Bands with no affected pixels skip neighbourhood filtering.
- The original channel is restored after using its plane as temporary moment storage, preserving texture calculations without allocating another plane.

## Correctness and compatibility

- Nine Rust tests, twelve version-tool tests, native/WASM Clippy, and the actual PhotoCraft CLI integration suite pass.
- The previous-release WASM oracle reports maximum sample difference **0** for effect/mask fixtures and a 33³ RGB colour cube at skin ranges 0/20/60/100. This is evidence for those fixtures, not proof for every image.
- Regression checks cover the alpha-weighted/opaque paths, transparent payloads, HDR/NaN samples, RGB without alpha, 16/32-bit documents, selections, Undo, and independently padded bands at radii 1/6/20.
- Repeated mask calls do not grow the module's linear memory after the caller allocates its pixel/JSON buffers.
- Plugin ID, parameter keys, default values, overlap, and ABI version are unchanged. Plugin-authored UI text uses English consistently. PhotoCraft 0.20.2 independently translates some generated labels using its own UI language; the plugin cannot override that. An entirely English dialog requires an English host UI.

Build SHA-256:

- 1.0.0 reference: `7b1e4c81e9bf3a0fe38c49d526ee401fbd3cbe41329a39b3986e75e03890574b`
- 1.0.1 benchmark candidate (before the subsequent UI-text-only correction): `d2df46aab1c07c504416b3417f9c4766873072336ef9f04341a3c0f8c70ae5ae`

See [testing.md](testing.md#measuring-processing-performance) for reproduction commands. The machine-readable evidence is [performance-2026-11-07.json](performance-2026-11-07.json).
