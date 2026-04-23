#!/usr/bin/env bash
# build_app.sh — Đóng gói claude-usage-tracker thành macOS .app bundle
# Usage: ./scripts/build_app.sh [arm64|x86_64|universal]
set -euo pipefail

BINARY="claude-usage-tracker"
APP_NAME="Claude Usage Tracker"
ARCH="${1:-universal}"

# Đọc version từ Cargo.toml
VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')

echo "==> Building $BINARY v$VERSION ($ARCH)..."

# Đảm bảo các target cần thiết đã được cài
case "$ARCH" in
  arm64)
    rustup target add aarch64-apple-darwin 2>/dev/null || true
    cargo build --release --target aarch64-apple-darwin
    BIN_PATH="target/aarch64-apple-darwin/release/$BINARY"
    TARBALL_SUFFIX="aarch64-apple-darwin"
    ;;
  x86_64)
    rustup target add x86_64-apple-darwin 2>/dev/null || true
    cargo build --release --target x86_64-apple-darwin
    BIN_PATH="target/x86_64-apple-darwin/release/$BINARY"
    TARBALL_SUFFIX="x86_64-apple-darwin"
    ;;
  universal)
    rustup target add aarch64-apple-darwin x86_64-apple-darwin 2>/dev/null || true
    cargo build --release --target aarch64-apple-darwin
    cargo build --release --target x86_64-apple-darwin
    mkdir -p target/universal-apple-darwin/release
    lipo -create \
      target/aarch64-apple-darwin/release/$BINARY \
      target/x86_64-apple-darwin/release/$BINARY \
      -output target/universal-apple-darwin/release/$BINARY
    BIN_PATH="target/universal-apple-darwin/release/$BINARY"
    TARBALL_SUFFIX="universal-apple-darwin"
    ;;
  *)
    echo "Unknown arch: $ARCH (use arm64 | x86_64 | universal)"
    exit 1
    ;;
esac

# Tạo thư mục dist
rm -rf dist
mkdir -p "dist/$APP_NAME.app/Contents/MacOS"
mkdir -p "dist/$APP_NAME.app/Contents/Resources"

echo "==> Copying binary..."
cp "$BIN_PATH" "dist/$APP_NAME.app/Contents/MacOS/$BINARY"
chmod +x "dist/$APP_NAME.app/Contents/MacOS/$BINARY"

echo "==> Writing Info.plist..."
# Patch version vào Info.plist
sed "s/<string>1.0.0<\/string>/<string>$VERSION<\/string>/g" Info.plist \
  > "dist/$APP_NAME.app/Contents/Info.plist"

echo "==> Converting icon..."
if command -v sips &>/dev/null && [ -f claude_ai_icon.jpg ]; then
  ICONSET="dist/AppIcon.iconset"
  mkdir -p "$ICONSET"
  for size in 16 32 64 128 256 512; do
    sips -z $size $size claude_ai_icon.jpg \
        --out "$ICONSET/icon_${size}x${size}.png" &>/dev/null
    sips -z $((size*2)) $((size*2)) claude_ai_icon.jpg \
        --out "$ICONSET/icon_${size}x${size}@2x.png" &>/dev/null
  done
  iconutil -c icns "$ICONSET" -o "dist/$APP_NAME.app/Contents/Resources/AppIcon.icns" 2>/dev/null || true
  rm -rf "$ICONSET"
fi

# Copy plist resource cho manual launchd install
cp com.user.claude-usage-tracker.plist "dist/$APP_NAME.app/Contents/Resources/"

echo ""
echo "✅  App bundle: dist/$APP_NAME.app"
echo ""

# Tạo archives
echo "==> Creating archives..."
cd dist

# .app zip (để distribute thủ công)
zip -r "${BINARY}-${TARBALL_SUFFIX}.zip" "$APP_NAME.app" --quiet
echo "   dist/${BINARY}-${TARBALL_SUFFIX}.zip"

# Tarball standalone binary (cho Homebrew formula)
if [ "$ARCH" = "universal" ]; then
  for t in aarch64-apple-darwin x86_64-apple-darwin; do
    mkdir -p "pkg-$t"
    lipo -thin "${t%%-apple-darwin}" "../$BIN_PATH" \
      -output "pkg-$t/$BINARY" 2>/dev/null || cp "../$BIN_PATH" "pkg-$t/$BINARY"
    cp "../com.user.claude-usage-tracker.plist" "pkg-$t/"
    tar czf "${BINARY}-${t}.tar.gz" "pkg-$t/"
    echo "   dist/${BINARY}-${t}.tar.gz"
    SHA=$(shasum -a 256 "${BINARY}-${t}.tar.gz" | awk '{print $1}')
    echo "   SHA256 ($t): $SHA"
  done
else
  mkdir -p "pkg"
  cp "$APP_NAME.app/Contents/MacOS/$BINARY" "pkg/"
  cp "../com.user.claude-usage-tracker.plist" "pkg/"
  tar czf "${BINARY}-${TARBALL_SUFFIX}.tar.gz" "pkg/"
  echo "   dist/${BINARY}-${TARBALL_SUFFIX}.tar.gz"
  SHA=$(shasum -a 256 "${BINARY}-${TARBALL_SUFFIX}.tar.gz" | awk '{print $1}')
  echo "   SHA256: $SHA"
fi

echo ""
echo "==> Done! Để cài .app:"
echo "   cp -r 'dist/$APP_NAME.app' /Applications/"
echo "   open '/Applications/$APP_NAME.app'"

