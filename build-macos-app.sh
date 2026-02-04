#!/bin/bash
set -e

APP_NAME="Switchboard"
BUNDLE_ID="com.switchboard.app"
VERSION="1.0.0"
BINARY_NAME="switchboard-ui"

echo "🔨 Building release binary..."
cargo build --release

echo "📦 Creating app bundle structure..."
APP_DIR="target/macos/${APP_NAME}.app"
rm -rf "${APP_DIR}"
mkdir -p "${APP_DIR}/Contents/MacOS"
mkdir -p "${APP_DIR}/Contents/Resources"

echo "📋 Creating Info.plist..."
cat > "${APP_DIR}/Contents/Info.plist" << EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleExecutable</key>
    <string>${BINARY_NAME}</string>
    <key>CFBundleIdentifier</key>
    <string>${BUNDLE_ID}</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>${APP_NAME}</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>${VERSION}</string>
    <key>CFBundleVersion</key>
    <string>${VERSION}</string>
    <key>LSMinimumSystemVersion</key>
    <string>10.13</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSHumanReadableCopyright</key>
    <string>Copyright © 2026</string>
</dict>
</plist>
EOF

echo "📝 Copying binary..."
cp "target/release/${BINARY_NAME}" "${APP_DIR}/Contents/MacOS/"

echo "✅ App bundle created at: ${APP_DIR}"
echo ""
echo "To run: open ${APP_DIR}"
echo "To copy to Applications: cp -r ${APP_DIR} /Applications/"
