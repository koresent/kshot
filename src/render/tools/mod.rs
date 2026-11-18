pub mod arrow;
pub mod redact;

pub use arrow::{build_arrow_geometry, ArrowStyle};
pub use redact::{apply_blur, apply_invert, apply_pixelate};

use std::sync::LazyLock;
use tiny_skia::{
    BlendMode, LineCap, LineJoin, Paint, PathBuilder, Pixmap, Stroke, Transform,
};

pub static DEFAULT_FONT_BYTES: &[u8] = include_bytes!("../../../assets/fonts/JetBrainsMono-Bold.ttf");
pub static DEFAULT_FONT: LazyLock<fontdue::Font> = LazyLock::new(|| {
    fontdue::Font::from_bytes(DEFAULT_FONT_BYTES, fontdue::FontSettings::default())
        .expect("Failed to parse embedded default font")
});

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolType {
    Selection,
    Pencil,
    Line,
    Arrow,
    Rectangle,
    RectFilled,
    Circle,
    Marker,
    Text,
    Counter,
    Pixelate,
    Blur,
    Invert,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum Annotation {
    Pencil {
        points: Vec<(f32, f32)>,
        color: (u8, u8, u8, u8),
        thickness: f32,
    },
    Line {
        start: (f32, f32),
        end: (f32, f32),
        color: (u8, u8, u8, u8),
        thickness: f32,
    },
    Arrow {
        start: (f32, f32),
        end: (f32, f32),
        color: (u8, u8, u8, u8),
        thickness: f32,
        style: ArrowStyle,
    },
    Rectangle {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: (u8, u8, u8, u8),
        thickness: f32,
        filled: bool,
    },
    Circle {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: (u8, u8, u8, u8),
        thickness: f32,
        filled: bool,
    },
    Marker {
        points: Vec<(f32, f32)>,
        color: (u8, u8, u8, u8),
        thickness: f32,
    },
    Text {
        x: f32,
        y: f32,
        text: String,
        color: (u8, u8, u8, u8),
        font_size: f32,
    },
    Counter {
        number: u32,
        center: (f32, f32),
        pointer: Option<(f32, f32)>,
        color: (u8, u8, u8, u8),
        radius: f32,
    },
    Pixelate {
        x: i32,
        y: i32,
        w: u32,
        h: u32,
        block_size: u32,
    },
    Blur {
        x: i32,
        y: i32,
        w: u32,
        h: u32,
        radius: usize,
    },
    Invert {
        x: i32,
        y: i32,
        w: u32,
        h: u32,
    },
}

impl Annotation {
    pub fn render(&self, pixmap: &mut Pixmap, font_data: Option<&[u8]>) {
        match self {
            Annotation::Pencil {
                points,
                color,
                thickness,
            } => {
                if points.len() < 2 {
                    return;
                }
                let mut pb = PathBuilder::new();
                pb.move_to(points[0].0, points[0].1);
                for pt in &points[1..] {
                    pb.line_to(pt.0, pt.1);
                }
                if let Some(path) = pb.finish() {
                    let mut paint = Paint::default();
                    paint.set_color_rgba8(color.0, color.1, color.2, color.3);
                    paint.anti_alias = true;
                    let stroke = Stroke {
                        width: *thickness,
                        line_cap: LineCap::Round,
                        line_join: LineJoin::Round,
                        ..Default::default()
                    };
                    pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
                }
            }

            Annotation::Line {
                start,
                end,
                color,
                thickness,
            } => {
                let mut pb = PathBuilder::new();
                pb.move_to(start.0, start.1);
                pb.line_to(end.0, end.1);
                if let Some(path) = pb.finish() {
                    let mut paint = Paint::default();
                    paint.set_color_rgba8(color.0, color.1, color.2, color.3);
                    paint.anti_alias = true;
                    let stroke = Stroke {
                        width: *thickness,
                        line_cap: LineCap::Round,
                        ..Default::default()
                    };
                    pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
                }
            }

            Annotation::Arrow {
                start,
                end,
                color,
                thickness,
                style,
            } => {
                if let Some(geom) = build_arrow_geometry(*start, *end, *thickness, *style) {
                    let mut paint = Paint::default();
                    paint.set_color_rgba8(color.0, color.1, color.2, color.3);
                    paint.anti_alias = true;

                    // Stroke shaft
                    let stroke = Stroke {
                        width: *thickness,
                        line_cap: LineCap::Round,
                        ..Default::default()
                    };
                    pixmap.stroke_path(
                        &geom.shaft_path,
                        &paint,
                        &stroke,
                        Transform::identity(),
                        None,
                    );

                    // Fill head
                    pixmap.fill_path(
                        &geom.head_path,
                        &paint,
                        tiny_skia::FillRule::Winding,
                        Transform::identity(),
                        None,
                    );
                }
            }

            Annotation::Rectangle {
                x,
                y,
                w,
                h,
                color,
                thickness,
                filled,
            } => {
                let rect = tiny_skia::Rect::from_xywh(*x, *y, *w, *h);
                if let Some(r) = rect {
                    let path = PathBuilder::from_rect(r);
                    let mut paint = Paint::default();
                    paint.set_color_rgba8(color.0, color.1, color.2, color.3);
                    paint.anti_alias = true;

                    if *filled {
                        pixmap.fill_path(
                            &path,
                            &paint,
                            tiny_skia::FillRule::Winding,
                            Transform::identity(),
                            None,
                        );
                    } else {
                        let stroke = Stroke {
                            width: *thickness,
                            line_join: LineJoin::Miter,
                            ..Default::default()
                        };
                        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
                    }
                }
            }

            Annotation::Circle {
                x,
                y,
                w,
                h,
                color,
                thickness,
                filled,
            } => {
                let rect = tiny_skia::Rect::from_xywh(*x, *y, *w, *h);
                if let Some(r) = rect {
                    let path = PathBuilder::from_oval(r);
                    if let Some(p) = path {
                        let mut paint = Paint::default();
                        paint.set_color_rgba8(color.0, color.1, color.2, color.3);
                        paint.anti_alias = true;

                        if *filled {
                            pixmap.fill_path(
                                &p,
                                &paint,
                                tiny_skia::FillRule::Winding,
                                Transform::identity(),
                                None,
                            );
                        } else {
                            let stroke = Stroke {
                                width: *thickness,
                                ..Default::default()
                            };
                            pixmap.stroke_path(&p, &paint, &stroke, Transform::identity(), None);
                        }
                    }
                }
            }

            Annotation::Marker {
                points,
                color,
                thickness,
            } => {
                if points.len() < 2 {
                    return;
                }
                let mut pb = PathBuilder::new();
                pb.move_to(points[0].0, points[0].1);
                for pt in &points[1..] {
                    pb.line_to(pt.0, pt.1);
                }
                if let Some(path) = pb.finish() {
                    let mut paint = Paint::default();
                    // Multiply mode with ~40% opacity mimics highlighters perfectly
                    paint.set_color_rgba8(color.0, color.1, color.2, 110);
                    paint.blend_mode = BlendMode::Multiply;
                    paint.anti_alias = true;
                    let stroke = Stroke {
                        width: *thickness * 3.5,
                        line_cap: LineCap::Square,
                        line_join: LineJoin::Bevel,
                        ..Default::default()
                    };
                    pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
                }
            }

            Annotation::Counter {
                number,
                center,
                pointer,
                color,
                radius,
            } => {
                let mut paint = Paint::default();
                paint.set_color_rgba8(color.0, color.1, color.2, color.3);
                paint.anti_alias = true;

                // Draw optional pointer line
                if let Some(pt) = pointer {
                    let mut pb = PathBuilder::new();
                    pb.move_to(center.0, center.1);
                    pb.line_to(pt.0, pt.1);
                    if let Some(p) = pb.finish() {
                        let stroke = Stroke {
                            width: 3.0,
                            line_cap: LineCap::Round,
                            ..Default::default()
                        };
                        pixmap.stroke_path(&p, &paint, &stroke, Transform::identity(), None);
                    }
                }

                // Draw filled bubble circle
                let r = *radius;
                let circle_rect = tiny_skia::Rect::from_xywh(center.0 - r, center.1 - r, r * 2.0, r * 2.0);
                if let Some(cr) = circle_rect {
                    if let Some(p) = PathBuilder::from_oval(cr) {
                        pixmap.fill_path(
                            &p,
                            &paint,
                            tiny_skia::FillRule::Winding,
                            Transform::identity(),
                            None,
                        );

                        // Outline with dark contrast ring
                        let mut outline_paint = Paint::default();
                        outline_paint.set_color_rgba8(40, 40, 40, 220);
                        outline_paint.anti_alias = true;
                        let stroke = Stroke {
                            width: 2.0,
                            ..Default::default()
                        };
                        pixmap.stroke_path(&p, &outline_paint, &stroke, Transform::identity(), None);
                    }
                }

                // Render number text inside bubble
                let text_str = number.to_string();
                render_text_centered(pixmap, &text_str, center.0, center.1, r * 1.1, (255, 255, 255, 255), font_data);
            }

            Annotation::Text {
                x,
                y,
                text,
                color,
                font_size,
            } => {
                render_text(pixmap, text, *x, *y, *font_size, *color, font_data);
            }

            Annotation::Pixelate {
                x,
                y,
                w,
                h,
                block_size,
            } => {
                apply_pixelate(pixmap, *x, *y, *w, *h, *block_size);
            }

            Annotation::Blur { x, y, w, h, radius } => {
                apply_blur(pixmap, *x, *y, *w, *h, *radius);
            }

            Annotation::Invert { x, y, w, h } => {
                apply_invert(pixmap, *x, *y, *w, *h);
            }
        }
    }
}

pub fn render_text(
    pixmap: &mut Pixmap,
    text: &str,
    x: f32,
    y: f32,
    font_size: f32,
    color: (u8, u8, u8, u8),
    font_data: Option<&[u8]>,
) {
    if text.is_empty() {
        return;
    }
    let font_owned;
    let font: &fontdue::Font = if let Some(bytes) = font_data {
        font_owned = match fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default()) {
            Ok(f) => Some(f),
            Err(_) => return,
        };
        font_owned.as_ref().unwrap()
    } else {
        &DEFAULT_FONT
    };

    let mut cur_x = x;
    let mut cur_y = y;
    let line_height = font_size * 1.25;

    let px_w = pixmap.width() as i32;
    let px_h = pixmap.height() as i32;
    let data = pixmap.data_mut();

    for ch in text.chars() {
        if ch == '\n' {
            cur_x = x;
            cur_y += line_height;
            continue;
        }

        let (metrics, bitmap) = font.rasterize(ch, font_size);
        let gx = (cur_x + metrics.xmin as f32) as i32;
        let gy = (cur_y + font_size - metrics.height as f32 - metrics.ymin as f32) as i32;

        for r in 0..metrics.height {
            for c in 0..metrics.width {
                let alpha = bitmap[r * metrics.width + c];
                if alpha == 0 {
                    continue;
                }

                let target_x = gx + c as i32;
                let target_y = gy + r as i32;

                if target_x >= 0 && target_x < px_w && target_y >= 0 && target_y < px_h {
                    let idx = ((target_y * px_w + target_x) * 4) as usize;
                    let a = (alpha as u16 * color.3 as u16) / 255;
                    let inv_a = 255 - a;

                    data[idx] = ((color.0 as u16 * a + data[idx] as u16 * inv_a) / 255) as u8;
                    data[idx + 1] = ((color.1 as u16 * a + data[idx + 1] as u16 * inv_a) / 255) as u8;
                    data[idx + 2] = ((color.2 as u16 * a + data[idx + 2] as u16 * inv_a) / 255) as u8;
                    data[idx + 3] = (a + (data[idx + 3] as u16 * inv_a) / 255).min(255) as u8;
                }
            }
        }

        cur_x += metrics.advance_width;
    }
}

pub fn render_text_centered(
    pixmap: &mut Pixmap,
    text: &str,
    center_x: f32,
    center_y: f32,
    font_size: f32,
    color: (u8, u8, u8, u8),
    font_data: Option<&[u8]>,
) {
    let font_owned;
    let font: &fontdue::Font = if let Some(bytes) = font_data {
        font_owned = match fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default()) {
            Ok(f) => Some(f),
            Err(_) => return,
        };
        font_owned.as_ref().unwrap()
    } else {
        &DEFAULT_FONT
    };

    let mut total_width = 0.0;
    for ch in text.chars() {
        let (metrics, _) = font.rasterize(ch, font_size);
        total_width += metrics.advance_width;
    }

    let start_x = center_x - total_width * 0.5;
    let start_y = center_y - font_size * 0.55;

    render_text(pixmap, text, start_x, start_y, font_size, color, font_data);
}

pub fn measure_text_width(text: &str, font_size: f32, font_data: Option<&[u8]>) -> f32 {
    let font_owned;
    let font: &fontdue::Font = if let Some(bytes) = font_data {
        font_owned = match fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default()) {
            Ok(f) => Some(f),
            Err(_) => return text.len() as f32 * font_size * 0.6,
        };
        font_owned.as_ref().unwrap()
    } else {
        &DEFAULT_FONT
    };

    let mut w = 0.0f32;
    for ch in text.chars() {
        if ch == '\n' {
            break;
        }
        let (metrics, _) = font.rasterize(ch, font_size);
        w += metrics.advance_width;
    }
    w
}

#[derive(Default, Clone)]
pub struct AnnotationStack {
    undo_stack: Vec<Annotation>,
    redo_stack: Vec<Annotation>,
}

#[allow(dead_code)]
impl AnnotationStack {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, annotation: Annotation) {
        self.undo_stack.push(annotation);
        self.redo_stack.clear();
    }

    pub fn undo(&mut self) -> Option<Annotation> {
        let item = self.undo_stack.pop()?;
        self.redo_stack.push(item.clone());
        Some(item)
    }

    pub fn redo(&mut self) -> Option<Annotation> {
        let item = self.redo_stack.pop()?;
        self.undo_stack.push(item.clone());
        Some(item)
    }

    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    pub fn items(&self) -> &[Annotation] {
        &self.undo_stack
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_banner_text_rendering() {
        let mut pixmap = Pixmap::new(600, 100).unwrap();
        let text = "Select area • Double-click full screen • Esc / Right-click to exit";
        let banner_w = 560.0;
        let banner_h = 36.0;
        let bx = 20.0;
        let by = 24.0;
        let cx = bx + banner_w * 0.5;
        let cy = by + banner_h * 0.5;
        render_text_centered(&mut pixmap, text, cx, cy, 12.0, (235, 219, 178, 255), None);
        let mut min_y = 100;
        let mut max_y = 0;
        for y in 0..100 {
            for x in 0..600 {
                let a = pixmap.pixel(x, y).unwrap().alpha();
                if a > 0 {
                    if y < min_y { min_y = y; }
                    if y > max_y { max_y = y; }
                }
            }
        }
        assert!(min_y >= by as u32);
        assert!(max_y <= (by + banner_h) as u32);
    }
}
