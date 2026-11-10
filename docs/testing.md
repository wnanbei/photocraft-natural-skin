# Native backend testing

Fetch dependencies once with `cargo fetch --locked`, then:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --features gui --locked --offline -- -D warnings
cargo test --workspace --features gui --locked --offline
cargo build --release --features gui --locked --offline --package natural-skin-companion
```

The native tests cover the existing guided filter, disjoint-region context, alpha/HDR/NaN, skin gates and tone controls, cache invalidation/reuse, expanded skin ranges, source immutability, cancellation, TIFF precision/profile/resolution, loopback-only transport and stale source guards.

The desktop probe launches and closes its own PhotoCraft instance using an isolated config, fresh token and exchange directory. It does not discover or connect to another running editor. It requires a graphical desktop; tests only use synthetic images.

```sh
# Set PHOTOCRAFT_APP for an executable outside the default macOS bundle.
python3 native/tests/probe_control.py
# Raw multi-tile desktop exchange (separate latest-pcraft-run.json):
NATURAL_SKIN_EXCHANGE=pcraft python3 native/tests/probe_control.py
# Optional, uses the zstd CLI to inspect one known 64x48 RGBA32 fixture.
python3 native/tests/probe_raw_snapshot.py
```

The probe tests actual desktop methods, rejected alternatives, native Rust authentication, capture/source preservation, 8/16/32-bit identity and retouch round trips, ICC/resolution/alpha, two-step Undo and rejecting a changed source. The optional `probe_raw_snapshot.py` is the original read-only research probe. The pcraft mode of `probe_control.py` exercises the implemented Rust reader/writer. Neither test proves an arbitrary concurrent user workflow safe.

```sh
./target/release/natural-skin-companion bench-preview 512 384
./target/release/natural-skin-companion bench-preview 1024 768
./target/release/natural-skin-companion bench 6000 4000
```

Benchmark source construction is excluded. Preview results include source copying and cached mixing; full-kernel results include scratch allocation. Both exclude editor capture/return, TIFF I/O and display upload. Do not compare them as end-to-end speedup factors against historical WASM host timings. Keep the raw samples, build hash and machine/version details, and run without simultaneous builds or integration tests.

See the [feasibility report](native-feasibility-2026-11-08.zh-CN.md) for current findings. Prior WASM measurements are [historical](history/wasm/testing.md).

The GUI uses the same tested worker and native adapters. Run `cargo run --release --features gui --package natural-skin-companion --bin natural-skin` for visual/interaction acceptance. Check Open, drag/drop, Original/Retouched, all sliders, Show Mask, Reset, cancellation, Export, and failure states. Cross-target Windows compilation is `cargo check --workspace --all-targets --features gui --target x86_64-pc-windows-gnu --locked --offline`; it does not link an executable or run Windows UI. See the [development report](native-development-2026-11-08.zh-CN.md).

Action-button handoff is a separate, no-control-port experiment. `python3 native/tests/probe_handoff.py` prepares an isolated macOS copy/config and keeps the owned test processes alive for actual UI interaction. See [handoff validation](handoff-route-2026-11-09.zh-CN.md); setup alone is not an end-to-end pass.

Desktop 0.2 setup is exercised with `python3 native/tests/probe_handoff.py --desktop`. It starts the GUI with only an isolated `--data-dir`, without `--watch` or `--return-to`. Select the test PhotoCraft bundle in Connection, Enable connection, import the GUI-generated installer, and run Natural Skin Send. Confirm minimize/reentry, mask-safe Apply, a new host document, remembered settings and last-result recovery. `--reuse RUN --desktop` reuses the same isolated state. The helper holds processes; UI actions and pixel comparisons are still required. Never enable the real account's login startup during this probe; the LaunchAgent test uses its own temporary directory.

Settings/activation tests cover invalid-state preservation, single-instance lock recovery, queued opens during export, connection persistence and invalid-target rollback. Windows cross-target checks do not exercise its Run-key registration; real Windows startup and host launch remain acceptance items.

## Detail, colour and resource acceptance

`cargo test` includes full-kernel vs tile/crop seams at maximum radius, RGB/RGBA alpha and metadata preservation, ICC reference conversions, invalid profile refusal, cancellation during cache/filter work, synthetic skin-shade noise/texture checks and stable progress geometry.

```sh
# Builds must finish before measurements; use a fresh output directory.
python3 native/tests/benchmark_acceptance.py --output test-output/acceptance-2026-11-10
# Isolated GUI preparation; UI interaction and screenshots remain manual.
python3 native/tests/probe_quality_ui.py --run test-output/quality-ui-run --source test-output/acceptance-2026-11-10/quality/synthetic-original.tiff
```

Check Fit → click a point → 1:1, physical-pixel dimensions, drag and arrow-key panning, Original/Retouched/Mask at the same crop, Fit restoration, resize/minimum-window layout, profile selection/reset/error and full-source export while in detail mode. Profile output is manual and temporary. Inspect synthetic TIFF outputs in the native application; they are not photographs and do not certify portrait quality or a calibrated display. See the [acceptance scope](quality-color-performance-2026-11-10.zh-CN.md).
