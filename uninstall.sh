#!/usr/bin/env bash
set -e

echo "Removing GNOMEscroll..."
rm -f "$HOME/.local/bin/gnomescroll"
rm -rf "$HOME/.local/share/gnomescroll"
if which gnome-extensions >/dev/null 2>&1; then
    gnome-extensions disable "gnomescroll@kalpit.github.com" 2>/dev/null || true
fi
rm -rf "$HOME/.local/share/gnome-shell/extensions/gnomescroll@kalpit.github.com"
echo "GNOMEscroll uninstalled."
