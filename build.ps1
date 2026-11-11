$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot
cargo build --release --features gui --locked --offline --package natural-skin-companion --bins
if ($LASTEXITCODE -ne 0) { throw "Native build failed" }
$packageId = cargo pkgid --package natural-skin-companion
if ($LASTEXITCODE -ne 0) { throw "Could not read native package version" }
$version = ($packageId -split "@")[-1]
New-Item -ItemType Directory -Force dist | Out-Null
Copy-Item target/release/natural-skin-companion.exe, target/release/natural-skin.exe dist/
Copy-Item README.md, README.zh-CN.md, LICENSE, CHANGELOG.md, SECURITY.md, THIRD_PARTY_NOTICES.md dist/
Copy-Item docs dist/ -Recurse -Force
$hashes = foreach ($file in @("natural-skin-companion.exe", "natural-skin.exe")) {
    $hash = (Get-FileHash "dist/$file" -Algorithm SHA256).Hash.ToLower()
    "$hash  $file"
}
$hashes | Set-Content dist/SHA256SUMS.txt
$packageItems = @(
    "dist/natural-skin.exe", "dist/natural-skin-companion.exe",
    "dist/README.md", "dist/README.zh-CN.md", "dist/LICENSE", "dist/CHANGELOG.md",
    "dist/SECURITY.md", "dist/THIRD_PARTY_NOTICES.md", "dist/docs", "dist/SHA256SUMS.txt"
)
Compress-Archive -Path $packageItems -DestinationPath "dist/Natural-Skin-$version-Windows.zip" -Force
Write-Host "Packaged Natural Skin desktop development build and CLI in dist/."
