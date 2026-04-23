#!/usr/bin/env bash
# Claude Usage Tracker — one-line installer
# curl -fsSL https://raw.githubusercontent.com/phieulong/claude-usage-tracker/main/install.sh | bash
set -euo pipefail

REPO="phieulong/claude-usage-tracker"
BINARY="claude-usage-tracker"
APP_NAME="Claude Usage Tracker"
APP_DIR="/Applications/${APP_NAME}.app"
BUNDLE_ID="com.phieulong.claude-usage-tracker"
PLIST_LABEL="$BUNDLE_ID"
PLIST_DST="$HOME/Library/LaunchAgents/${PLIST_LABEL}.plist"

# ── Colors ────────────────────────────────────────────────────────────────────
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; BLUE='\033[0;34m'; NC='\033[0m'
info()    { echo -e "${BLUE}==>${NC} $*"; }
success() { echo -e "${GREEN} ✓${NC} $*"; }
die()     { echo -e "${RED}Error:${NC} $*" >&2; exit 1; }

# ── Platform check ────────────────────────────────────────────────────────────
[ "$(uname)" = "Darwin" ] || die "This tool only supports macOS."

ARCH=$(uname -m)
case "$ARCH" in
  arm64)  TARBALL_ARCH="aarch64-apple-darwin" ;;
  x86_64) TARBALL_ARCH="x86_64-apple-darwin"  ;;
  *)      die "Unsupported architecture: $ARCH" ;;
esac

# ── Find latest release ───────────────────────────────────────────────────────
info "Fetching latest release..."
LATEST=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
  | grep '"tag_name"' | sed 's/.*"tag_name": *"\(.*\)".*/\1/')
[ -n "$LATEST" ] || die "Could not fetch release info. Check your internet connection."
VERSION="${LATEST#v}"
info "Latest version: $LATEST"

# ── Download binary ───────────────────────────────────────────────────────────
TARBALL="${BINARY}-${TARBALL_ARCH}.tar.gz"
URL="https://github.com/${REPO}/releases/download/${LATEST}/${TARBALL}"

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

info "Downloading binary for $ARCH..."
curl -fsSL --progress-bar "$URL" -o "$TMP/$TARBALL" \
  || die "Download failed."
tar xzf "$TMP/$TARBALL" -C "$TMP"

# ── Create .app bundle ────────────────────────────────────────────────────────
info "Creating ${APP_NAME}.app..."

sudo rm -rf "$APP_DIR"
sudo mkdir -p "$APP_DIR/Contents/MacOS"
sudo mkdir -p "$APP_DIR/Contents/Resources"

# Binary
sudo cp "$TMP/$BINARY" "$APP_DIR/Contents/MacOS/$BINARY"
sudo chmod +x "$APP_DIR/Contents/MacOS/$BINARY"

# Info.plist (LSUIElement=true → ẩn Dock, không mở Terminal)
sudo tee "$APP_DIR/Contents/Info.plist" > /dev/null << PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>${APP_NAME}</string>
    <key>CFBundleDisplayName</key>
    <string>${APP_NAME}</string>
    <key>CFBundleIdentifier</key>
    <string>${BUNDLE_ID}</string>
    <key>CFBundleVersion</key>
    <string>${VERSION}</string>
    <key>CFBundleShortVersionString</key>
    <string>${VERSION}</string>
    <key>CFBundleExecutable</key>
    <string>${BINARY}</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>LSUIElement</key>
    <true/>
    <key>LSMinimumSystemVersion</key>
    <string>12.0</string>
    <key>NSPrincipalClass</key>
    <string>NSApplication</string>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
PLIST

# Remove quarantine flag → Gatekeeper không chặn khi double-click
sudo xattr -rd com.apple.quarantine "$APP_DIR" 2>/dev/null || true

# Re-register với Launch Services → Spotlight index ngay
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister \
  -f "$APP_DIR" 2>/dev/null || true

success ".app bundle: $APP_DIR"

# ── Symlink CLI vào PATH ──────────────────────────────────────────────────────
CLI_TARGET="$APP_DIR/Contents/MacOS/$BINARY"
if [ -d "/opt/homebrew/bin" ]; then
  CLI_LINK="/opt/homebrew/bin/$BINARY"
else
  CLI_LINK="/usr/local/bin/$BINARY"
fi
sudo ln -sf "$CLI_TARGET" "$CLI_LINK"
success "CLI symlink: $CLI_LINK → $CLI_TARGET"

# ── Auto-start at login (launchd) ────────────────────────────────────────────
echo ""
# read phải đọc từ /dev/tty vì stdin đang là pipe (curl | bash)
read -r -p "$(echo -e "${YELLOW}?${NC} Tự chạy khi login? [Y/n]: ")" AUTOSTART < /dev/tty || AUTOSTART="Y"
AUTOSTART="${AUTOSTART:-Y}"

if [[ "$AUTOSTART" =~ ^[Yy]$ ]]; then
  launchctl unload "$PLIST_DST" 2>/dev/null || true
  mkdir -p "$HOME/Library/LaunchAgents"
  cat > "$PLIST_DST" << LPLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>${PLIST_LABEL}</string>
    <key>ProgramArguments</key>
    <array>
        <string>${APP_DIR}/Contents/MacOS/${BINARY}</string>
        <string>daemon</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>/tmp/claude-usage-tracker.log</string>
    <key>StandardErrorPath</key>
    <string>/tmp/claude-usage-tracker.err</string>
    <key>EnvironmentVariables</key>
    <dict>
        <key>RUST_LOG</key>
        <string>info</string>
    </dict>
</dict>
</plist>
LPLIST
  launchctl load "$PLIST_DST"
  success "Auto-start khi login: bật"
fi

# ── Chạy ngay ─────────────────────────────────────────────────────────────────
echo ""
read -r -p "$(echo -e "${YELLOW}?${NC} Chạy ngay bây giờ? [Y/n]: ")" RUNNOW < /dev/tty || RUNNOW="Y"
RUNNOW="${RUNNOW:-Y}"
if [[ "$RUNNOW" =~ ^[Yy]$ ]]; then
  open "$APP_DIR"
  success "Đang chạy — nhìn lên menu bar!"
fi

# ── Done ──────────────────────────────────────────────────────────────────────
echo ""
echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${GREEN}  Claude Usage Tracker ${LATEST} đã cài xong!${NC}"
echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""
echo "  Spotlight:  Cmd+Space → gõ \"Claude Usage\""
echo "  CLI:        $BINARY daemon"
echo "  Logs:       tail -f /tmp/claude-usage-tracker.log"
echo ""
echo "  Gỡ cài:     sudo rm -rf \"$APP_DIR\" && rm -f \"$CLI_LINK\""
[ -f "$PLIST_DST" ] && echo "              launchctl unload \"$PLIST_DST\" && rm \"$PLIST_DST\""
echo ""

