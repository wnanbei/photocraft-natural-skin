# Contributing

The target is a native Natural Skin companion. Do not modify PhotoCraft or reintroduce a second WASM product. `src/` contains the native retouching core and preview cache; `native/` contains TIFF/pcraft exchange, the worker, desktop GUI, loopback client and CLI.

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --features gui --locked --offline -- -D warnings
cargo test --workspace --features gui --locked --offline
cargo build --release --features gui --locked --offline --package natural-skin-companion
```

Run `cargo fetch --locked` first on a clean machine. Tests use synthetic fixtures. Local desktop integration must use a separate PhotoCraft configuration and document set; never attach a test to the user's ordinary editor instance. See `docs/testing.md`.

Keep input immutable across preview requests and publish only completed current results. Invalidate the cache on source/geometry/scale changes and its filter parameters. Test changed skin ranges outside a prior mask. Preserve alpha/HDR/invalid samples and independent-region context. Cancellation may discard a working buffer; it must not publish it.

Control connections stay on loopback, authenticate with an explicit private token file and use dedicated read/write roots. Never log tokens or retry a timed-out mutation automatically. A source preflight is not atomic compare-and-apply; document the remaining race instead of claiming it is solved.

ICC metadata preservation is not display colour management. Report exact versions and what actually ran; distinguish kernel timing, file exchange, desktop integration and visible GUI behavior. No source photographs or compiled binaries in commits. Generated results belong under ignored `test-output/`, `target/` or `dist/`.

[MIT](LICENSE)
