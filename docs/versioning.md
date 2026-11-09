# Native versions

`native/Cargo.toml` versions the cross-platform desktop application and CLI (currently 0.2.0, development stage). `natural-skin-core` retains the inherited algorithm version 1.1.1; it is not a separate WASM release. Both Rust workspace members are unpublished.

The macOS Info.plist in `build.sh` currently contains the matching 0.2.0/2 development version. Update it when bumping the native package. Windows is currently a portable executable build, with installer/version resources planned for release packaging.

Native releases have not started. Do not create one by pushing an old WASM tag or move any published historical tag. CI uploads development artifacts, not releases.
