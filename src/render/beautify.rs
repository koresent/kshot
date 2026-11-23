use tiny_skia::{
    Color, FillRule, FilterQuality, Paint, Path, PathBuilder, Pattern, Pixmap, SpreadMode, Stroke,
    Transform,
};

/// Builds a rounded rectangle path using cubic Bezier curves for smooth corners.
pub fn build_rounded_rect_path(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<Path> {
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    let r = r.min(w * 0.5).min(h * 0.5);
    if r <= 0.0 {
        let mut pb = PathBuilder::new();
        pb.move_to(x, y);
        pb.line_to(x + w, y);
        pb.line_to(x + w, y + h);
        pb.line_to(x, y + h);
        pb.close();
        return pb.finish();
    }

    let k = 0.55228475 * r;
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.cubic_to(x + w - r + k, y, x + w, y + r - k, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.cubic_to(x + w, y + h - r + k, x + w - r + k, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.cubic_to(x + r - k, y + h, x, y + h - r + k, x, y + h - r);
    pb.line_to(x, y + r);
    pb.cubic_to(x, y + r - k, x + r - k, y, x + r, y);
    pb.close();
    pb.finish()
}

/// Applies a 3-pass fast box blur to a 1-channel alpha buffer, approximating Gaussian blur.
fn box_blur_alpha(buf: &mut [u8], w: usize, h: usize, radius: usize) {
    if radius == 0 || w == 0 || h == 0 {
        return;
    }
    let mut temp = vec![0u8; w * h];

    for _ in 0..3 {
        // Horizontal pass
        for y in 0..h {
            let row_offset = y * w;
            let mut sum: u32 = 0;
            let div = (radius * 2 + 1) as u32;

            for x in 0..radius {
                sum += buf[row_offset + x.min(w - 1)] as u32;
            }
            sum += buf[row_offset] as u32 * (radius + 1) as u32;

            for x in 0..w {
                let left = x.saturating_sub(radius);
                let right = (x + radius).min(w - 1);
                temp[row_offset + x] = (sum / div) as u8;

                let next_in = buf[row_offset + right] as u32;
                let prev_out = buf[row_offset + left] as u32;
                sum = sum + next_in - prev_out;
            }
        }

        // Vertical pass
        for x in 0..w {
            let mut sum: u32 = 0;
            let div = (radius * 2 + 1) as u32;

            for y in 0..radius {
                sum += temp[y.min(h - 1) * w + x] as u32;
            }
            sum += temp[x] as u32 * (radius + 1) as u32;

            for y in 0..h {
                let top = y.saturating_sub(radius);
                let bottom = (y + radius).min(h - 1);
                buf[y * w + x] = (sum / div) as u8;

                let next_in = temp[bottom * w + x] as u32;
                let prev_out = temp[top * w + x] as u32;
                sum = sum + next_in - prev_out;
            }
        }
    }
}

/// Beautifies a source Pixmap by applying rounded corners, soft Gaussian drop shadow,
/// and surrounding padding.
pub fn beautify_pixmap(src: &Pixmap, radius: f32, padding: u32) -> Option<Pixmap> {
    let src_w = src.width();
    let src_h = src.height();
    if src_w == 0 || src_h == 0 {
        return None;
    }

    let pad = padding.max(20);
    let dst_w = src_w + pad * 2;
    let dst_h = src_h + pad * 2;

    let mut dst = Pixmap::new(dst_w, dst_h)?;

    // 1. Render drop shadow mask
    let shadow_radius = 16usize;
    let shadow_offset_y = 8.0f32;
    let shadow_x = pad as f32;
    let shadow_y = pad as f32 + shadow_offset_y;

    let shadow_path = build_rounded_rect_path(shadow_x, shadow_y, src_w as f32, src_h as f32, radius)?;

    let mut shadow_pixmap = Pixmap::new(dst_w, dst_h)?;
    let mut shadow_paint = Paint::default();
    shadow_paint.set_color(Color::from_rgba8(0, 0, 0, 110));
    shadow_paint.anti_alias = true;

    shadow_pixmap.fill_path(
        &shadow_path,
        &shadow_paint,
        FillRule::Winding,
        Transform::identity(),
        None,
    );

    // Extract alpha channel, blur it, and write back into shadow_pixmap
    let total_pixels = (dst_w * dst_h) as usize;
    let mut alpha_buf = vec![0u8; total_pixels];
    let shadow_data = shadow_pixmap.data();
    for i in 0..total_pixels {
        alpha_buf[i] = shadow_data[i * 4 + 3];
    }

    box_blur_alpha(&mut alpha_buf, dst_w as usize, dst_h as usize, shadow_radius);

    let shadow_data_mut = shadow_pixmap.data_mut();
    for i in 0..total_pixels {
        let a = alpha_buf[i];
        shadow_data_mut[i * 4] = 0;
        shadow_data_mut[i * 4 + 1] = 0;
        shadow_data_mut[i * 4 + 2] = 0;
        shadow_data_mut[i * 4 + 3] = a;
    }

    // Draw shadow onto dst
    dst.draw_pixmap(
        0,
        0,
        shadow_pixmap.as_ref(),
        &tiny_skia::PixmapPaint::default(),
        Transform::identity(),
        None,
    );

    // 2. Draw the rounded source image
    let content_path = build_rounded_rect_path(pad as f32, pad as f32, src_w as f32, src_h as f32, radius)?;

    let pattern = Pattern::new(
        src.as_ref(),
        SpreadMode::Pad,
        FilterQuality::Bilinear,
        1.0,
        Transform::from_translate(pad as f32, pad as f32),
    );

    let mut content_paint = Paint::default();
    content_paint.shader = pattern;
    content_paint.anti_alias = true;

    dst.fill_path(
        &content_path,
        &content_paint,
        FillRule::Winding,
        Transform::identity(),
        None,
    );

    // 3. Subtle outer border for dark/light contrast (Gruvbox subtle border)
    let mut border_paint = Paint::default();
    border_paint.set_color(Color::from_rgba8(235, 219, 178, 22)); // #ebdbb2 with subtle opacity
    border_paint.anti_alias = true;
    let mut stroke = Stroke::default();
    stroke.width = 1.0;

    dst.stroke_path(
        &content_path,
        &border_paint,
        &stroke,
        Transform::identity(),
        None,
    );

    Some(dst)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_rounded_rect() {
        let path = build_rounded_rect_path(10.0, 10.0, 100.0, 50.0, 8.0);
        assert!(path.is_some());
    }

    #[test]
    fn test_beautify_pixmap() {
        let mut src = Pixmap::new(100, 100).unwrap();
        src.fill(Color::from_rgba8(184, 187, 38, 255)); // Gruvbox green

        let beautified = beautify_pixmap(&src, 12.0, 30);
        assert!(beautified.is_some());
        let res = beautified.unwrap();
        assert_eq!(res.width(), 160);
        assert_eq!(res.height(), 160);
    }
}
