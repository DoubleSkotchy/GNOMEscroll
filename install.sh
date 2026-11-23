#!/usr/bin/env bash
set -e

echo "======================================================"
echo "        🚀 Installing GNOMEscroll"
echo "======================================================"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN_DIR="$HOME/.local/bin"
SHARE_DIR="$HOME/.local/share/gnomescroll"
EXT_DIR="$HOME/.local/share/gnome-shell/extensions/gnomescroll@doubleskotchy.github.com"

# 1. Build release binary if not built yet
echo "📦 Building optimized release binary..."
cargo build --release

# 2. Install binary
echo "📁 Installing binary to $BIN_DIR/gnomescroll..."
mkdir -p "$BIN_DIR"
cp -f "$SCRIPT_DIR/target/release/gnomescroll" "$BIN_DIR/gnomescroll"
chmod +x "$BIN_DIR/gnomescroll"

# 3. Install scripts & assets
echo "📁 Installing support files to $SHARE_DIR..."
mkdir -p "$SHARE_DIR"
cp -f "$SCRIPT_DIR/scripts/select_area.py" "$SHARE_DIR/select_area.py"
chmod +x "$SHARE_DIR/select_area.py"

# 4. Install GNOME Shell Extension
echo "🔌 Installing GNOME Shell Extension to $EXT_DIR..."
mkdir -p "$EXT_DIR"
cp -rf "$SCRIPT_DIR/extension/"* "$EXT_DIR/"

# 5. Enable extension
if which gnome-extensions >/dev/null 2>&1; then
    echo "⚙️  Enabling extension in GNOME..."
    gnome-extensions enable "gnomescroll@doubleskotchy.github.com" 2>/dev/null || true
    if which gsettings >/dev/null 2>&1; then
        schema="org.gnome.shell"
        key="enabled-extensions"
        current=$(gsettings get "$schema" "$key" 2>/dev/null || echo "[]")
        uuid="gnomescroll@doubleskotchy.github.com"
        if ! echo "$current" | grep -q "$uuid"; then
            new_list=$(echo "$current" | sed "s/]/, '$uuid']/")
            if [ "$current" = "[]" ] || [ "$current" = "@as []" ]; then
                new_list="['$uuid']"
            fi
            gsettings set "$schema" "$key" "$new_list" 2>/dev/null || true
        fi
    fi
fi

# 6. Configure Super+Ctrl+S shortcut in GNOME Settings if not present
if which gsettings >/dev/null 2>&1; then
    schema="org.gnome.settings-daemon.plugins.media-keys"
    kb_list=$(gsettings get "$schema" custom-keybindings 2>/dev/null || echo "[]")
    has_gnomescroll=false
    for path in $(echo "$kb_list" | tr -d "[]'," | tr " " "\n"); do
        if [ -n "$path" ]; then
            cmd=$(gsettings get "$schema.custom-keybinding:$path" command 2>/dev/null || true)
            if echo "$cmd" | grep -q "gnomescroll"; then
                has_gnomescroll=true
                break
            fi
        fi
    done
    if [ "$has_gnomescroll" = false ]; then
        echo "⚙️  Registering Super+Ctrl+S shortcut in GNOME..."
        new_path="/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/gnomescroll/"
        if ! echo "$kb_list" | grep -q "$new_path"; then
            updated_kb=$(echo "$kb_list" | sed "s/]/, '$new_path']/")
            if [ "$kb_list" = "[]" ] || [ "$kb_list" = "@as []" ]; then
                updated_kb="['$new_path']"
            fi
            gsettings set "$schema" custom-keybindings "$updated_kb" 2>/dev/null || true
        fi
        gsettings set "$schema.custom-keybinding:$new_path" name "GNOMEscroll" 2>/dev/null || true
        gsettings set "$schema.custom-keybinding:$new_path" command "$HOME/.local/bin/gnomescroll" 2>/dev/null || true
        gsettings set "$schema.custom-keybinding:$new_path" binding "<Primary><Super>s" 2>/dev/null || true
    fi
fi

# 7. Ensure PATH in ~/.bashrc
if ! grep -q '.local/bin' "$HOME/.bashrc" 2>/dev/null; then
    echo 'export PATH="$HOME/.local/bin:$PATH"' >> "$HOME/.bashrc"
fi

echo ""
echo "======================================================"
echo "  ✅ GNOMEscroll installed successfully!"
echo "======================================================"
echo ""
echo "How to use:"
echo " 1. Dedicated Shortcut (Super + Control + S):"
echo "    Press Super+Ctrl+S anytime."
echo "    Drag a rectangle over the region to capture."
echo "    Scroll down, then click Finish!"
echo ""
echo " 2. Native GNOME Screenshot UI (Super + Shift + S):"
echo "    Active after your next login/logout."
echo ""
echo " 3. Terminal Command:"
echo "    gnomescroll"
echo "======================================================"
