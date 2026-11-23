#!/usr/bin/env python3
"""GNOMEscroll Area Selector — 100% transparent screen overlay using Mutter desktop snapshot."""
import os
import sys
import math
import subprocess
import warnings
warnings.filterwarnings('ignore')

import cairo
import gi

gi.require_version('Gtk', '3.0')
from gi.repository import Gtk, Gdk, GLib, Gio

SNAPSHOT_PATH = "/tmp/gnomescroll_bg.png"


def capture_desktop_snapshot():
    """Capture a 1-frame snapshot of the live desktop via Mutter ScreenCast."""
    try:
        bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
        proxy = Gio.DBusProxy.new_sync(
            bus, Gio.DBusProxyFlags.NONE, None,
            "org.gnome.Mutter.ScreenCast",
            "/org/gnome/Mutter/ScreenCast",
            "org.gnome.Mutter.ScreenCast",
            None
        )
        session_path = proxy.call_sync("CreateSession", GLib.Variant("(a{sv})", ({},)), Gio.DBusCallFlags.NONE, -1, None).unpack()[0]
        session_proxy = Gio.DBusProxy.new_sync(
            bus, Gio.DBusProxyFlags.NONE, None,
            "org.gnome.Mutter.ScreenCast",
            session_path,
            "org.gnome.Mutter.ScreenCast.Session",
            None
        )
        # Determine monitor dimensions
        screen = Gdk.Screen.get_default()
        sw = screen.get_width() if screen else 1920
        sh = screen.get_height() if screen else 1080

        stream_path = session_proxy.call_sync("RecordArea", GLib.Variant("(iiiia{sv})", (0, 0, sw, sh, {})), Gio.DBusCallFlags.NONE, -1, None).unpack()[0]
        stream_proxy = Gio.DBusProxy.new_sync(
            bus, Gio.DBusProxyFlags.NONE, None,
            "org.gnome.Mutter.ScreenCast",
            stream_path,
            "org.gnome.Mutter.ScreenCast.Stream",
            None
        )

        node_id = None
        def on_signal(proxy, sender_name, signal_name, parameters):
            nonlocal node_id
            if signal_name == "PipeWireStreamAdded":
                node_id = parameters.unpack()[0]

        stream_proxy.connect("g-signal", on_signal)
        session_proxy.call_sync("Start", None, Gio.DBusCallFlags.NONE, -1, None)

        loop = GLib.MainLoop()
        def check_node():
            if node_id is not None:
                loop.quit()
                return False
            return True

        GLib.timeout_add(5, check_node)
        loop.run()

        gst_cmd = [
            "gst-launch-1.0", "-q",
            "pipewiresrc", f"path={node_id}", "num-buffers=1", "keepalive-time=100",
            "!", "videoconvert",
            "!", "pngenc",
            "!", "filesink", f"location={SNAPSHOT_PATH}"
        ]
        subprocess.run(gst_cmd, check=True, timeout=2.0)
        session_proxy.call_sync("Stop", None, Gio.DBusCallFlags.NONE, -1, None)
        return os.path.exists(SNAPSHOT_PATH)
    except Exception:
        return False


class AreaSelector(Gtk.Window):
    def __init__(self):
        super().__init__(type=Gtk.WindowType.TOPLEVEL)
        self.set_title('GNOMEscroll Area Selector')
        self.set_decorated(False)

        # Load live desktop snapshot as the backdrop
        self.bg_surface = None
        if os.path.exists(SNAPSHOT_PATH):
            try:
                self.bg_surface = cairo.ImageSurface.create_from_png(SNAPSHOT_PATH)
            except Exception:
                self.bg_surface = None

        self.start_x = None
        self.start_y = None
        self.curr_x = None
        self.curr_y = None
        self.confirmed = False

        self.add_events(
            Gdk.EventMask.BUTTON_PRESS_MASK
            | Gdk.EventMask.BUTTON_RELEASE_MASK
            | Gdk.EventMask.POINTER_MOTION_MASK
            | Gdk.EventMask.KEY_PRESS_MASK
        )

        self.connect('draw', self.on_draw)
        self.connect('button-press-event', self.on_button_press)
        self.connect('motion-notify-event', self.on_motion_notify)
        self.connect('button-release-event', self.on_button_release)
        self.connect('key-press-event', self.on_key_press)
        self.connect('realize', self.on_realize)

        self.fullscreen()

    def on_realize(self, widget):
        window = self.get_window()
        if window:
            display = Gdk.Display.get_default()
            cursor = Gdk.Cursor.new_from_name(display, 'crosshair')
            window.set_cursor(cursor)

    def on_button_press(self, widget, event):
        if event.button == 1:
            self.start_x = event.x
            self.start_y = event.y
            self.curr_x = event.x
            self.curr_y = event.y
            self.queue_draw()
        return True

    def on_motion_notify(self, widget, event):
        if self.start_x is not None:
            self.curr_x = event.x
            self.curr_y = event.y
            self.queue_draw()
        return True

    def on_button_release(self, widget, event):
        if event.button == 1:
            self.confirm_and_exit()
        return True

    def confirm_and_exit(self):
        if self.start_x is not None and self.curr_x is not None:
            w = abs(self.curr_x - self.start_x)
            h = abs(self.curr_y - self.start_y)
            if w >= 20 and h >= 20:
                x1 = min(self.start_x, self.curr_x)
                y1 = min(self.start_y, self.curr_y)
                self.hide()
                while Gtk.events_pending():
                    Gtk.main_iteration()
                print(f"{int(x1)},{int(y1)},{int(w)},{int(h)}")
                sys.stdout.flush()
                self.confirmed = True
                Gtk.main_quit()
                return
        self.start_x = None
        self.start_y = None
        self.curr_x = None
        self.curr_y = None
        self.queue_draw()

    def on_key_press(self, widget, event):
        if event.keyval == Gdk.KEY_Escape:
            self.hide()
            Gtk.main_quit()
            sys.exit(1)
        elif event.keyval in (Gdk.KEY_Return, Gdk.KEY_KP_Enter, Gdk.KEY_space):
            self.confirm_and_exit()
        return True

    @staticmethod
    def _draw_rounded_rect(cr, x, y, w, h, r):
        cr.new_sub_path()
        cr.arc(x + w - r, y + r, r, -math.pi / 2, 0)
        cr.arc(x + w - r, y + h - r, r, 0, math.pi / 2)
        cr.arc(x + r, y + h - r, r, math.pi / 2, math.pi)
        cr.arc(x + r, y + r, r, math.pi, 3 * math.pi / 2)
        cr.close_path()

    def on_draw(self, widget, cr):
        alloc = widget.get_allocation()
        screen_w = alloc.width
        screen_h = alloc.height

        # 1. Paint the live desktop snapshot as the base!
        if self.bg_surface:
            cr.set_source_surface(self.bg_surface, 0, 0)
            cr.paint()
        else:
            cr.set_source_rgba(0.1, 0.1, 0.1, 1.0)
            cr.paint()

        # 2. Draw dim overlay outside selection
        if self.start_x is not None and self.curr_x is not None:
            x = min(self.start_x, self.curr_x)
            y = min(self.start_y, self.curr_y)
            w = abs(self.curr_x - self.start_x)
            h = abs(self.curr_y - self.start_y)

            cr.save()
            # 40% darkness outside the selection rectangle
            cr.set_source_rgba(0.0, 0.0, 0.0, 0.40)

            # Top band
            if y > 0:
                cr.rectangle(0, 0, screen_w, y)
                cr.fill()
            # Bottom band
            if y + h < screen_h:
                cr.rectangle(0, y + h, screen_w, screen_h - (y + h))
                cr.fill()
            # Left band
            if x > 0:
                cr.rectangle(0, y, x, h)
                cr.fill()
            # Right band
            if x + w < screen_w:
                cr.rectangle(x + w, y, screen_w - (x + w), h)
                cr.fill()

            # The region inside [x, y, w, h] is 100% UNTOUCHED — completely crisp original desktop!

            # Blue selection border
            if w >= 2 and h >= 2:
                cr.new_path()
                cr.set_source_rgba(0.21, 0.52, 0.89, 0.95)
                cr.set_line_width(2.0)
                cr.rectangle(x, y, w, h)
                cr.stroke()

            # White corner handles
            if w >= 24 and h >= 24:
                handle_len = min(14.0, w / 3.0, h / 3.0)
                cr.new_path()
                cr.set_source_rgba(1.0, 1.0, 1.0, 1.0)
                cr.set_line_width(3.0)
                cr.set_line_cap(cairo.LINE_CAP_ROUND)

                for (cx, cy, dx1, dy1, dx2, dy2) in [
                    (x, y, handle_len, 0, 0, handle_len),
                    (x + w, y, -handle_len, 0, 0, handle_len),
                    (x, y + h, handle_len, 0, 0, -handle_len),
                    (x + w, y + h, -handle_len, 0, 0, -handle_len),
                ]:
                    cr.move_to(cx + dx1, cy + dy1)
                    cr.line_to(cx, cy)
                    cr.line_to(cx + dx2, cy + dy2)
                cr.stroke()

            # Measurement pill badge
            if w >= 30 and h >= 30:
                badge_text = f"{int(w)} × {int(h)} px"
                cr.set_font_size(13.0)
                extents = cr.text_extents(badge_text)
                bw = extents.width + 20.0
                bh = 26.0

                bx = x + (w - bw) / 2.0
                by = y + h + 10.0
                if by + bh > screen_h - 10:
                    by = y - bh - 10.0
                bx = max(10.0, min(screen_w - bw - 10.0, bx))
                by = max(10.0, min(screen_h - bh - 10.0, by))

                radius = bh / 2.0
                cr.new_path()
                cr.set_source_rgba(0.12, 0.12, 0.12, 0.92)
                self._draw_rounded_rect(cr, bx, by, bw, bh, radius)
                cr.fill()

                cr.new_path()
                cr.set_source_rgba(1.0, 1.0, 1.0, 1.0)
                cr.move_to(bx + 10.0, by + 18.0)
                cr.show_text(badge_text)

            cr.restore()
        else:
            # No selection yet: subtle 15% dimming over desktop so user knows overlay is active
            cr.set_source_rgba(0.0, 0.0, 0.0, 0.15)
            cr.rectangle(0, 0, screen_w, screen_h)
            cr.fill()

        # Top instruction banner
        cr.save()
        banner_text = "Drag to select capture area  —  Esc to cancel"
        cr.set_font_size(13.0)
        extents = cr.text_extents(banner_text)
        bw = extents.width + 32.0
        bh = 32.0
        bx = (screen_w - bw) / 2.0
        by = 28.0
        radius = bh / 2.0

        cr.new_path()
        cr.set_source_rgba(0.12, 0.12, 0.12, 0.88)
        self._draw_rounded_rect(cr, bx, by, bw, bh, radius)
        cr.fill()

        cr.new_path()
        cr.set_source_rgba(1.0, 1.0, 1.0, 0.15)
        cr.set_line_width(1.0)
        self._draw_rounded_rect(cr, bx, by, bw, bh, radius)
        cr.stroke()

        cr.new_path()
        cr.set_source_rgba(1.0, 1.0, 1.0, 0.95)
        cr.move_to(bx + 16.0, by + 21.0)
        cr.show_text(banner_text)
        cr.restore()

        return False


def main():
    # 1. Grab snapshot of live desktop first
    capture_desktop_snapshot()

    # 2. Open area selector
    win = AreaSelector()
    win.show_all()
    Gtk.main()


if __name__ == '__main__':
    main()
