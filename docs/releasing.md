# Native development packaging

`native/Cargo.toml` versions the desktop application and CLI (currently 0.2.0). Native releases have not been published. The old WASM tag-triggered workflow is retired.

After `cargo fetch --locked`, `sh build.sh` builds both executables with the `gui` feature, packages documentation and checksums, and creates a locally ad-hoc-signed macOS `Natural Skin.app` and a permission-preserving ZIP. `build.ps1` creates the two Windows executables and checksums plus a portable ZIP on Windows. Build scripts are repeatable and do not publish, install or modify PhotoCraft.

The GitHub CI matrix builds and tests on Windows/macOS and uploads development artifacts. This workflow has been configured locally; its remote result must be checked before claiming it passed. The macOS ZIP produced by build.sh preserves bundle permissions; public distribution signing/notarization and a signed Windows installer remain future work.

The current bundle is a development build, not a notarized installer or a completed colour-managed product. Before public release: finish real Windows runtime acceptance, real-photo/colour acceptance, automatic exchange coordination if offered, bounded job cancellation/memory behavior, dependency notices and OS signing/packaging. Do not move historical published tags.
