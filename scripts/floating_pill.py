#!/usr/bin/env python3
import sys
import os
import signal
import gi

gi.require_version('Gtk', '4.0')
from gi.repository import Gtk, GLib, Gdk

def main():
    if len(sys.argv) < 2:
        print("Usage: floating_pill.py <fifo_path>")
        sys.exit(1)

    fifo_path = sys.argv[1]

    app = Gtk.Application(application_id='com.gnomescroll.pill')

    def send_action_and_exit(action):
        try:
            with open(fifo_path, 'w') as f:
                f.write(action + '\n')
        except Exception:
            pass
        app.quit()

    def on_activate(app):
        win = Gtk.ApplicationWindow(application=app)
        win.set_title('GNOMEscroll Controller')
        win.set_decorated(False)
        win.set_focusable(False)

        # CSS for modern Adwaita frosted-glass pill styling
        css_provider = Gtk.CssProvider()
        css = """
        window {
            background-color: transparent;
        }
        .osd-pill {
            background: rgba(30, 30, 30, 0.90);
            border: 1px solid rgba(255, 255, 255, 0.18);
            border-radius: 9999px;
            padding: 8px 18px;
            box-shadow: 0 8px 28px rgba(0, 0, 0, 0.45);
        }
        .rec-dot {
            color: #e01b24;
            font-size: 16px;
            margin-right: 6px;
        }
        .pill-title {
            color: #ffffff;
            font-weight: 700;
            font-size: 13px;
            margin-right: 4px;
        }
        .pill-subtitle {
            color: rgba(255, 255, 255, 0.65);
            font-size: 12px;
            margin-right: 14px;
        }
        .btn-finish {
            background-color: #3584e4;
            color: #ffffff;
            font-weight: 700;
            font-size: 12px;
            border-radius: 9999px;
            padding: 6px 16px;
            border: none;
            transition: all 150ms ease-in-out;
        }
        .btn-finish:hover {
            background-color: #1c71d8;
        }
        .btn-finish:active {
            background-color: #15539e;
        }
        .btn-cancel {
            background-color: rgba(255, 255, 255, 0.12);
            color: #e0e0e0;
            font-weight: 600;
            font-size: 12px;
            border-radius: 9999px;
            padding: 6px 12px;
            border: none;
            margin-left: 6px;
            transition: all 150ms ease-in-out;
        }
        .btn-cancel:hover {
            background-color: #e01b24;
            color: #ffffff;
        }
        """
        css_provider.load_from_data(css.encode('utf-8'))
        Gtk.StyleContext.add_provider_for_display(
            Gdk.Display.get_default(),
            css_provider,
            Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION
        )

        # Wrap in Gtk.WindowHandle so the pill can be dragged anywhere on screen
        handle = Gtk.WindowHandle()
        box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=6)
        box.add_css_class('osd-pill')

        # Red recording pulse indicator
        rec_dot = Gtk.Label(label='●')
        rec_dot.add_css_class('rec-dot')
        box.append(rec_dot)

        # Label: Title & hint
        title = Gtk.Label(label='Recording')
        title.add_css_class('pill-title')
        box.append(title)

        subtitle = Gtk.Label(label='•  Scroll down freely')
        subtitle.add_css_class('pill-subtitle')
        box.append(subtitle)

        # Finish button
        btn_finish = Gtk.Button(label='✅ Finish (Enter)')
        btn_finish.add_css_class('btn-finish')
        btn_finish.set_focusable(False)
        btn_finish.connect('clicked', lambda _: send_action_and_exit('DONE'))
        box.append(btn_finish)

        # Cancel button
        btn_cancel = Gtk.Button(label='✕ Esc')
        btn_cancel.add_css_class('btn-cancel')
        btn_cancel.set_focusable(False)
        btn_cancel.connect('clicked', lambda _: send_action_and_exit('CANCEL'))
        box.append(btn_cancel)

        handle.set_child(box)
        win.set_child(handle)
        win.present()

    app.connect('activate', on_activate)
    app.run(None)

if __name__ == '__main__':
    main()
