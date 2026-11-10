#!/usr/bin/env sh
set -eu
cd -- "$(dirname -- "$0")"
cargo build --release --features gui --locked --offline --package natural-skin-companion --bins
mkdir -p dist
cp target/release/natural-skin-companion target/release/natural-skin dist/
cp README.md README.zh-CN.md LICENSE CHANGELOG.md SECURITY.md THIRD_PARTY_NOTICES.md dist/
cp -R docs dist/
if [ "$(uname -s)" = Darwin ]; then
  mkdir -p 'dist/Natural Skin.app/Contents/MacOS'
  cp target/release/natural-skin 'dist/Natural Skin.app/Contents/MacOS/natural-skin'
  mkdir -p 'dist/Natural Skin.app/Contents/Resources'
  cp LICENSE THIRD_PARTY_NOTICES.md 'dist/Natural Skin.app/Contents/Resources/'
  cat > 'dist/Natural Skin.app/Contents/Info.plist' <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>natural-skin</string>
<key>CFBundleIdentifier</key><string>io.github.wnanbei.natural-skin.native</string>
<key>CFBundleName</key><string>Natural Skin</string>
<key>CFBundleDevelopmentRegion</key><string>en</string>
<key>CFBundleLocalizations</key><array><string>en</string><string>zh_CN</string></array>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>0.3.0</string>
<key>CFBundleVersion</key><string>3</string>
<key>NSHighResolutionCapable</key><true/>
<key>CFBundleDocumentTypes</key><array><dict>
<key>CFBundleTypeName</key><string>PhotoCraft document or TIFF</string>
<key>CFBundleTypeRole</key><string>Editor</string>
<key>LSHandlerRank</key><string>Alternate</string>
<key>CFBundleTypeExtensions</key><array><string>pcraft</string><string>tif</string><string>tiff</string></array>
</dict></array>
</dict></plist>
PLIST
  codesign --force --sign - 'dist/Natural Skin.app'
  ditto -c -k --sequesterRsrc --keepParent 'dist/Natural Skin.app' "dist/Natural-Skin-0.3.0-macOS-$(uname -m).zip"
fi
if command -v shasum >/dev/null 2>&1; then
  (cd dist && shasum -a 256 natural-skin-companion natural-skin > SHA256SUMS.txt)
else
  (cd dist && sha256sum natural-skin-companion natural-skin > SHA256SUMS.txt)
fi
printf '%s\n' 'Packaged Natural Skin desktop development build and CLI in dist/.'
