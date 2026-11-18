use crate::render::tools::render_text;
use tiny_skia::{Paint, PathBuilder, Pixmap, PixmapPaint, Stroke, Transform};

pub struct MagnifierConfig {
    pub zoom: u32,
    pub grid_size: u32,
}

impl Default for MagnifierConfig {
    fn default() -> Self {
        Self {
            zoom: 10,
            grid_size: 11,
        }
    }
}

/// Render Flameshot-style circular magnifier loupe
pub fn render_circular_magnifier(
    canvas: &mut Pixmap,
    source_frame: &Pixmap,
    cursor_x: i32,
    cursor_y: i32,
    accent_color: (u8, u8, u8),
    config: &MagnifierConfig,
) {
    let grid_size = config.grid_size.max(5) | 1; // Ensure odd
    let half_grid = (grid_size / 2) as i32;
    let zoom = config.zoom.max(6);

    let radius = ((grid_size * zoom) as f32) / 2.0; // e.g. 11 * 10 / 2 = 55.0
    let diameter = (radius * 2.0).ceil() as u32;

    let canvas_w = canvas.width() as f32;
    let canvas_h = canvas.height() as f32;

    // Center position of circular loupe relative to canvas
    let offset_dist = 30.0 + radius;
    let mut center_x = cursor_x as f32 + offset_dist;
    let mut center_y = cursor_y as f32 + offset_dist;

    if center_x + radius + 10.0 > canvas_w {
        center_x = (cursor_x as f32 - offset_dist).max(radius + 10.0);
    }
    if center_y + radius + 30.0 > canvas_h {
        center_y = (cursor_y as f32 - offset_dist).max(radius + 10.0);
    }

    // 1. Create temporary buffer for magnified circular lens
    let mut lens = match Pixmap::new(diameter, diameter) {
        Some(p) => p,
        None => return,
    };

    let src_w = source_frame.width() as i32;
    let src_h = source_frame.height() as i32;
    let src_data = source_frame.data();

    let mut center_pixel_color = (0u8, 0u8, 0u8);

    // Render zoomed pixels into lens
    let r_sq = radius * radius;
    let half_d = radius;

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
                center_pixel_color = (r, g, b);
            }

            let px_x = gx as f32 * zoom as f32;
            let px_y = gy as f32 * zoom as f32;

            if let Some(pixel_rect) = tiny_skia::Rect::from_xywh(px_x, px_y, zoom as f32, zoom as f32) {
                let mut paint = Paint::default();
                paint.set_color_rgba8(r, g, b, 255);
                lens.fill_rect(pixel_rect, &paint, Transform::identity(), None);
            }
        }
    }

    // Circular crop mask on the lens pixmap
    let lens_data = lens.data_mut();
    for y in 0..diameter {
        for x in 0..diameter {
            let dx = x as f32 - half_d;
            let dy = y as f32 - half_d;
            let dist_sq = dx * dx + dy * dy;
            let idx = ((y * diameter + x) * 4) as usize;
            if dist_sq > r_sq {
                lens_data[idx + 3] = 0; // Transparent
            } else if dist_sq > (radius - 1.0) * (radius - 1.0) {
                // Anti-aliased outer edge
                let alpha = ((radius - dist_sq.sqrt()).clamp(0.0, 1.0) * 255.0) as u8;
                let orig_a = lens_data[idx + 3] as f32 / 255.0;
                let new_a = (alpha as f32 / 255.0) * orig_a;
                lens_data[idx] = (lens_data[idx] as f32 * new_a) as u8;
                lens_data[idx + 1] = (lens_data[idx + 1] as f32 * new_a) as u8;
                lens_data[idx + 2] = (lens_data[idx + 2] as f32 * new_a) as u8;
                lens_data[idx + 3] = (new_a * 255.0) as u8;
            }
        }
    }

    // 2. Draw crosshairs on the lens around the center pixel
    let center_px_start = half_grid as f32 * zoom as f32;
    let center_px_end = center_px_start + zoom as f32;

    let mut ch_paint = Paint::default();
    ch_paint.set_color_rgba8(accent_color.0, accent_color.1, accent_color.2, 220);

    // Crosshair top, bottom, left, right
    let ch_w = 1.5f32;
    let half_ch = ch_w / 2.0;

    // Top
    if let Some(r) = tiny_skia::Rect::from_xywh(half_d - half_ch, 0.0, ch_w, center_px_start) {
        lens.fill_rect(r, &ch_paint, Transform::identity(), None);
    }
    // Bottom
    if let Some(r) = tiny_skia::Rect::from_xywh(half_d - half_ch, center_px_end, ch_w, diameter as f32 - center_px_end) {
        lens.fill_rect(r, &ch_paint, Transform::identity(), None);
    }
    // Left
    if let Some(r) = tiny_skia::Rect::from_xywh(0.0, half_d - half_ch, center_px_start, ch_w) {
        lens.fill_rect(r, &ch_paint, Transform::identity(), None);
    }
    // Right
    if let Some(r) = tiny_skia::Rect::from_xywh(center_px_end, half_d - half_ch, diameter as f32 - center_px_end, ch_w) {
        lens.fill_rect(r, &ch_paint, Transform::identity(), None);
    }

    // Center targeted pixel box
    if let Some(target_rect) = tiny_skia::Rect::from_xywh(center_px_start, center_px_start, zoom as f32, zoom as f32) {
        let p = PathBuilder::from_rect(target_rect);
        let mut target_paint = Paint::default();
        target_paint.set_color_rgba8(accent_color.0, accent_color.1, accent_color.2, 255);
        let stroke = Stroke { width: 1.5, ..Default::default() };
        lens.stroke_path(&p, &target_paint, &stroke, Transform::identity(), None);
    }

    // 3. Blit lens onto main canvas
    let blit_x = center_x - half_d;
    let blit_y = center_y - half_d;
    canvas.draw_pixmap(
        blit_x.round() as i32,
        blit_y.round() as i32,
        lens.as_ref(),
        &PixmapPaint::default(),
        Transform::identity(),
        None,
    );

    // 4. Draw outer border rings around the lens
    if let Some(oval) = tiny_skia::Rect::from_xywh(blit_x, blit_y, diameter as f32, diameter as f32) {
        if let Some(path) = PathBuilder::from_oval(oval) {
            // Dark outer shadow ring
            let mut shadow_paint = Paint::default();
            shadow_paint.set_color_rgba8(20, 20, 20, 180);
            shadow_paint.anti_alias = true;
            let shadow_stroke = Stroke { width: 5.0, ..Default::default() };
            canvas.stroke_path(&path, &shadow_paint, &shadow_stroke, Transform::identity(), None);

            // Solid accent border ring (Flameshot style: pen.setWidth(4))
            let mut border_paint = Paint::default();
            border_paint.set_color_rgba8(accent_color.0, accent_color.1, accent_color.2, 255);
            border_paint.anti_alias = true;
            let border_stroke = Stroke { width: 3.5, ..Default::default() };
            canvas.stroke_path(&path, &border_paint, &border_stroke, Transform::identity(), None);
        }
    }

    // 5. Draw Info Pill below the circular loupe
    let pill_w = 110.0;
    let pill_h = 24.0;
    let pill_x = center_x - pill_w / 2.0;
    let pill_y = center_y + radius + 8.0;

    if pill_y + pill_h < canvas_h - 4.0 {
        if let Some(pr) = tiny_skia::Rect::from_xywh(pill_x, pill_y, pill_w, pill_h) {
            let path = PathBuilder::from_rect(pr);
            let mut pill_paint = Paint::default();
            pill_paint.set_color_rgba8(40, 40, 40, 240); // Gruvbox dark
            canvas.fill_rect(pr, &pill_paint, Transform::identity(), None);

            let mut border_paint = Paint::default();
            border_paint.set_color_rgba8(80, 73, 69, 255); // #504945
            let stroke = Stroke { width: 1.0, ..Default::default() };
            canvas.stroke_path(&path, &border_paint, &stroke, Transform::identity(), None);

            // Small color swatch
            if let Some(swatch) = tiny_skia::Rect::from_xywh(pill_x + 6.0, pill_y + 5.0, 14.0, 14.0) {
                let mut sp = Paint::default();
                sp.set_color_rgba8(center_pixel_color.0, center_pixel_color.1, center_pixel_color.2, 255);
                canvas.fill_rect(swatch, &sp, Transform::identity(), None);
            }

            // Hex text
            let hex_text = format!(
                "#{:02X}{:02X}{:02X}",
                center_pixel_color.0, center_pixel_color.1, center_pixel_color.2
            );
            render_text(canvas, &hex_text, pill_x + 25.0, pill_y + 16.0, 11.0, (235, 219, 178, 255), None);
        }
    }
}
