# Natural Skin Native

A native PhotoCraft companion built with **Rust + egui for Windows and macOS**. PhotoCraft itself stays unchanged; the WASM product has been retired.

Download versioned software from [GitHub Releases](https://github.com/wnanbei/photocraft-natural-skin/releases). Pushing a `native-v*` tag runs native builds for Windows x64 and both Mac architectures, then publishes the ZIPs and SHA-256 checksums as long-lived release assets. See [release instructions](docs/releasing.md).

The desktop development build opens a single-raster `.pcraft` or RGB TIFF, offers asynchronous cached previews, original/result comparison, a skin mask and retouch/tone controls, and exports a new full-resolution `.pcraft`. It shows actual stages and elapsed time, supports cancellation, and discards stale preview results. Version **0.3.0** adds a PhotoCraft-style workbench, independent Chinese/English selection, 1:1 detail inspection, SDR ICC processing and tiled exports. Persistent connection setup, single-instance activation, optional login startup and last-result recovery remain available. The [action handoff](docs/handoff-route-2026-11-09.zh-CN.md) captures a layer and returns a new host document without the editor control port.

[简体中文](README.zh-CN.md) · [Development and validation](docs/native-development-2026-11-08.zh-CN.md) · [Testing](docs/testing.md)

Requires Rust **1.95+**. Fetch once, then build offline:

```sh
cargo fetch --locked
sh build.sh
```

Open `dist/Natural Skin.app` on macOS. On Windows, run `build.ps1` locally and open `dist/natural-skin.exe`. Or run `cargo run --release --features gui --locked --offline --package natural-skin-companion --bin natural-skin`.

The toolbar’s **Language** selector switches the entire workbench between **简体中文** and **English**, immediately and independently of PhotoCraft. The choice persists across launches; the initial language follows the system locale. **Appearance** offers PhotoCraft-style Dark, Gray and Light palettes plus Follow system. Existing connection settings remain compatible.

On first launch, choose PhotoCraft in **Connection** and click **Enable connection**. In PhotoCraft, import the generated `Install Natural Skin.json` through **File → Scripts → Browse**, once. Select an ordinary pixel layer and run **Natural Skin Send** in its Actions panel. Natural Skin opens the capture; **Apply to PhotoCraft** processes the full source and opens a new host document. **Export Result** saves another new file. Existing outputs are never overwritten.

Closing the window minimizes it and keeps receiving; **Quit** exits. Login startup is opt-in. Subsequent launches restore the connection; a second launch activates the existing instance. Completed results are retained and can be recovered from **Connection**. Manage them through **Show saved files**; there is no automatic cleanup. Manual TIFF/pcraft opening remains available without setup. See the [desktop guide](docs/desktop-0.2.zh-CN.md).

The macOS bundle is locally ad-hoc signed, not notarized. Windows passes cross-target compilation checks but still requires runtime validation on Windows.

The pcraft adapter supports RGB/RGBA U8/U16/F32, alpha, hidden transparent RGB, ICC bytes, DPI, off-canvas tiles and edge padding. It checks ZIP structure, object hashes and resource budgets. Initial scope: one visible Normal raster layer at full opacity/fill, without masks, clipping, effects, selections, extra channels or timelines. Full images are capped at 36 MP. Fit previews are approximate and capped at 1024 pixels on the long side. Click the image or choose 1:1 detail for a physical-pixel crop; drag or use arrow keys to pan. Exports process the immutable full source in bounded tiles. Show Mask affects preview only.

SDR RGB matrix/TRC ICC profiles are converted to encoded sRGB for processing and back to the source profile on export; untagged input assumes sRGB. Invalid, LUT and PQ/HLG profiles are rejected. An optional display ICC transforms canvas output only. Automatic monitor matching, calibrated-display/real-portrait acceptance and HDR are unverified. Filter/cache/tone checkpoints support cancellation; file commit and some I/O remain non-interruptible. No ETA is shown. See the [implementation and acceptance notes](docs/quality-color-performance-2026-11-10.zh-CN.md).

```sh
./dist/natural-skin-companion process input.pcraft output.pcraft '{"Strength":45}'
./dist/natural-skin-companion --help
```

Historical [WASM records](docs/history/wasm/README.md) are no longer build instructions. [MIT](LICENSE).
