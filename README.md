<p align="center">
  <img src="assets/logo.png" width="96" alt="kshot logo" />
</p>

# kshot

Fast, native Wayland screenshot, cropping, and annotation utility written in Rust.

## Overview

`kshot` is a native Wayland screenshot and annotation utility designed as a lightweight alternative to Flameshot on Wayland compositors (such as Umbriel, Sway, Hyprland, and River). It captures directly via Wayland screencopy protocols, handles fractional scaling across mixed-DPI monitors, offers vector annotation tools in the Gruvbox Dark aesthetic, and configures via declarative YAML.

## Key Features

- **Native Wayland Screencopy**: Direct frame capture via `wlr-screencopy-unstable-v1` and `ext-image-copy-capture-v1`, with fallback to `grim`.
- **Zero Portal Latency**: Grabs compositor buffers directly, bypassing `xdg-desktop-portal` roundtrips to freeze screen state immediately on invocation.
- **Fractional Scaling Precision**: Per-output buffer scaling (`wl_surface.set_buffer_scale`) guarantees pixel-perfect rendering across mixed-DPI displays.
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
- **Declarative YAML Configuration**: Stores settings, key bindings, and appearance in `~/.config/kshot/config.yaml`.
- **Gruvbox Dark Aesthetic**: Consistent Gruvbox dark palette with customizable color swatches.

## CLI Usage

```bash
# Interactive GUI mode (default when run without arguments)
kshot gui

# Copy directly to clipboard upon selection
kshot gui -c

# Delay capture by 500ms
kshot gui -d 500

# Pre-select region geometry
kshot gui -r "800x600+100+100"

# Output raw PNG stream to stdout
kshot gui --raw > screenshot.png

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

## Keyboard Controls

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

### Hyprland / Umbriel

```ini
# Interactive GUI selection
bind = , Print, exec, kshot gui

# Full-screen capture to clipboard
bind = SHIFT, Print, exec, kshot screen -c

# Crop with immediate clipboard export
bind = CTRL, Print, exec, kshot gui -c
```

### Sway

```ini
# Interactive GUI selection
bindsym Print exec kshot gui

# Full-screen capture to clipboard
bindsym Shift+Print exec kshot screen -c

# Crop with immediate clipboard export
bindsym Ctrl+Print exec kshot gui -c
```

## Requirements

- **Linux** with a Wayland compositor implementing `zwlr_layer_shell_v1` (such as Umbriel, Hyprland, Sway, or River).
- **Runtime tools** (optional, recommended):
  - `wl-clipboard`: for Wayland clipboard export (`wl-copy`).
  - `grim`: fallback screen capture mechanism.
  - `notify-send`: for desktop capture notifications.

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
