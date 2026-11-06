# Contributing

Bug reports, reproducible image-processing issues, documentation fixes, and small focused pull requests are welcome. English and Chinese reports are both fine.

## Local development

Use stable Rust with `wasm32-unknown-unknown`, rustfmt, and Clippy, plus Node.js 24 or newer. There are no Cargo or npm dependencies to install beyond the Rust standard library target. The full check sequence is in [README.md](README.md#verify).

`src/lib.rs` contains the allocation-free filtering kernel and the WASM ABI wrapper. `manifest.json` is embedded at compile time and declares plugin identity, controls, and required overlap. `tests/verify.mjs` runs the compiled module directly and optionally drives PhotoCraft's actual CLI. `scripts/package.mjs` validates identity and versions before preparing distributable files. `scripts/version.mjs` synchronizes release metadata; `tests/version.test.mjs` exercises version bumps, changelog preparation, drift rejection, and prerelease precedence.

When changing filter behaviour, add regression checks that demonstrate the user-visible failure and exercise the real compiled WASM. Preserve alpha, selections, zero-strength identity, transparent/HDR handling, and independence between host bands. The guided filter's influence extends to twice the radius; if the maximum radius changes, update the manifest overlap and seam tests together.

Never replace an unsupported colour mode with an unchecked RGB interpretation. Keep the WASM import-free and avoid SIMD or WASI features unsupported by PhotoCraft ABI v1. Check user-facing controls, errors, and mask-preview behaviour against both READMEs.

## Pull requests

Explain what changes, why it matters, and which checks you ran. Include PhotoCraft version and OS for integration claims. Distinguish engine/CLI validation from real GUI checks. Use only images you may share, and avoid including personal portraits in a public bug report; a synthetic reproduction is preferable.

Update the current draft changelog for user-visible changes. Follow [the version policy](docs/versioning.md); use the version script rather than editing independent version copies, and never move published tags. Build outputs and generated fixture files belong in `dist/`, `target/`, or `test-output/`, which are ignored. Do not commit the `.wasm` binary to source control; releases and CI artifacts distribute it.

Contributions are provided under the repository's [MIT license](LICENSE).
