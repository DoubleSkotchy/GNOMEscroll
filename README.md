# GNOMEscroll 📜⚡

> **High-Performance Scrolling Screenshot Utility for GNOME (Wayland)**  
> Written in optimized **Rust** with multi-threaded SIMD stitching, native Wayland PipeWire capture, and seamless integration into GNOME's native Screenshot UI (`Super+Shift+S`).

---

## ✨ Features

- **🚀 Ultra-Optimized Rust Core:**
  - Zero-lag frame capture straight from Mutter via PipeWire (`org.gnome.Mutter.ScreenCast`).
  - Multi-threaded coarse-to-fine displacement correlation (SAD + 1D luminance projection).
  - Handles variable scroll speeds, trackpads, and mouse wheel ticks seamlessly.
  - Sub-millisecond slice alignment and stitch latency.

- **🎯 Seamless Native GNOME Integration:**
  - Works directly with your standard shortcut (`Super+Shift+S` or `PrtScn`).
  - Adds a dedicated "Scroll Screenshot" button right inside the native GNOME Screenshot overlay.
  - No permission prompts or portal friction on every capture.

- **💾 Zero-Friction Export:**
  - **Clipboard:** Automatically copies as `image/png` directly to your Wayland clipboard via `wl-copy`.
  - **Disk:** Saves to `~/Pictures/Screenshots/Screenshot from <timestamp>-scrolling.png` (matching GNOME's native screenshot conventions).
  - **Desktop Notification:** Instant notification preview with sound and notification action.

- **🎈 Minimal & Resource Light:**
  - Memory-efficient rolling canvas (discards duplicate frames in microsecond checks).
  - Small single self-contained binary (~4 MB).
  - Sleek floating pill widget that doesn't steal focus from your scrolling window.

---

## 📦 Requirements

- **Linux with GNOME Shell 45+** (GNOME 45, 46, 47, 48, 49, 50, 51+ on Wayland or X11)
- **Rust toolchain** (`cargo`, `rustc`)
- **GStreamer with PipeWire plugin** (installed by default on most modern distros):
  - Arch Linux: `pacman -S gst-plugin-pipewire pipewire wl-clipboard`
  - Fedora: `dnf install gstreamer1-plugin-pipewire pipewire wl-clipboard`
  - Ubuntu/Debian: `apt install gstreamer1.0-pipewire pipewire-bin wl-clipboard`

---

## 🛠️ Installation

Clone the repository and run the automated installer:

```bash
git clone https://github.com/DoubleSkotchy/GNOMEscroll.git
cd GNOMEscroll
./install.sh
```

The installer will:
1. Compile the optimized release binary `gnomescroll`.
2. Install it into `~/.local/bin/`.
3. Install the floating controller into `~/.local/share/gnomescroll/`.
4. Install and enable the companion GNOME Shell extension.

---

## 🎮 How to Use

### Method 1: Via GNOME Native Screenshot UI (Recommended)
1. Press your regular screenshot shortcut: **`Super + Shift + S`** (or `PrintScreen`).
2. Draw a rectangle over the window or area you want to capture.
3. In the screenshot toolbar at the bottom, click the **GNOMEscroll (Paged View) button**.
4. The screenshot tool closes and the floating indicator appears:
   > *📜 GNOMEscroll: Scroll down now... [✅ Finish] [✕]*
5. Scroll down through your webpage, document, or chat at your normal speed.
6. Click **Finish** (or press **Enter** in terminal).
7. Done! The full long screenshot is copied to your clipboard and saved in `~/Pictures/Screenshots/`.

---

### Method 2: Direct Command Line or Custom Shortcut
You can also run it directly or map it to a custom keyboard shortcut in GNOME Settings:

```bash
# Capture with custom coordinates
gnomescroll --area 200,150,900,700

# Specify custom output destination
gnomescroll --area 0,0,1920,1080 --out ~/Desktop/my-long-page.png

# Capture without writing to clipboard
gnomescroll --no-clipboard
```

---

## ⚙️ Architecture & Stitching Algorithm

GNOMEscroll uses a high-performance multi-stage displacement correlation pipeline:
1. **3-Strip Vertical 1D Luminance Profiles:**
   $$L_t(y) = \frac{1}{|X|} \sum_{x \in X} (0.299 R + 0.587 G + 0.114 B)$$
   Compares 1D vertical luminance profiles across Left, Center, and Right strips between frame $t$ and frame $t-1$ over shifts $\Delta y \in [2, \Delta y_{\max}]$ in both scroll directions (Down and Up). This reduces candidate searching to microsecond vector operations.
2. **2D Full-Overlap SAD Verification:**
   The top candidate shifts are verified with exact 2D pixel SAD and high-contrast ink verification gates, preventing discontinuous page jumps and text superimposition.
3. **Bidirectional Rolling Canvas:**
   Newly revealed rows are seamlessly appended to the bottom (when scrolling down) or prepended to the top (when scrolling up), ensuring smooth progressive stitching with zero tearing.

---

## 🗑️ Uninstallation

To remove GNOMEscroll cleanly:

```bash
./uninstall.sh
```

---

## 📜 License

MIT License. Free for personal and commercial use.

