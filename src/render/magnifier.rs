use tiny_skia::{Paint, PathBuilder, Pixmap, PixmapPaint, Stroke, Transform};

#[allow(dead_code)]
pub struct MagnifierConfig {
    pub zoom: u32,
    pub grid_size: u32,
}

impl Default for MagnifierConfig {
    fn default() -> Self {
        Self {
            zoom: 10,
            grid_size: 17,
        }
    }
}

/// Render 1:1 Flameshot-identical circular magnifier loupe (MagnifierWidget::drawMagnifierCircle)
/// Flameshot SSOT:
/// - m_magPixels = 8, magZoom = 10, m_pixels = 17
/// - radius = m_pixels * magZoom / 2 = 85.0 px (diameter = 170 px)
/// - m_magOffset = 16.0 px
/// - crosshair: 10px wide arms extending 80px top/bottom/left/right, leaving center 10x10 px open
/// - outer ring: stroke width 4.0 px in accent color
/// - NO color picker, NO hex codes, NO telemetry pills
pub fn render_circular_magnifier(
    canvas: &mut Pixmap,
    source_frame: &Pixmap,
    cursor_x: i32,
    cursor_y: i32,
    accent_color: (u8, u8, u8),
    _config: &MagnifierConfig,
) {
    const MAG_PIXELS: i32 = 8;
    const MAG_ZOOM: i32 = 10;
    const GRID_SIZE: i32 = 2 * MAG_PIXELS + 1; // 17
    const RADIUS: f32 = (GRID_SIZE * MAG_ZOOM) as f32 / 2.0; // 85.0
    const DIAMETER: u32 = (GRID_SIZE * MAG_ZOOM) as u32; // 170
    const MAG_OFFSET: f32 = 16.0;

    let canvas_w = canvas.width() as f32;
    let canvas_h = canvas.height() as f32;

    // Position loupe offset from cursor, flipping if clipping screen bounds (Flameshot logic)
    let mut draw_pos_x = cursor_x as f32 + MAG_OFFSET + RADIUS;
    if draw_pos_x > canvas_w - RADIUS - 4.0 {
        draw_pos_x = cursor_x as f32 - MAG_OFFSET - RADIUS;
    }
    let mut draw_pos_y = cursor_y as f32 + MAG_OFFSET + RADIUS;
    if draw_pos_y > canvas_h - RADIUS - 4.0 {
        draw_pos_y = cursor_y as f32 - MAG_OFFSET - RADIUS;
    }

    // Clamp inside canvas bounds
    draw_pos_x = draw_pos_x.clamp(RADIUS, canvas_w - RADIUS);
    draw_pos_y = draw_pos_y.clamp(RADIUS, canvas_h - RADIUS);

    // Create 170x170 pixmap with guaranteed zeroed transparent pixels
    let mut lens = match Pixmap::new(DIAMETER, DIAMETER) {
        Some(p) => p,
        None => return,
    };

    let src_w = source_frame.width() as i32;
    let src_h = source_frame.height() as i32;
    let src_data = source_frame.data();

    let half_d = RADIUS;
    let r_sq = RADIUS * RADIUS;
    let inner_r_sq = (RADIUS - 1.0) * (RADIUS - 1.0);

    let lens_data = lens.data_mut();

    // Render 170x170 circle with mathematical pixel-perfect circular masking
    for ly in 0..DIAMETER as i32 {
        for lx in 0..DIAMETER as i32 {
            let dx = lx as f32 + 0.5 - half_d;
            let dy = ly as f32 + 0.5 - half_d;
            let dist_sq = dx * dx + dy * dy;

            // Strictly outside circle: keep 100% transparent (no rectangular leak possible)
            if dist_sq > r_sq {
                continue;
            }

            let gx = lx / MAG_ZOOM;
            let gy = ly / MAG_ZOOM;

            // Check if this pixel is part of the Flameshot crosshair
            // Arms: top (gx=8, gy<8), bottom (gx=8, gy>8), left (gy=8, gx<8), right (gy=8, gx>8)
            // Center (gx=8, gy=8) is left open showing original zoomed pixel
            let is_crosshair = (gx == MAG_PIXELS && gy != MAG_PIXELS) || (gy == MAG_PIXELS && gx != MAG_PIXELS);

            let (r, g, b) = if is_crosshair {
                (accent_color.0, accent_color.1, accent_color.2)
            } else {
                let sample_x = cursor_x - MAG_PIXELS + gx;
                let sample_y = cursor_y - MAG_PIXELS + gy;

                if sample_x >= 0 && sample_x < src_w && sample_y >= 0 && sample_y < src_h {
                    let idx = ((sample_y * src_w + sample_x) * 4) as usize;
                    (src_data[idx], src_data[idx + 1], src_data[idx + 2])
                } else {
                    (0, 0, 0)
                }
            };

            // Anti-alias edge pixels with proper premultiplied alpha
            let alpha = if dist_sq > inner_r_sq {
                ((RADIUS - dist_sq.sqrt()).clamp(0.0, 1.0) * 255.0) as u32
            } else {
                255u32
            };

            let out_idx = ((ly * DIAMETER as i32 + lx) * 4) as usize;
            let r_pre = (r as u32 * alpha / 255) as u8;
            let g_pre = (g as u32 * alpha / 255) as u8;
            let b_pre = (b as u32 * alpha / 255) as u8;

            lens_data[out_idx] = r_pre;
            lens_data[out_idx + 1] = g_pre;
            lens_data[out_idx + 2] = b_pre;
            lens_data[out_idx + 3] = alpha as u8;
        }
    }

    // Blit lens onto main canvas at top-left of the circle
    let blit_x = (draw_pos_x - RADIUS).round() as i32;
    let blit_y = (draw_pos_y - RADIUS).round() as i32;
    canvas.draw_pixmap(
        blit_x,
        blit_y,
        lens.as_ref(),
        &PixmapPaint::default(),
        Transform::identity(),
        None,
    );

    // Draw outer circular border ring (Flameshot: pen.setWidth(4))
    if let Some(oval) = tiny_skia::Rect::from_xywh(
        draw_pos_x - RADIUS,
        draw_pos_y - RADIUS,
        DIAMETER as f32,
        DIAMETER as f32,
    ) {
        if let Some(path) = PathBuilder::from_oval(oval) {
            // Subtle outer dark shadow stroke
            let mut shadow_paint = Paint::default();
            shadow_paint.set_color_rgba8(20, 20, 20, 180);
            shadow_paint.anti_alias = true;
            let shadow_stroke = Stroke { width: 6.0, ..Default::default() };
            canvas.stroke_path(&path, &shadow_paint, &shadow_stroke, Transform::identity(), None);

            // 4px solid accent border ring (Flameshot style)
            let mut border_paint = Paint::default();
            border_paint.set_color_rgba8(accent_color.0, accent_color.1, accent_color.2, 255);
            border_paint.anti_alias = true;
            let border_stroke = Stroke { width: 4.0, ..Default::default() };
            canvas.stroke_path(&path, &border_paint, &border_stroke, Transform::identity(), None);
        }
    }
}
