#!/usr/bin/env python3
import os
import sys
import subprocess
import gi

gi.require_version('Gtk', '3.0')
from gi.repository import Gtk

try:
    gi.require_version('AyatanaAppIndicator3', '0.1')
    from gi.repository import AyatanaAppIndicator3 as AppIndicator
except Exception:
    gi.require_version('AppIndicator3', '0.1')
    from gi.repository import AppIndicator3 as AppIndicator

APPINDICATOR_ID = 'gnomescroll-tray'

def get_binary_path():
    home = os.path.expanduser('~')
    candidates = [
        os.path.join(home, '.local/bin/gnomescroll'),
        '/usr/local/bin/gnomescroll',
        '/usr/bin/gnomescroll',
    ]
    for c in candidates:
        if os.path.isfile(c) and os.access(c, os.X_OK):
            return c
    return 'gnomescroll'

def on_capture_scroll(_):
    # Launch GNOME native interactive screenshot or gnomescroll
    # Trigger portal screenshot interactive so user gets the native area selector
    cmd = [
        'gdbus', 'call', '--session',
        '--dest', 'org.freedesktop.portal.Desktop',
        '--object-path', '/org/freedesktop/portal/desktop',
        '--method', 'org.freedesktop.portal.Screenshot.Screenshot',
        '', "{'interactive': <true>}"
    ]
    subprocess.Popen(cmd)

def on_capture_fullscreen(_):
    bin_path = get_binary_path()
    subprocess.Popen([bin_path])

def on_open_folder(_):
    pictures_dir = os.path.expanduser('~/Pictures/Screenshots')
    os.makedirs(pictures_dir, exist_ok=True)
    subprocess.Popen(['xdg-open', pictures_dir])

def on_quit(_):
    Gtk.main_quit()

def build_menu():
    menu = Gtk.Menu()

    item_scroll = Gtk.MenuItem(label='📸 Open Screenshot Tool (Super+Shift+S)')
    item_scroll.connect('activate', on_capture_scroll)
    menu.append(item_scroll)

    item_full = Gtk.MenuItem(label='📜 Fullscreen Scrolling Screenshot')
    item_full.connect('activate', on_capture_fullscreen)
    menu.append(item_full)

    menu.append(Gtk.SeparatorMenuItem())

    item_folder = Gtk.MenuItem(label='📂 Open Screenshots Folder')
    item_folder.connect('activate', on_open_folder)
    menu.append(item_folder)

    menu.append(Gtk.SeparatorMenuItem())

    item_quit = Gtk.MenuItem(label='Quit')
    item_quit.connect('activate', on_quit)
    menu.append(item_quit)

    menu.show_all()
    return menu

def main():
    indicator = AppIndicator.Indicator.new(
        APPINDICATOR_ID,
        'camera-photo-symbolic',
        AppIndicator.IndicatorCategory.APPLICATION_STATUS
    )
    indicator.set_status(AppIndicator.IndicatorStatus.ACTIVE)
    indicator.set_title('GNOMEscroll')
    indicator.set_menu(build_menu())

    Gtk.main()

if __name__ == '__main__':
    main()

