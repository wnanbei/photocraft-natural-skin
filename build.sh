#!/usr/bin/env sh
set -eu
cd -- "$(dirname -- "$0")"
cargo build --release --locked --offline --target wasm32-unknown-unknown
node scripts/package.mjs "$@"
