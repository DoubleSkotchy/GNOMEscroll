import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import GObject from 'gi://GObject';
import St from 'gi://St';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';
import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';

export default class GNOMEscrollExtension extends Extension {
    enable() {
        this._scrollButton = null;
        this._indicator = null;
        this._tooltip = null;

        this._injectTopBarIndicator();
        this._injectScreenshotUI();
    }

    _injectTopBarIndicator() {
        try {
            this._indicator = new PanelMenu.Button(0.0, 'GNOMEscroll', false);
            const icon = new St.Icon({
                icon_name: 'camera-photo-symbolic',
                style_class: 'system-status-icon',
            });
            this._indicator.add_child(icon);

            const itemCapture = new PopupMenu.PopupMenuItem('📸 Open Screenshot Tool (Super+Shift+S)');
            itemCapture.connect('activate', () => {
                if (Main.screenshotUI) {
                    Main.screenshotUI.open();
                }
            });
            this._indicator.menu.addMenuItem(itemCapture);

            const itemFolder = new PopupMenu.PopupMenuItem('📂 Open Screenshots Folder');
            itemFolder.connect('activate', () => {
                const home = GLib.get_home_dir();
                const path = `${home}/Pictures/Screenshots`;
                Gio.AppInfo.launch_default_for_uri(`file://${path}`, null);
            });
            this._indicator.menu.addMenuItem(itemFolder);

            Main.panel.addToStatusArea('gnomescroll-indicator', this._indicator);
        } catch (e) {
            logError(e, 'Failed to inject GNOMEscroll top bar indicator');
        }
    }

    _injectScreenshotUI() {
        const screenshotUI = Main.screenshotUI;
        if (!screenshotUI || !screenshotUI._shotCastContainer) {
            return;
        }

        // Create Scrolling Screenshot button inside screenshot overlay
        this._scrollButton = new St.Button({
            style_class: 'screenshot-ui-shot-cast-button gnomescroll-button',
            icon_name: 'view-paged-symbolic',
            can_focus: true,
            reactive: true,
        });

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

        screenshotUI._shotCastContainer.add_child(this._scrollButton);
    }

    _startScrollingScreenshot() {
        const screenshotUI = Main.screenshotUI;
        if (!screenshotUI) return;

        const geometry = screenshotUI._getSelectedGeometry(true);
        if (!geometry || geometry[2] <= 0 || geometry[3] <= 0) {
            Main.notify('GNOMEscroll', 'Please select a valid area first.');
            return;
        }

        const [x, y, w, h] = geometry.map(v => Math.round(v));
        screenshotUI.close(true);

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
        if (this._indicator) {
            this._indicator.destroy();
            this._indicator = null;
        }
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
