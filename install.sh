#!/usr/bin/env bash
set -e

echo "======================================================"
echo "        🚀 Installing GNOMEscroll"
echo "======================================================"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN_DIR="$HOME/.local/bin"
SHARE_DIR="$HOME/.local/share/gnomescroll"
EXT_DIR="$HOME/.local/share/gnome-shell/extensions/gnomescroll@kalpit.github.com"

# 1. Build release binary if not built yet
if [ ! -f "$SCRIPT_DIR/target/release/gnomescroll" ]; then
    echo "📦 Building optimized release binary..."
    cargo build --release
fi

# 2. Install binary
echo "📁 Installing binary to $BIN_DIR/gnomescroll..."
mkdir -p "$BIN_DIR"
cp -f "$SCRIPT_DIR/target/release/gnomescroll" "$BIN_DIR/gnomescroll"
chmod +x "$BIN_DIR/gnomescroll"

# 3. Install scripts & assets
echo "📁 Installing support files to $SHARE_DIR..."
mkdir -p "$SHARE_DIR"
cp -f "$SCRIPT_DIR/scripts/floating_pill.py" "$SHARE_DIR/floating_pill.py"
chmod +x "$SHARE_DIR/floating_pill.py"

# 4. Install GNOME Shell Extension
echo "🔌 Installing GNOME Shell Extension to $EXT_DIR..."
mkdir -p "$EXT_DIR"
cp -rf "$SCRIPT_DIR/extension/"* "$EXT_DIR/"

# 5. Enable extension
if which gnome-extensions >/dev/null 2>&1; then
    echo "⚙️  Enabling extension in GNOME..."
    gnome-extensions enable "gnomescroll@kalpit.github.com" 2>/dev/null || true
fi

echo ""
echo "======================================================"
echo "  ✅ GNOMEscroll installed successfully!"
echo "======================================================"
echo ""
echo "How to use:"
echo " 1. Native GNOME Screenshot UI Integration:"
echo "    Press your normal screenshot shortcut (Super+Shift+S or PrtScn)."
echo "    Select the region you wish to scroll-capture."
echo "    Click the new GNOMEscroll (paged view) button in the screenshot bar!"
echo ""
echo " 2. Direct Command / Dedicated Custom Shortcut:"
echo "    Run: gnomescroll --area x,y,w,h"
echo "    Or simply run: gnomescroll"
echo ""
echo "Output:"
echo " • Images automatically save to ~/Pictures/Screenshots/"
echo " • Automatically copied to clipboard (image/png)"
echo " • Live notification with preview"
echo "======================================================"
