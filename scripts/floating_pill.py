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

        # Style provider for sleek dark rounded pill look
        css_provider = Gtk.CssProvider()
        css = """
        window {
            background-color: transparent;
        }
        .pill-box {
            background-color: rgba(30, 30, 30, 0.92);
            border: 1px solid rgba(255, 255, 255, 0.2);
            border-radius: 28px;
            padding: 8px 16px;
            box-shadow: 0 4px 16px rgba(0, 0, 0, 0.5);
        }
        .pill-label {
            color: #ffffff;
            font-weight: bold;
            font-size: 13px;
            margin-right: 12px;
        }
        .pill-btn-done {
            background-color: #3584e4;
            color: white;
            font-weight: bold;
            border-radius: 18px;
            padding: 6px 14px;
            border: none;
        }
        .pill-btn-done:hover {
            background-color: #1c71d8;
        }
        .pill-btn-cancel {
            background-color: rgba(255, 255, 255, 0.1);
            color: #e0e0e0;
            border-radius: 18px;
            padding: 6px 12px;
            border: none;
            margin-left: 6px;
        }
        .pill-btn-cancel:hover {
            background-color: #e01b24;
            color: white;
        }
        """
        css_provider.load_from_data(css.encode('utf-8'))
        Gtk.StyleContext.add_provider_for_display(
            Gdk.Display.get_default(),
            css_provider,
            Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION
        )

        box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=6)
        box.add_css_class('pill-box')

        label = Gtk.Label(label='📜 GNOMEscroll: Scroll down now...')
        label.add_css_class('pill-label')
        box.append(label)

        btn_done = Gtk.Button(label='✅ Finish')
        btn_done.add_css_class('pill-btn-done')
        btn_done.set_focusable(False)
        btn_done.connect('clicked', lambda _: send_action_and_exit('DONE'))
        box.append(btn_done)

        btn_cancel = Gtk.Button(label='✕')
        btn_cancel.add_css_class('pill-btn-cancel')
        btn_cancel.set_focusable(False)
        btn_cancel.connect('clicked', lambda _: send_action_and_exit('CANCEL'))
        box.append(btn_cancel)

        win.set_child(box)
        win.present()

    app.connect('activate', on_activate)
    app.run(None)

if __name__ == '__main__':
    main()
