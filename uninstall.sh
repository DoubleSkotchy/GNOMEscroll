#!/usr/bin/env bash
set -e

# ==============================================================================
# 0. Privilege & Environment Management
# ==============================================================================
# If accidentally executed with sudo, automatically drop privileges to the real user
if [ "$EUID" -eq 0 ]; then
    if [ -n "$SUDO_USER" ] && [ "$SUDO_USER" != "root" ]; then
        echo "ℹ️  Re-executing uninstaller as user '$SUDO_USER'..."
        exec sudo -u "$SUDO_USER" -H "$0" "$@"
    else
        echo "❌ GNOMEscroll must be uninstalled as regular desktop user, not root."
        exit 1
    fi
fi

# Ensure D-Bus session address is available for gsettings / gnome-extensions
if [ -z "$DBUS_SESSION_BUS_ADDRESS" ]; then
    user_id=$(id -u)
    if [ -S "/run/user/$user_id/bus" ]; then
        export DBUS_SESSION_BUS_ADDRESS="unix:path=/run/user/$user_id/bus"
    fi
fi

echo "======================================================"
echo "        🗑️  Uninstalling GNOMEscroll"
echo "======================================================"

# ==============================================================================
# 1. Kill Any Active GNOMEscroll Processes
# ==============================================================================
echo "⏹️  Stopping active GNOMEscroll processes..."
pkill -f 'gnomescroll' 2>/dev/null || true

# ==============================================================================
# 2. Remove Binary & Application Support Files
# ==============================================================================
echo "📁 Removing binary from ~/.local/bin/gnomescroll..."
rm -f "$HOME/.local/bin/gnomescroll"

echo "📁 Removing support files from ~/.local/share/gnomescroll..."
rm -rf "$HOME/.local/share/gnomescroll"

# ==============================================================================
# 3. Clean Runtime Temp Files & Autostart Entries
# ==============================================================================
echo "🧹 Cleaning runtime temporary files & shortcuts..."
rm -f /tmp/gnomescroll*
rm -f "$HOME/.config/autostart/gnomescroll*" "$HOME/.local/share/applications/gnomescroll*"

# ==============================================================================
# 4. Disable & Remove GNOME Shell Extensions
# ==============================================================================
for ext in "gnomescroll@doubleskotchy.github.com" "gnomescroll@kalpit.github.com"; do
    if which gnome-extensions >/dev/null 2>&1; then
        echo "⚙️  Disabling extension $ext..."
        gnome-extensions disable "$ext" 2>/dev/null || true
    fi
    rm -rf "$HOME/.local/share/gnome-shell/extensions/$ext"
done

# Remove extension UUIDs from enabled-extensions
if which gsettings >/dev/null 2>&1 && which python3 >/dev/null 2>&1; then
    python3 -c "
import subprocess, ast
schema = 'org.gnome.shell'
key = 'enabled-extensions'
try:
    raw = subprocess.check_output(['gsettings', 'get', schema, key]).decode().strip().replace('@as ', '')
    arr = ast.literal_eval(raw)
    uuids = {'gnomescroll@doubleskotchy.github.com', 'gnomescroll@kalpit.github.com'}
    cleaned = [u for u in arr if u not in uuids]
    if len(cleaned) != len(arr):
        subprocess.run(['gsettings', 'set', schema, key, str(cleaned)], check=True)
except Exception:
    pass
" 2>/dev/null || true
fi

# ==============================================================================
# 5. Clean GNOME Keybinding Shortcuts
# ==============================================================================
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
        is_gnomescroll = ('gnomescroll' in p)
        if not is_gnomescroll:
            try:
                cmd = subprocess.check_output(['gsettings', 'get', f'{schema}.custom-keybinding:{p}', 'command']).decode()
                if 'gnomescroll' in cmd:
                    is_gnomescroll = True
            except Exception:
                pass
        if is_gnomescroll:
            subprocess.run(['gsettings', 'reset-recursively', f'{schema}.custom-keybinding:{p}'], stderr=subprocess.DEVNULL)
        else:
            cleaned.append(p)
    if len(cleaned) != len(arr):
        subprocess.run(['gsettings', 'set', schema, 'custom-keybindings', str(cleaned)], check=True)
except Exception:
    pass
" 2>/dev/null || true
fi

echo ""
echo "======================================================"
echo "  ✅ GNOMEscroll uninstalled completely!"
echo "======================================================"
