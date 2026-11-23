#!/usr/bin/env bash
set -e

if [ "$EUID" -eq 0 ]; then
    echo "Please do NOT run uninstall.sh with sudo!"
    echo "GNOME extensions and user configurations must be uninstalled as your regular user."
    exit 1
fi

echo "======================================================"
echo "        🗑️  Uninstalling GNOMEscroll"
echo "======================================================"

# 1. Kill any active gnomescroll processes
pkill -f 'gnomescroll' 2>/dev/null || true

# 2. Remove installed binary
echo "Removing binary from ~/.local/bin/gnomescroll..."
rm -f "$HOME/.local/bin/gnomescroll"

# 3. Remove application support files
echo "Removing support files from ~/.local/share/gnomescroll..."
rm -rf "$HOME/.local/share/gnomescroll"

# 4. Remove runtime temp files & fifos
rm -f /tmp/gnomescroll*

# 5. Disable and remove GNOME Shell Extension (both current and legacy UUIDs)
for ext in "gnomescroll@doubleskotchy.github.com"; do
    if which gnome-extensions >/dev/null 2>&1; then
        echo "Disabling extension $ext..."
        gnome-extensions disable "$ext" 2>/dev/null || true
        gnome-extensions uninstall "$ext" 2>/dev/null || true
    fi
    rm -rf "$HOME/.local/share/gnome-shell/extensions/$ext"
done

# 6. Remove custom shortcut if assigned in GNOME settings
if which gsettings >/dev/null 2>&1 && which python3 >/dev/null 2>&1; then
    echo "⚙️  Removing GNOME shortcut..."
    python3 -c "
import subprocess, ast
schema = 'org.gnome.settings-daemon.plugins.media-keys'
try:
    raw = subprocess.check_output(['gsettings', 'get', schema, 'custom-keybindings']).decode().strip().replace('@as ', '')
    arr = ast.literal_eval(raw)
    cleaned = []
    for p in arr:
        try:
            cmd = subprocess.check_output(['gsettings', 'get', f'{schema}.custom-keybinding:{p}', 'command']).decode()
            if 'gnomescroll' in cmd:
                subprocess.run(['gsettings', 'reset-recursively', f'{schema}.custom-keybinding:{p}'], stderr=subprocess.DEVNULL)
                continue
        except Exception:
            pass
        cleaned.append(p)
    if len(cleaned) != len(arr):
        subprocess.run(['gsettings', 'set', schema, 'custom-keybindings', str(cleaned)])
except Exception:
    pass
" 2>/dev/null || true
fi

echo ""
echo "======================================================"
echo "  ✅ GNOMEscroll uninstalled completely!"
echo "======================================================"
