$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot
cargo build --release --features gui --locked --offline --package natural-skin-companion --bins
if ($LASTEXITCODE -ne 0) { throw "Native build failed" }
New-Item -ItemType Directory -Force dist | Out-Null
Copy-Item target/release/natural-skin-companion.exe, target/release/natural-skin.exe dist/
Copy-Item README.md, README.zh-CN.md, LICENSE, CHANGELOG.md, SECURITY.md, THIRD_PARTY_NOTICES.md dist/
Copy-Item docs dist/ -Recurse -Force
$hashes = foreach ($file in @("natural-skin-companion.exe", "natural-skin.exe")) {
    $hash = (Get-FileHash "dist/$file" -Algorithm SHA256).Hash.ToLower()
    "$hash  $file"
}
$hashes | Set-Content dist/SHA256SUMS.txt
$packageItems = Get-ChildItem dist | Where-Object { $_.Extension -ne ".zip" }
Compress-Archive -Path $packageItems.FullName -DestinationPath dist/Natural-Skin-0.2.0-Windows.zip -Force
Write-Host "Packaged Natural Skin desktop development build and CLI in dist/."
