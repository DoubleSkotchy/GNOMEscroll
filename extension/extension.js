import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import GObject from 'gi://GObject';
import St from 'gi://St';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';

export default class GNOMEscrollExtension extends Extension {
    enable() {
        this._scrollButton = null;
        this._injectUI();
    }

    _injectUI() {
        const screenshotUI = Main.screenshotUI;
        if (!screenshotUI || !screenshotUI._shotCastContainer) {
            return;
        }

        // Create our Scrolling Screenshot button
        this._scrollButton = new St.Button({
            style_class: 'screenshot-ui-shot-cast-button gnomescroll-button',
            icon_name: 'view-paged-symbolic',
            can_focus: true,
            reactive: true,
        });

        // Tooltip
        this._tooltip = new St.Label({
            text: 'Scrolling Screenshot (GNOMEscroll)',
            style_class: 'screenshot-ui-tooltip',
            visible: false,
        });
        screenshotUI.add_child(this._tooltip);

        this._scrollButton.connect('notify::hover', () => {
            if (this._scrollButton.hover) {
                const [x, y] = this._scrollButton.get_transformed_position();
                const [w, h] = this._scrollButton.get_transformed_size();
                this._tooltip.set_position(
                    Math.round(x + w / 2 - this._tooltip.width / 2),
                    Math.round(y - this._tooltip.height - 8)
                );
                this._tooltip.show();
            } else {
                this._tooltip.hide();
            }
        });

        this._scrollButton.connect('clicked', () => {
            this._startScrollingScreenshot();
        });

        // Insert right into the shot/cast toggle container
        screenshotUI._shotCastContainer.add_child(this._scrollButton);
    }

    _startScrollingScreenshot() {
        const screenshotUI = Main.screenshotUI;
        if (!screenshotUI) return;

        // Obtain selected geometry [x, y, w, h] (true for rescaled coordinates)
        const geometry = screenshotUI._getSelectedGeometry(true);
        if (!geometry || geometry[2] <= 0 || geometry[3] <= 0) {
            Main.notify('GNOMEscroll', 'Please select a valid area first.');
            return;
        }

        const [x, y, w, h] = geometry.map(v => Math.round(v));

        // Close the screenshot overlay immediately so it doesn't appear in the capture
        screenshotUI.close(true);

        // Find gnomescroll binary
        let binaryPath = GLib.find_program_in_path('gnomescroll');
        if (!binaryPath) {
            const home = GLib.get_home_dir();
            const fallbackPath = `${home}/.local/bin/gnomescroll`;
            if (GLib.file_test(fallbackPath, GLib.FileTest.IS_EXECUTABLE)) {
                binaryPath = fallbackPath;
            } else {
                binaryPath = 'gnomescroll';
            }
        }

        try {
            const proc = new Gio.Subprocess({
                argv: [binaryPath, '--area', `${x},${y},${w},${h}`],
                flags: Gio.SubprocessFlags.NONE,
            });
            proc.init(null);
        } catch (err) {
            logError(err, 'Failed to launch GNOMEscroll');
            Main.notify('GNOMEscroll Error', `Failed to start scrolling capture: ${err.message}`);
        }
    }

    disable() {
        if (this._tooltip) {
            this._tooltip.destroy();
            this._tooltip = null;
        }
        if (this._scrollButton) {
            this._scrollButton.destroy();
            this._scrollButton = null;
        }
    }
}
