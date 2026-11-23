#!/usr/bin/env python3
import sys
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

        # Set crosshair cursor
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

    def on_draw(self, drawing_area, cr, screen_w, screen_h):
        cr.new_path()

        # 1. Dark overlay outside the selection
        if self.start_x is not None and self.curr_x is not None:
            x = min(self.start_x, self.curr_x)
            y = min(self.start_y, self.curr_y)
            w = abs(self.curr_x - self.start_x)
            h = abs(self.curr_y - self.start_y)

            # Draw 4 clean non-overlapping rectangles outside the selection:
            # The inside region [x, y, w, h] is NEVER drawn upon - 100% transparent and clear!
            cr.set_source_rgba(0.0, 0.0, 0.0, 0.35)
            if y > 0:
                cr.rectangle(0, 0, screen_w, y)
            if y + h < screen_h:
                cr.rectangle(0, y + h, screen_w, screen_h - (y + h))
            if x > 0:
                cr.rectangle(0, y, x, h)
            if x + w < screen_w:
                cr.rectangle(x + w, y, screen_w - (x + w), h)
            cr.fill()

            if w >= 2 and h >= 2:
                # Blue selection border
                cr.new_path()
                cr.set_source_rgba(0.21, 0.52, 0.89, 0.95) # #3584e4
                cr.set_line_width(2.0)
                cr.rectangle(x, y, w, h)
                cr.stroke()

            # Corner handles
            if w >= 24 and h >= 24:
                handle_len = min(14.0, w / 3.0, h / 3.0)
                cr.new_path()
                cr.set_source_rgba(1.0, 1.0, 1.0, 1.0)
                cr.set_line_width(3.0)

                # Top-left
                cr.move_to(x, y + handle_len); cr.line_to(x, y); cr.line_to(x + handle_len, y)
                # Top-right
                cr.move_to(x + w - handle_len, y); cr.line_to(x + w, y); cr.line_to(x + w, y + handle_len)
                # Bottom-left
                cr.move_to(x, y + h - handle_len); cr.line_to(x, y + h); cr.line_to(x + handle_len, y + h)
                # Bottom-right
                cr.move_to(x + w - handle_len, y + h); cr.line_to(x + w, y + h); cr.line_to(x + w, y + h - handle_len)
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

                # Isolated new path for badge capsule
                cr.new_path()
                cr.set_source_rgba(0.12, 0.12, 0.12, 0.92)
                radius = bh / 2.0
                cr.arc(bx + radius, by + radius, radius, 1.5707, 3.1415)
                cr.arc(bx + radius, by + bh - radius, radius, 3.1415, 4.7123)
                cr.arc(bx + bw - radius, by + bh - radius, radius, 4.7123, 6.2831)
                cr.arc(bx + bw - radius, by + radius, radius, 0.0, 1.5707)
                cr.close_path()
                cr.fill()

                cr.new_path()
                cr.set_source_rgba(1.0, 1.0, 1.0, 1.0)
                cr.move_to(bx + 10.0, by + 18.0)
                cr.show_text(badge_text)
        else:
            # No selection yet: draw uniform dark overlay
            cr.set_source_rgba(0.0, 0.0, 0.0, 0.35)
            cr.rectangle(0, 0, screen_w, screen_h)
            cr.fill()

        # 2. Top instruction pill
        banner_text = "📸 Drag to select capture region  •  Press Esc to cancel"
        cr.set_font_size(13.0)
        extents = cr.text_extents(banner_text)
        bw = extents.width + 32.0
        bh = 32.0
        bx = (screen_w - bw) / 2.0
        by = 28.0

        # Isolated new path for banner capsule
        cr.new_path()
        cr.set_source_rgba(0.12, 0.12, 0.12, 0.88)
        radius = bh / 2.0
        cr.arc(bx + radius, by + radius, radius, 1.5707, 3.1415)
        cr.arc(bx + radius, by + bh - radius, radius, 3.1415, 4.7123)
        cr.arc(bx + bw - radius, by + bh - radius, radius, 4.7123, 6.2831)
        cr.arc(bx + bw - radius, by + radius, radius, 0.0, 1.5707)
        cr.close_path()
        cr.fill()

        # Banner border
        cr.new_path()
        cr.set_source_rgba(1.0, 1.0, 1.0, 0.15)
        cr.set_line_width(1.0)
        cr.arc(bx + radius, by + radius, radius, 1.5707, 3.1415)
        cr.arc(bx + radius, by + bh - radius, radius, 3.1415, 4.7123)
        cr.arc(bx + bw - radius, by + bh - radius, radius, 4.7123, 6.2831)
        cr.arc(bx + bw - radius, by + radius, radius, 0.0, 1.5707)
        cr.close_path()
        cr.stroke()

        # Banner text
        cr.new_path()
        cr.set_source_rgba(1.0, 1.0, 1.0, 0.95)
        cr.move_to(bx + 16.0, by + 21.0)
        cr.show_text(banner_text)

def main():
    app = Gtk.Application(application_id='com.gnomescroll.selector')
    def on_activate(app):
        win = AreaSelector(app)
        win.present()
    app.connect('activate', on_activate)
    app.run(None)

if __name__ == '__main__':
    main()
