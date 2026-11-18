# kshot

High-performance, native Wayland screenshot, cropping, and annotation utility written in Rust.

`kshot` is designed as a modern, zero-legacy replacement for Flameshot under Wayland compositors (Umbriel, Sway, Hyprland, River). It eliminates X11 translation shims, multi-monitor fractional scaling alignment bugs, and desktop portal delays, delivering sub-millisecond frame freezes, vector annotations in the Gruvbox Dark aesthetic, and declarative YAML configuration.

---

## Features

- **Native Wayland Capture:** Frame capture via Wayland screencopy protocols (`wlr-screencopy-unstable-v1`, `ext-image-copy-capture-v1`) with fallback to `grim`.
- **Zero Startup Latency:** Eliminates the 1–2s `xdg-desktop-portal` delay; instantly freezes screen state on invocation.
- **Fractional Scaling Precision:** Per-output buffer scaling (`wl_surface.set_buffer_scale`) guarantees pixel-perfect rendering across mixed DPI monitors.
- **Full Flameshot Annotation Suite:**
  - **Selection:** Crop box with 8 magnetic resize handles, drag-to-move, and full-screen toggle.
  - **Arrow:** Straight and curved arrowheads with configurable notch depth and stroke thickness.
  - **Rectangle & Circle:** Stroked or filled shapes (hold `Shift` while dragging to fill).
  - **Line:** Straight vector line with `Shift` angle snapping (0°, 45°, 90°).
  - **Pencil:** Smooth freehand curves with antialiased strokes.
  - **Marker:** Translucent highlighter with `Multiply` blend mode to preserve text contrast.
  - **Step Counter:** Auto-incrementing numbered badge bubbles (`1, 2, 3...`) with optional pointer line; right-click decrements.
  - **Pixelate & Blur:** Mosaic downsampling and Gaussian blur for redacting passwords, tokens, and personal data.
  - **Invert:** Color channel inversion within the selected region.
  - **Magnifier Loupe:** Real-time zoomed pixel grid tracking the cursor with center crosshair, exact RGB hex/dec telemetry, and coordinates.
  - **Undo / Redo:** Full immutable history stack (`Ctrl+Z`, `Ctrl+Shift+Z` / `Ctrl+Y`).
- **Floating Pinned Windows (`kshot pin`):** Pin any cropped region as a borderless, stays-on-top floating window with drag repositioning and mouse-wheel zoom.
- **Declarative YAML Configuration:** Fully configurable via `~/.config/kshot/config.yaml`.
- **Gruvbox Dark Palette:** Native system theme integration with customizable palette swatches.

---

## Installation & Build

### Requirements
- Rust toolchain (1.80+)
- Wayland compositor supporting `zwlr_layer_shell_v1`
- `wl-clipboard` (for clipboard export)
- `notify-send` (for desktop notifications)

### Building from Source

```bash
git clone https://github.com/koresent/kshot.git
cd kshot
cargo build --release
sudo install -Dm755 target/release/kshot /usr/local/bin/kshot
```

---

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
kshot pin -f ~/Pictures/reference.png

# Print path to configuration file
kshot config --path
```

---

## Default Shortcuts

| Shortcut | Action |
| :--- | :--- |
| `S` | Selection tool |
| `A` | Arrow tool |
| `R` | Rectangle tool |
| `C` | Circle / Ellipse tool |
| `P` | Pencil (freehand) |
| `M` | Marker (highlighter) |
| `T` | Text tool |
| `N` | Step counter (numbered bubble) |
| `B` | Pixelate / blur redaction |
| `I` | Invert colors |
| `Ctrl + C` | Copy crop to clipboard and exit |
| `Ctrl + S` | Save crop to file and exit |
| `Enter` | Confirm and copy to clipboard |
| `Space` | Pin crop to screen |
| `Ctrl + Z` | Undo last annotation |
| `Ctrl + Shift + Z` / `Ctrl + Y` | Redo annotation |
| `Ctrl + A` | Select entire screen |
| `Arrow Keys` | Nudge selection by 1px (hold `Shift` for 10px) |
| `Mouse Wheel` | Adjust tool thickness / zoom |
| `Right Click` | Decrement counter / reset active tool |
| `Escape` | Clear selection / cancel and exit |

---

## Configuration (`~/.config/kshot/config.yaml`)

On first launch, `kshot` automatically creates `~/.config/kshot/config.yaml` with Gruvbox Dark defaults:

```yaml
general:
  save_path: ~/Pictures/Screenshots
  filename_pattern: Screenshot_%Y-%m-%d_%H-%M-%S.png
  auto_copy_on_save: true
  close_after_action: true
  show_notifications: true
  dim_opacity: 175
  magnifier_enabled: true
  magnifier_zoom: 8
  magnifier_size: 11

theme:
  background: "#282828"
  foreground: "#ebdbb2"
  accent: "#fe8019"
  selection_border: "#fabd2f"
  handle_color: "#fe8019"
  font_family: "JetBrainsMono Nerd Font"
  default_color: "#cc241d"
  default_thickness: 3
  swatches:
    - "#cc241d" # Red
    - "#98971a" # Green
    - "#d79921" # Yellow
    - "#458588" # Blue
    - "#b16286" # Purple
    - "#689d6a" # Aqua
    - "#fe8019" # Orange
    - "#ebdbb2" # Light
    - "#282828" # Dark
    - "#928374" # Gray

tools:
  initial_tool: selection
  arrow_style: curved
  counter_start: 1
  pixelate_block_size: 10
  blur_radius: 8

shortcuts:
  bindings:
    selection: s
    arrow: a
    rectangle: r
    circle: c
    pencil: p
    marker: m
    text: t
    counter: n
    pixelate: b
    invert: i
    copy: ctrl+c
    save: ctrl+s
    pin: space
    undo: ctrl+z
    redo: ctrl+shift+z
    select_all: ctrl+a
    cancel: escape
    confirm: enter
    toggle_help: question
```

---

## License

GPL-3.0-or-later.
