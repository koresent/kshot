use crate::render::tools::render_text;
use tiny_skia::{Paint, PathBuilder, Pixmap, Stroke, Transform};

pub struct MagnifierConfig {
    pub zoom: u32,
    pub grid_size: u32,
}

impl Default for MagnifierConfig {
    fn default() -> Self {
        Self {
            zoom: 8,
            grid_size: 11,
        }
    }
}

pub fn render_magnifier(
    canvas: &mut Pixmap,
    source_frame: &Pixmap,
    cursor_x: i32,
    cursor_y: i32,
    config: &MagnifierConfig,
) {
    let grid_size = config.grid_size.max(5) | 1; // Ensure odd
    let half_grid = (grid_size / 2) as i32;
    let pixel_zoom = config.zoom.max(4);

    let grid_pixel_width = grid_size * pixel_zoom;
    let grid_pixel_height = grid_size * pixel_zoom;

    let card_padding = 10.0;
    let card_w = grid_pixel_width as f32 + card_padding * 2.0;
    let text_area_h = 50.0;
    let card_h = grid_pixel_height as f32 + card_padding * 2.0 + text_area_h;

    let canvas_w = canvas.width() as f32;
    let canvas_h = canvas.height() as f32;

    // Position card near cursor, flipping if clipping edges
    let mut card_x = cursor_x as f32 + 25.0;
    let mut card_y = cursor_y as f32 + 25.0;

    if card_x + card_w > canvas_w - 10.0 {
        card_x = (cursor_x as f32 - card_w - 25.0).max(10.0);
    }
    if card_y + card_h > canvas_h - 10.0 {
        card_y = (cursor_y as f32 - card_h - 25.0).max(10.0);
    }

    // 1. Draw Gruvbox Card Background
    let card_rect = tiny_skia::Rect::from_xywh(card_x, card_y, card_w, card_h);
    if let Some(cr) = card_rect {
        let path = PathBuilder::from_rect(cr);

        // Fill background
        let mut bg_paint = Paint::default();
        bg_paint.set_color_rgba8(40, 40, 40, 245); // Gruvbox dark
        bg_paint.anti_alias = true;
        canvas.fill_path(&path, &bg_paint, tiny_skia::FillRule::Winding, Transform::identity(), None);

        // Card border
        let mut border_paint = Paint::default();
        border_paint.set_color_rgba8(254, 128, 25, 230); // Gruvbox orange
        border_paint.anti_alias = true;
        let stroke = Stroke {
            width: 2.0,
            ..Default::default()
        };
        canvas.stroke_path(&path, &border_paint, &stroke, Transform::identity(), None);
    }

    // 2. Sample and Draw Zoom Grid
    let grid_start_x = card_x + card_padding;
    let grid_start_y = card_y + card_padding;

    let src_w = source_frame.width() as i32;
    let src_h = source_frame.height() as i32;
    let src_data = source_frame.data();

    let mut center_pixel_color = (0u8, 0u8, 0u8, 255u8);

    for gy in 0..grid_size as i32 {
        for gx in 0..grid_size as i32 {
            let sample_x = cursor_x - half_grid + gx;
            let sample_y = cursor_y - half_grid + gy;

            let (r, g, b) = if sample_x >= 0 && sample_x < src_w && sample_y >= 0 && sample_y < src_h {
                let idx = ((sample_y * src_w + sample_x) * 4) as usize;
                (src_data[idx], src_data[idx + 1], src_data[idx + 2])
            } else {
                (20, 20, 20)
            };

            if gx == half_grid && gy == half_grid {
                center_pixel_color = (r, g, b, 255);
            }

            let px_x = grid_start_x + (gx as f32 * pixel_zoom as f32);
            let px_y = grid_start_y + (gy as f32 * pixel_zoom as f32);

            if let Some(r_rect) = tiny_skia::Rect::from_xywh(px_x, px_y, pixel_zoom as f32, pixel_zoom as f32) {
                let p = PathBuilder::from_rect(r_rect);
                let mut p_paint = Paint::default();
                p_paint.set_color_rgba8(r, g, b, 255);
                canvas.fill_path(&p, &p_paint, tiny_skia::FillRule::Winding, Transform::identity(), None);
            }
        }
    }

    // 3. Draw Center Target Crosshair
    let target_x = grid_start_x + (half_grid as f32 * pixel_zoom as f32);
    let target_y = grid_start_y + (half_grid as f32 * pixel_zoom as f32);

    if let Some(target_rect) = tiny_skia::Rect::from_xywh(target_x, target_y, pixel_zoom as f32, pixel_zoom as f32) {
        let p = PathBuilder::from_rect(target_rect);
        let mut target_paint = Paint::default();
        target_paint.set_color_rgba8(250, 189, 47, 255); // Gruvbox yellow highlight
        let stroke = Stroke {
            width: 2.0,
            ..Default::default()
        };
        canvas.stroke_path(&p, &target_paint, &stroke, Transform::identity(), None);
    }

    // 4. Draw Telemetry Text (Hex, RGB, Pos)
    let info_start_y = grid_start_y + grid_pixel_height as f32 + 8.0;

    // Small color badge
    if let Some(swatch_rect) = tiny_skia::Rect::from_xywh(grid_start_x, info_start_y + 2.0, 14.0, 14.0) {
        let p = PathBuilder::from_rect(swatch_rect);
        let mut sp = Paint::default();
        sp.set_color_rgba8(center_pixel_color.0, center_pixel_color.1, center_pixel_color.2, 255);
        canvas.fill_path(&p, &sp, tiny_skia::FillRule::Winding, Transform::identity(), None);

        let mut b_paint = Paint::default();
        b_paint.set_color_rgba8(235, 219, 178, 200);
        let stroke = Stroke { width: 1.0, ..Default::default() };
        canvas.stroke_path(&p, &b_paint, &stroke, Transform::identity(), None);
    }

    let hex_text = format!(
        "#{:02X}{:02X}{:02X}",
        center_pixel_color.0, center_pixel_color.1, center_pixel_color.2
    );
    render_text(canvas, &hex_text, grid_start_x + 20.0, info_start_y + 13.0, 12.0, (235, 219, 178, 255), None);

    let coord_text = format!("X:{} Y:{}", cursor_x, cursor_y);
    render_text(canvas, &coord_text, grid_start_x, info_start_y + 32.0, 11.0, (168, 153, 132, 255), None);
}
