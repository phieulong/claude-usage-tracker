#!/usr/bin/env bash
# Claude Usage Tracker — one-line installer
# curl -fsSL https://raw.githubusercontent.com/phieulong/claude-usage-tracker/main/install.sh | bash
set -euo pipefail

REPO="phieulong/claude-usage-tracker"
BINARY="claude-usage-tracker"
INSTALL_DIR="/usr/local/bin"
PLIST_LABEL="com.phieulong.claude-usage-tracker"
PLIST_DST="$HOME/Library/LaunchAgents/${PLIST_LABEL}.plist"

# ── Colors ────────────────────────────────────────────────────────────────────
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; BLUE='\033[0;34m'; NC='\033[0m'
info()    { echo -e "${BLUE}==>${NC} $*"; }
success() { echo -e "${GREEN} ✓${NC} $*"; }
warn()    { echo -e "${YELLOW} !${NC} $*"; }
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
info "Latest version: $LATEST"

# ── Download ──────────────────────────────────────────────────────────────────
TARBALL="${BINARY}-${TARBALL_ARCH}.tar.gz"
URL="https://github.com/${REPO}/releases/download/${LATEST}/${TARBALL}"

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

info "Downloading $TARBALL..."
curl -fsSL --progress-bar "$URL" -o "$TMP/$TARBALL" \
  || die "Download failed. URL: $URL"

# ── Extract ───────────────────────────────────────────────────────────────────
info "Extracting..."
tar xzf "$TMP/$TARBALL" -C "$TMP"

# ── Install binary ────────────────────────────────────────────────────────────
# Prefer /opt/homebrew/bin if it exists (Apple Silicon Homebrew)
if [ -d "/opt/homebrew/bin" ]; then
  INSTALL_DIR="/opt/homebrew/bin"
fi

info "Installing to $INSTALL_DIR/$BINARY..."
if [ -w "$INSTALL_DIR" ]; then
  cp "$TMP/$BINARY" "$INSTALL_DIR/$BINARY"
else
  sudo cp "$TMP/$BINARY" "$INSTALL_DIR/$BINARY"
fi
chmod +x "$INSTALL_DIR/$BINARY"

# Remove macOS quarantine flag (avoids Gatekeeper block on first run)
xattr -d com.apple.quarantine "$INSTALL_DIR/$BINARY" 2>/dev/null || true

success "Binary installed: $INSTALL_DIR/$BINARY"

# ── Setup launchd (auto-start at login) ──────────────────────────────────────
echo ""
read -r -p "$(echo -e "${YELLOW}?${NC} Start automatically at login? [Y/n]: ")" AUTOSTART
AUTOSTART="${AUTOSTART:-Y}"

if [[ "$AUTOSTART" =~ ^[Yy]$ ]]; then
  # Unload old version if running
  launchctl unload "$PLIST_DST" 2>/dev/null || true

  # Write plist
  mkdir -p "$HOME/Library/LaunchAgents"
  cat > "$PLIST_DST" << PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>${PLIST_LABEL}</string>
    <key>ProgramArguments</key>
    <array>
        <string>${INSTALL_DIR}/${BINARY}</string>
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
PLIST

  launchctl load "$PLIST_DST"
  success "Auto-start enabled (launchd)"
  success "Claude Usage Tracker is now running in the menu bar!"
fi

# ── Done ──────────────────────────────────────────────────────────────────────
echo ""
echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${GREEN} Claude Usage Tracker ${LATEST} installed!${NC}"
echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""
echo "  Run now:   $BINARY daemon"
echo "  Check:     $BINARY status"
echo "  Logs:      tail -f /tmp/claude-usage-tracker.log"
echo ""
echo "  Uninstall: rm $INSTALL_DIR/$BINARY && launchctl unload $PLIST_DST && rm $PLIST_DST"
echo ""

