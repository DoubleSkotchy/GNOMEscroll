#!/usr/bin/env bash
set -e

# ==============================================================================
# 0. Privilege & Environment Management
# ==============================================================================
# If accidentally executed with sudo, automatically drop privileges to the real user
if [ "$EUID" -eq 0 ]; then
    if [ -n "$SUDO_USER" ] && [ "$SUDO_USER" != "root" ]; then
        echo "ℹ️  Re-executing installer as user '$SUDO_USER'..."
        exec sudo -u "$SUDO_USER" -H "$0" "$@"
    else
        echo "❌ GNOMEscroll is a desktop user application and cannot be installed as root."
        echo "Please run as your regular desktop user."
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
echo "        🚀 Installing GNOMEscroll"
echo "======================================================"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN_DIR="$HOME/.local/bin"
SHARE_DIR="$HOME/.local/share/gnomescroll"
EXT_UUID="gnomescroll@doubleskotchy.github.com"
EXT_DIR="$HOME/.local/share/gnome-shell/extensions/$EXT_UUID"

# ==============================================================================
# 1. Dependency Pre-flight Checks
# ==============================================================================
echo "🔍 Checking dependencies..."
missing_deps=()

if ! which cargo >/dev/null 2>&1; then
    missing_deps+=("cargo (Rust toolchain)")
fi

if ! which python3 >/dev/null 2>&1; then
    missing_deps+=("python3")
fi

if ! python3 -c "import gi; gi.require_version('Gtk', '3.0')" >/dev/null 2>&1; then
    missing_deps+=("python-gobject / gtk3")
fi

if [ ${#missing_deps[@]} -gt 0 ]; then
    echo "❌ Missing required dependencies:"
    for dep in "${missing_deps[@]}"; do
        echo "   • $dep"
    done
    echo ""
    echo "Please install them via your distribution package manager:"
    echo "  Arch/Manjaro:  sudo pacman -S cargo python python-gobject gtk3 wl-clipboard"
    echo "  Fedora:        sudo dnf install cargo python3 python3-gobject gtk3 wl-clipboard"
    echo "  Ubuntu/Debian: sudo apt install cargo python3 python3-gi gir1.2-gtk-3.0 wl-clipboard"
    exit 1
fi

if ! which wl-copy >/dev/null 2>&1; then
    echo "⚠️  Optional: 'wl-clipboard' not found. Highly recommended for automatic clipboard copying."
fi

# ==============================================================================
# 2. Stop Any Running Instance Before Overwriting
# ==============================================================================
pkill -f 'gnomescroll' 2>/dev/null || true
rm -f /tmp/gnomescroll* 2>/dev/null || true

# ==============================================================================
# 3. Build & Install Release Binary
# ==============================================================================
echo "📦 Building optimized release binary..."
cargo build --release --manifest-path "$SCRIPT_DIR/Cargo.toml"

echo "📁 Installing binary to $BIN_DIR/gnomescroll..."
install -Dm755 "$SCRIPT_DIR/target/release/gnomescroll" "$BIN_DIR/gnomescroll"

# ==============================================================================
# 4. Install Support Scripts & Assets
# ==============================================================================
echo "📁 Installing support files to $SHARE_DIR..."
install -Dm755 "$SCRIPT_DIR/scripts/select_area.py" "$SHARE_DIR/select_area.py"

# ==============================================================================
# 5. Install & Live-Reload GNOME Shell Extension
# ==============================================================================
echo "🔌 Installing GNOME Shell Extension to $EXT_DIR..."
mkdir -p "$EXT_DIR"
cp -rf "$SCRIPT_DIR/extension/"* "$EXT_DIR/"
chmod -R u+rwX "$EXT_DIR"

if which gnome-extensions >/dev/null 2>&1; then
    echo "⚙️  Activating extension in GNOME Shell..."

    # Ensure user extensions are allowed globally
    if which gsettings >/dev/null 2>&1; then
        gsettings set org.gnome.shell disable-user-extensions false 2>/dev/null || true
    fi

    # Pack and install bundle so live GNOME Shell registers new files
    TEMP_ZIP="/tmp/$EXT_UUID.shell-extension.zip"
    if gnome-extensions pack "$SCRIPT_DIR/extension" --force --out-dir /tmp/ 2>/dev/null; then
        gnome-extensions install --force "$TEMP_ZIP" 2>/dev/null || true
        rm -f "$TEMP_ZIP"
    fi

    # Live reload: cycle disable -> enable so GNOME Shell immediately applies changes
    if gnome-extensions list --enabled 2>/dev/null | grep -q "$EXT_UUID"; then
        gnome-extensions disable "$EXT_UUID" 2>/dev/null || true
        sleep 0.2
        gnome-extensions enable "$EXT_UUID" 2>/dev/null || true
    else
        gnome-extensions enable "$EXT_UUID" 2>/dev/null || true
    fi

    # Ensure extension is registered in enabled-extensions dconf list
    if which gsettings >/dev/null 2>&1 && which python3 >/dev/null 2>&1; then
        python3 -c "
import subprocess, ast
schema = 'org.gnome.shell'
key = 'enabled-extensions'
try:
    raw = subprocess.check_output(['gsettings', 'get', schema, key]).decode().strip().replace('@as ', '')
    arr = ast.literal_eval(raw)
    uuid = '$EXT_UUID'
    if uuid not in arr:
        arr.append(uuid)
        subprocess.run(['gsettings', 'set', schema, key, str(arr)], check=True)
except Exception:
    pass
" 2>/dev/null || true
    fi
fi

# ==============================================================================
# 6. Configure Super+Ctrl+S Keybinding in GNOME Settings
# ==============================================================================
if which gsettings >/dev/null 2>&1 && which python3 >/dev/null 2>&1; then
    echo "⚙️  Configuring Super+Ctrl+S shortcut in GNOME..."
    python3 -c "
import subprocess, ast, os

schema = 'org.gnome.settings-daemon.plugins.media-keys'
bin_path = os.path.expanduser('~/.local/bin/gnomescroll')
desired_binding = '<Primary><Super>s'
custom_base = '/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/'

try:
    raw = subprocess.check_output(['gsettings', 'get', schema, 'custom-keybindings']).decode().strip().replace('@as ', '')
    arr = ast.literal_eval(raw)
except Exception:
    arr = []

# Find if a keybinding already points to gnomescroll
target_path = None
for p in arr:
    try:
        cmd = subprocess.check_output(['gsettings', 'get', f'{schema}.custom-keybinding:{p}', 'command']).decode().strip().strip(\"'\\\"\")
        if 'gnomescroll' in cmd:
            target_path = p
            break
    except Exception:
        pass

# If no slot exists, create a dedicated path
if not target_path:
    target_path = f'{custom_base}gnomescroll/'
    if target_path not in arr:
        arr.append(target_path)
        subprocess.run(['gsettings', 'set', schema, 'custom-keybindings', str(arr)], check=True)

# Update shortcut settings immediately
subprocess.run(['gsettings', 'set', f'{schema}.custom-keybinding:{target_path}', 'name', 'GNOMEscroll'], check=True)
subprocess.run(['gsettings', 'set', f'{schema}.custom-keybinding:{target_path}', 'command', bin_path], check=True)
subprocess.run(['gsettings', 'set', f'{schema}.custom-keybinding:{target_path}', 'binding', desired_binding], check=True)
" 2>/dev/null || true
fi

# ==============================================================================
# 7. Ensure ~/.local/bin in User PATH
# ==============================================================================
for rc in "$HOME/.bashrc" "$HOME/.zshrc"; do
    if [ -f "$rc" ] && ! grep -q '\.local/bin' "$rc" 2>/dev/null; then
        echo 'export PATH="$HOME/.local/bin:$PATH"' >> "$rc"
    fi
done

echo ""
echo "======================================================"
echo "  ✅ GNOMEscroll updated & ready to use immediately!"
echo "======================================================"
echo ""
echo "How to use:"
echo " 1. Global Shortcut: Press Super + Ctrl + S anytime."
echo " 2. Native GNOME Screenshot UI: Press Super + Shift + S."
echo " 3. Terminal: Run 'gnomescroll'."
echo "======================================================"
