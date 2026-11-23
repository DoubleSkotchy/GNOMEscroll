#!/usr/bin/env python3
import sys
import gi

gi.require_version('Gtk', '4.0')
from gi.repository import Gtk, Gdk, GLib

class AreaSelector(Gtk.ApplicationWindow):
    def __init__(self, app):
        super().__init__(application=app)
        self.set_title('Select Area')
        self.set_decorated(False)
        self.fullscreen()

        self.start_x = None
        self.start_y = None
        self.curr_x = None
        self.curr_y = None
        self.selected_rect = None

        # CSS for dark transparent overlay
        css_provider = Gtk.CssProvider()
        css = """
        window {
            background-color: rgba(0, 0, 0, 0.25);
        }
        """
        css_provider.load_from_data(css.encode('utf-8'))
        Gtk.StyleContext.add_provider_for_display(
            Gdk.Display.get_default(),
            css_provider,
            Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION
        )

        # Drawing area for rectangle and dimming
        self.drawing_area = Gtk.DrawingArea()
        self.drawing_area.set_draw_func(self.on_draw)
        self.set_child(self.drawing_area)

        # Gesture drag for selection
        self.drag = Gtk.GestureDrag()
        self.drag.connect('drag-begin', self.on_drag_begin)
        self.drag.connect('drag-update', self.on_drag_update)
        self.drag.connect('drag-end', self.on_drag_end)
        self.drawing_area.add_controller(self.drag)

        # Key controller for ESC
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
        if self.start_x is not None:
            x1 = min(self.start_x, self.curr_x)
            y1 = min(self.start_y, self.curr_y)
            w = abs(self.curr_x - self.start_x)
            h = abs(self.curr_y - self.start_y)

            if w >= 20 and h >= 20:
                self.selected_rect = (int(x1), int(y1), int(w), int(h))
                # Print selected rect to stdout
                print(f"{int(x1)},{int(y1)},{int(w)},{int(h)}")
                self.get_application().quit()
            else:
                self.start_x = None
                self.start_y = None
                self.drawing_area.queue_draw()

    def on_key_pressed(self, controller, keyval, keycode, state):
        if keyval == Gdk.KEY_Escape:
            sys.exit(1)
        return False

    def on_draw(self, drawing_area, cr, width, height):
        if self.start_x is not None and self.curr_x is not None:
            x = min(self.start_x, self.curr_x)
            y = min(self.start_y, self.curr_y)
            w = abs(self.curr_x - self.start_x)
            h = abs(self.curr_y - self.start_y)

            # Clear/dim background outside rect
            cr.set_source_rgba(0.0, 0.0, 0.0, 0.45)
            cr.rectangle(0, 0, width, height)
            cr.fill()

            # Cutout selection rectangle
            cr.set_operator(0) # CAIRO_OPERATOR_CLEAR
            cr.rectangle(x, y, w, h)
            cr.fill()

            # Border for selection
            cr.set_operator(2) # CAIRO_OPERATOR_OVER
            cr.set_source_rgba(0.21, 0.52, 0.89, 1.0) # #3584e4
            cr.set_line_width(2.0)
            cr.rectangle(x, y, w, h)
            cr.stroke()

            # Draw size label text
            cr.set_source_rgba(1.0, 1.0, 1.0, 1.0)
            cr.set_font_size(14.0)
            cr.move_to(x + 8, y + 22 if y + 22 < height else y - 8)
            cr.show_text(f"{int(w)} × {int(h)}")
        else:
            # Dim whole screen and show hint
            cr.set_source_rgba(0.0, 0.0, 0.0, 0.35)
            cr.rectangle(0, 0, width, height)
            cr.fill()

            cr.set_source_rgba(1.0, 1.0, 1.0, 0.9)
            cr.set_font_size(18.0)
            text = "Click and drag to select scrolling capture region (Press Esc to cancel)"
            cr.move_to(width / 2.0 - 260.0, height / 2.0)
            cr.show_text(text)

def main():
    app = Gtk.Application(application_id='com.gnomescroll.selector')
    def on_activate(app):
        win = AreaSelector(app)
        win.present()
    app.connect('activate', on_activate)
    app.run(None)

if __name__ == '__main__':
    main()
