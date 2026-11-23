#!/usr/bin/env python3
"""GNOMEscroll Area Selector — GTK4 fullscreen overlay with zero visual artifacts."""
import sys
import math
import cairo
import gi

gi.require_version('Gtk', '4.0')
from gi.repository import Gtk, Gdk, GLib


class AreaSelector(Gtk.ApplicationWindow):
    def __init__(self, app):
        super().__init__(application=app)
        self.set_title('GNOMEscroll Area Selector')
        self.set_decorated(False)
        self.fullscreen()

        self.start_x = None
        self.start_y = None
        self.curr_x = None
        self.curr_y = None
        self.confirmed = False

        self.set_cursor_from_name('crosshair')

        # Transparent window CSS
        css_provider = Gtk.CssProvider()
        css = """
        window, .background {
            background-color: transparent;
            background: transparent;
        }
        """
        css_provider.load_from_data(css.encode('utf-8'))
        Gtk.StyleContext.add_provider_for_display(
            Gdk.Display.get_default(),
            css_provider,
            Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION
        )

        # Drawing area
        self.drawing_area = Gtk.DrawingArea()
        self.drawing_area.set_draw_func(self.on_draw)
        self.drawing_area.set_cursor_from_name('crosshair')
        self.set_child(self.drawing_area)

        # Drag gesture
        self.drag = Gtk.GestureDrag()
        self.drag.connect('drag-begin', self.on_drag_begin)
        self.drag.connect('drag-update', self.on_drag_update)
        self.drag.connect('drag-end', self.on_drag_end)
        self.drawing_area.add_controller(self.drag)

        # Key controller
        key_controller = Gtk.EventControllerKey()
        key_controller.connect('key-pressed', self.on_key_pressed)
        self.add_controller(key_controller)

    def on_drag_begin(self, gesture, start_x, start_y):
        self.start_x = start_x
        self.start_y = start_y
        self.curr_x = start_x
        self.curr_y = start_y
        self.drawing_area.queue_draw()

    def on_drag_update(self, gesture, offset_x, offset_y):
        if self.start_x is not None:
            self.curr_x = self.start_x + offset_x
            self.curr_y = self.start_y + offset_y
            self.drawing_area.queue_draw()

    def on_drag_end(self, gesture, offset_x, offset_y):
        if self.start_x is not None and self.curr_x is not None:
            w = abs(self.curr_x - self.start_x)
            h = abs(self.curr_y - self.start_y)
            if w >= 20 and h >= 20:
                x1 = min(self.start_x, self.curr_x)
                y1 = min(self.start_y, self.curr_y)
                print(f"{int(x1)},{int(y1)},{int(w)},{int(h)}")
                self.confirmed = True
                self.get_application().quit()
            else:
                self.start_x = None
                self.start_y = None
                self.curr_x = None
                self.curr_y = None
                self.drawing_area.queue_draw()

    def on_key_pressed(self, controller, keyval, keycode, state):
        if keyval == Gdk.KEY_Escape:
            sys.exit(1)
        elif keyval in (Gdk.KEY_Return, Gdk.KEY_KP_Enter, Gdk.KEY_space):
            if self.start_x is not None and self.curr_x is not None:
                w = abs(self.curr_x - self.start_x)
                h = abs(self.curr_y - self.start_y)
                if w >= 20 and h >= 20:
                    x1 = min(self.start_x, self.curr_x)
                    y1 = min(self.start_y, self.curr_y)
                    print(f"{int(x1)},{int(y1)},{int(w)},{int(h)}")
                    self.confirmed = True
                    self.get_application().quit()
        return False

    @staticmethod
    def _draw_rounded_rect(cr, x, y, w, h, r):
        """Draw a rounded rectangle as a single self-contained sub-path."""
        cr.new_sub_path()
        cr.arc(x + w - r, y + r, r, -math.pi / 2, 0)
        cr.arc(x + w - r, y + h - r, r, 0, math.pi / 2)
        cr.arc(x + r, y + h - r, r, math.pi / 2, math.pi)
        cr.arc(x + r, y + r, r, math.pi, 3 * math.pi / 2)
        cr.close_path()

    def on_draw(self, drawing_area, cr, screen_w, screen_h):
        # ── 1. Dark overlay ──────────────────────────────────────────
        # Always start from a completely blank path state
        cr.save()
        cr.new_path()

        if self.start_x is not None and self.curr_x is not None:
            x = min(self.start_x, self.curr_x)
            y = min(self.start_y, self.curr_y)
            w = abs(self.curr_x - self.start_x)
            h = abs(self.curr_y - self.start_y)

            # 4 non-overlapping rectangles around selection — each is a separate fill
            cr.set_source_rgba(0.0, 0.0, 0.0, 0.35)

            # Top band
            if y > 0:
                cr.new_path()
                cr.rectangle(0, 0, screen_w, y)
                cr.fill()
            # Bottom band
            if y + h < screen_h:
                cr.new_path()
                cr.rectangle(0, y + h, screen_w, screen_h - (y + h))
                cr.fill()
            # Left band
            if x > 0:
                cr.new_path()
                cr.rectangle(0, y, x, h)
                cr.fill()
            # Right band
            if x + w < screen_w:
                cr.new_path()
                cr.rectangle(x + w, y, screen_w - (x + w), h)
                cr.fill()

            # ── 2. Blue selection border ─────────────────────────────
            if w >= 2 and h >= 2:
                cr.new_path()
                cr.set_source_rgba(0.21, 0.52, 0.89, 0.95)
                cr.set_line_width(2.0)
                cr.rectangle(x, y, w, h)
                cr.stroke()

            # ── 3. White corner handles ──────────────────────────────
            if w >= 24 and h >= 24:
                handle_len = min(14.0, w / 3.0, h / 3.0)
                cr.new_path()
                cr.set_source_rgba(1.0, 1.0, 1.0, 1.0)
                cr.set_line_width(3.0)
                cr.set_line_cap(cairo.LINE_CAP_ROUND)

                for (cx, cy, dx1, dy1, dx2, dy2) in [
                    (x, y, handle_len, 0, 0, handle_len),              # top-left
                    (x + w, y, -handle_len, 0, 0, handle_len),         # top-right
                    (x, y + h, handle_len, 0, 0, -handle_len),         # bottom-left
                    (x + w, y + h, -handle_len, 0, 0, -handle_len),    # bottom-right
                ]:
                    cr.move_to(cx + dx1, cy + dy1)
                    cr.line_to(cx, cy)
                    cr.line_to(cx + dx2, cy + dy2)

                cr.stroke()

            # ── 4. Dimension badge ───────────────────────────────────
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
                cr.new_path()
        else:
            # No selection yet — uniform dark overlay
            cr.set_source_rgba(0.0, 0.0, 0.0, 0.35)
            cr.rectangle(0, 0, screen_w, screen_h)
            cr.fill()

        cr.restore()

        # ── 5. Top instruction banner (always drawn, isolated context) ──
        cr.save()
        cr.new_path()

        banner_text = "Drag to select capture region  —  Esc to cancel"
        cr.set_font_size(13.0)
        extents = cr.text_extents(banner_text)
        bw = extents.width + 32.0
        bh = 32.0
        bx = (screen_w - bw) / 2.0
        by = 28.0
        radius = bh / 2.0

        # Banner background
        cr.set_source_rgba(0.12, 0.12, 0.12, 0.88)
        self._draw_rounded_rect(cr, bx, by, bw, bh, radius)
        cr.fill()

        # Banner border
        cr.new_path()
        cr.set_source_rgba(1.0, 1.0, 1.0, 0.15)
        cr.set_line_width(1.0)
        self._draw_rounded_rect(cr, bx, by, bw, bh, radius)
        cr.stroke()

        # Banner text
        cr.new_path()
        cr.set_source_rgba(1.0, 1.0, 1.0, 0.95)
        cr.move_to(bx + 16.0, by + 21.0)
        cr.show_text(banner_text)
        cr.new_path()

        cr.restore()


def main():
    app = Gtk.Application(application_id='com.gnomescroll.selector')
    def on_activate(app):
        win = AreaSelector(app)
        win.present()
    app.connect('activate', on_activate)
    app.run(None)

if __name__ == '__main__':
    main()
