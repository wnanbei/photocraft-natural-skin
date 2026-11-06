$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot
cargo build --release --locked --offline --target wasm32-unknown-unknown
if ($LASTEXITCODE -ne 0) { throw 'Rust compilation failed.' }
node scripts/package.mjs @args
if ($LASTEXITCODE -ne 0) { throw 'Packaging failed.' }
