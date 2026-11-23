<p align="center">
  <img src="assets/logo.png" width="96" alt="kshot logo" />
</p>

# kshot

Fast, native Wayland screenshot, cropping, and annotation suite written in Rust.

## Overview

`kshot` is a native Wayland screenshot, window capture, and annotation suite designed as a lightweight alternative to Flameshot on modern Wayland compositors (such as Umbriel, Sway, Hyprland, and River). It captures directly via Wayland screencopy protocols, handles fractional scaling across mixed-DPI monitors, offers vector annotation tools in the Gruvbox Dark aesthetic, provides interactive window and scrolling capture, renders drop-shadow beautification without external dependencies, and integrates a native Wayland Layer Shell HUD.

## Key Features

- **Native Wayland Screencopy**: Direct frame capture via `wlr-screencopy-unstable-v1` and `ext-image-copy-capture-v1`, with fallback to `grim`.
- **Zero Portal Latency**: Grabs compositor buffers directly, bypassing `xdg-desktop-portal` roundtrips to freeze screen state immediately on invocation.
- **Fractional Scaling Precision**: Per-output buffer scaling (`wl_surface.set_buffer_scale`) guarantees pixel-perfect rendering across mixed-DPI displays.
- **Native HUD Mode Selector (`kshot hud`)**: Centered Wayland Layer Shell popover offering single-key triggers for area capture, window capture, screen capture, scrolling capture, and companion tools.
- **Interactive Window Capture (`kshot window`)**: Queries compositor window tree via Umbriel IPC socket, highlighting window boundaries on hover with title and application ID badges.
- **Scrolling Screen Capture (`kshot scroll`)**: Vertical scrolling capture that correlates row-luminance profiles across consecutive frames, stitching continuously with a live thumbnail preview.
- **In-Process Beautify Engine (`--pretty`)**: Renders soft drop shadows, cubic-bezier rounded corners, and frame padding via `tiny-skia` directly in-process without ImageMagick.
- **Flameshot Annotation Suite**:
  - **Selection**: Crop region with 8 resize handles, drag-to-move, and full-screen toggle.
  - **Arrow**: Straight and curved arrowheads with configurable notch depth and stroke thickness.
  - **Rectangle & Circle**: Stroked or filled shapes (hold <kbd>Shift</kbd> while dragging to fill).
  - **Line**: Straight vector lines with <kbd>Shift</kbd> angle snapping (0°, 45°, 90°).
  - **Pencil**: Smooth freehand curves with antialiased strokes.
  - **Marker**: Translucent highlighter with high-contrast visibility on dark and light themes.
  - **Text**: Direct transparent text annotation with live cursor typing.
  - **Step Counter**: Auto-incrementing numbered badge bubbles (`1, 2, 3...`) with optional pointer lines; right-click decrements.
  - **Pixelate & Blur**: Mosaic downsampling and Gaussian blur for redacting sensitive data.
  - **Invert**: Color channel inversion within the selected region.
  - **Magnifier Loupe**: Circular pixel lens matching Flameshot geometry with center crosshair.
  - **History Stack**: Undo and redo stack (<kbd>Ctrl+Z</kbd>, <kbd>Ctrl+Y</kbd> / <kbd>Ctrl+Shift+Z</kbd>).
- **Floating Overlays (`kshot pin`)**: Pin any cropped region as a borderless, stays-on-top floating window with drag repositioning and mouse-wheel zoom.
- **Companion Tool Delegation**: Instant invocation of specialized utilities via HUD buttons (<kbd>A</kbd> for `cpick`, <kbd>S</kbd> for `ruler`, <kbd>D</kbd> for `qscan`, <kbd>F</kbd> for `ocr-screen`).
- **Declarative YAML Configuration**: Stores settings, key bindings, and appearance in `~/.config/kshot/config.yaml`.
- **Gruvbox Dark Aesthetic**: Consistent Gruvbox dark palette with customizable color swatches.

## CLI Usage

```bash
# Open native HUD mode selector overlay
kshot hud

# Open HUD with beautified export enabled (drop shadow, rounded corners, padding)
kshot hud --pretty

# Interactive GUI crop and annotation session (default when run without arguments)
kshot gui

# Interactive GUI with beautified export
kshot gui --pretty

# Copy directly to clipboard upon selection
kshot gui -c

# Delay capture by 500ms
kshot gui -d 500

# Pre-select region geometry
kshot gui -r "800x600+100+100"

# Output raw PNG stream to stdout
kshot gui --raw > screenshot.png

# Interactive window capture via Umbriel IPC
kshot window

# Interactive window capture with beautified shadow and rounded corners
kshot window --pretty

# Interactive window capture, opening result in annotation GUI
kshot window -e

# Interactive vertical scrolling screenshot
kshot scroll

# Vertical scrolling screenshot with beautified export
kshot scroll --pretty

# Capture full screen and save to default path
kshot screen

# Capture a specific monitor to clipboard
kshot screen -o DP-1 -c

# Capture a predefined area non-interactively
kshot area -g "1920x1080+0+0" -p ~/Pictures/capture.png

# Pin an existing image as a floating overlay
kshot pin ~/Pictures/reference.png

# Print path to configuration file
kshot config --path
```

## HUD Modes & Hotkeys

When launching `kshot hud`, a centered popover appears over the active output. Pressing any hotkey selects the corresponding mode:

| Key | Mode | Description | Target |
| :--- | :--- | :--- | :--- |
| <kbd>1</kbd> | Area | Interactive rectangular crop and annotation canvas | `kshot gui` |
| <kbd>2</kbd> | Window | Interactive window hover selection via Umbriel IPC | `kshot window` |
| <kbd>3</kbd> | Screen | Full screen capture of the focused monitor | `kshot screen` |
| <kbd>4</kbd> | Scroll | Vertical scrolling capture with live preview | `kshot scroll` |
| <kbd>A</kbd> | Picker | Magnified color loupe with HEX and RGB values | `cpick` |
| <kbd>S</kbd> | Ruler | On-screen distance and alignment measurement | `ruler` |
| <kbd>D</kbd> | Scanner | QR code and barcode recognition from screen | `qscan` |
| <kbd>F</kbd> | OCR | Optical character recognition to clipboard | `ocr-screen` |
| <kbd>Esc</kbd> | Close | Dismiss HUD overlay without action | Exit |

## Keyboard Controls (GUI Annotation Mode)

| Shortcut | Action |
| :--- | :--- |
| <kbd>S</kbd> | Selection tool |
| <kbd>A</kbd> | Arrow tool |
| <kbd>R</kbd> | Rectangle tool |
| <kbd>F</kbd> | Filled rectangle tool |
| <kbd>C</kbd> | Circle / ellipse tool |
| <kbd>P</kbd> | Pencil (freehand) |
| <kbd>M</kbd> | Marker (highlighter) |
| <kbd>T</kbd> | Text tool |
| <kbd>N</kbd> | Step counter (numbered bubble) |
| <kbd>B</kbd> | Pixelate redaction |
| <kbd>U</kbd> | Blur redaction |
| <kbd>I</kbd> | Invert colors |
| <kbd>?</kbd> | Toggle help cheatsheet |
| <kbd>Ctrl+C</kbd> | Copy selection to clipboard and exit |
| <kbd>Ctrl+S</kbd> | Save selection to file and exit |
| <kbd>Enter</kbd> | Confirm and copy to clipboard |
| <kbd>Space</kbd> | Pin crop to screen |
| <kbd>Ctrl+Z</kbd> | Undo last annotation |
| <kbd>Ctrl+Y</kbd> / <kbd>Ctrl+Shift+Z</kbd> | Redo annotation |
| <kbd>Ctrl+A</kbd> | Select entire screen |
| <kbd>Arrow Keys</kbd> | Nudge selection by 1px (hold <kbd>Shift</kbd> for 10px) |
| <kbd>Mouse Wheel</kbd> | Adjust tool thickness / zoom |
| <kbd>Right Click</kbd> | Decrement counter / circular color palette |
| <kbd>Esc</kbd> | Clear selection / cancel and exit |

## Configuration

On first launch, `kshot` automatically creates its configuration file at `~/.config/kshot/config.yaml`.

See [config.example.yaml](config.example.yaml) for the full configuration reference and default values.

## Compositor Hotkey Binding

Bind `kshot` commands to shortcuts in your Wayland compositor configuration.

### Umbriel

```toml
# Native HUD mode selector
"Mod+Shift+S" = "spawn kshot hud"

# HUD with beautified export (shadow, rounded corners, padding)
"Mod+Ctrl+Shift+S" = "spawn kshot hud --pretty"

# Direct interactive crop
"Print" = "spawn kshot gui"

# Full-screen capture to clipboard
"Shift+Print" = "spawn kshot screen -c"
```

### Hyprland

```ini
# Native HUD mode selector
bind = SUPER SHIFT, S, exec, kshot hud

# HUD with beautified export
bind = SUPER CTRL SHIFT, S, exec, kshot hud --pretty

# Direct interactive crop
bind = , Print, exec, kshot gui

# Full-screen capture to clipboard
bind = SHIFT, Print, exec, kshot screen -c
```

### Sway

```ini
# Native HUD mode selector
bindsym Mod4+Shift+s exec kshot hud

# HUD with beautified export
bindsym Mod4+Ctrl+Shift+s exec kshot hud --pretty

# Direct interactive crop
bindsym Print exec kshot gui

# Full-screen capture to clipboard
bindsym Shift+Print exec kshot screen -c
```

## Requirements

- **Linux** with a Wayland compositor implementing `zwlr_layer_shell_v1` (such as Umbriel, Hyprland, Sway, or River).
- **Runtime tools** (optional, recommended):
  - `wl-clipboard`: for Wayland clipboard export (`wl-copy`).
  - `grim`: fallback screen capture mechanism.
  - `notify-send`: for desktop capture notifications.
  - `cpick`, `ruler`, `qscan`, `ocr-screen`: companion tools for HUD delegation.

## Building from Source

Ensure you have Rust (stable 1.80+) and required development headers installed:

```bash
git clone https://github.com/koresent/kshot.git
cd kshot
cargo build --release
```

The compiled binary will be placed at `target/release/kshot`.

Install it to your local binary path:

```bash
install -Dm755 target/release/kshot ~/.local/bin/kshot
```

## Desktop Integration

`kshot` includes a desktop launcher entry (`kshot.desktop`) and an application icon (`assets/logo.png`):

```bash
install -Dm644 kshot.desktop ~/.local/share/applications/kshot.desktop
install -Dm644 assets/logo.png ~/.local/share/icons/hicolor/512x512/apps/kshot.png
```
