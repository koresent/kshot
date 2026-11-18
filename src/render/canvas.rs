use crate::capture::CaptureFrame;
use crate::config::Config;
use crate::render::magnifier::{render_circular_magnifier, MagnifierConfig};
use crate::render::selection::SelectionRect;
use crate::render::tools::{measure_text_width, render_text, Annotation, AnnotationStack};
use crate::ui::{ColorWheel, ToolbarManager};
use tiny_skia::{Paint, PathBuilder, Pixmap, Stroke, Transform};

#[derive(Debug, Clone)]
pub struct ActiveTextInput {
    pub x: f32,
    pub y: f32,
    pub text: String,
    pub color: (u8, u8, u8, u8),
    pub font_size: f32,
    pub cursor_visible: bool,
}

pub struct Canvas {
    pub base_frame: CaptureFrame,
    pub width: u32,
    pub height: u32,
    pub selection: Option<SelectionRect>,
    pub annotations: AnnotationStack,
    pub in_progress_annotation: Option<Annotation>,
    pub active_text_input: Option<ActiveTextInput>,
    pub cursor_pos: Option<(i32, i32)>,
    pub show_magnifier: bool,
    pub size_notification: Option<(String, std::time::Instant)>,
}

impl Canvas {
    pub fn new(frame: CaptureFrame, _config: &Config) -> Self {
        let w = frame.width;
        let h = frame.height;
        Self {
            base_frame: frame,
            width: w,
            height: h,
            selection: None,
            annotations: AnnotationStack::new(),
            in_progress_annotation: None,
            active_text_input: None,
            cursor_pos: None,
            show_magnifier: true,
            size_notification: None,
        }
    }

    /// Composite everything into target surface pixmap
    pub fn render_to(
        &self,
        target: &mut Pixmap,
        toolbar: &ToolbarManager,
        color_wheel: &ColorWheel,
        config: &Config,
    ) {
        let target_w = target.width();
        let target_h = target.height();
        let target_data = target.data_mut();
        let src_data = self.base_frame.pixmap.data();

        // 1. Copy base frozen frame
        if target_w == self.base_frame.width && target_h == self.base_frame.height {
            let total = (target_w * target_h * 4) as usize;
            if total <= target_data.len() && total <= src_data.len() {
                target_data[..total].copy_from_slice(&src_data[..total]);
            }
        } else {
            let copy_w = (target_w.min(self.base_frame.width)) as usize;
            let copy_h = (target_h.min(self.base_frame.height)) as usize;
            let dst_stride = target_w as usize * 4;
            let src_stride = self.base_frame.width as usize * 4;

            for row in 0..copy_h {
                let src_start = row * src_stride;
                let src_end = src_start + copy_w * 4;
                let dst_start = row * dst_stride;
                let dst_end = dst_start + copy_w * 4;

                if src_end <= src_data.len() && dst_end <= target_data.len() {
                    target_data[dst_start..dst_end].copy_from_slice(&src_data[src_start..src_end]);
                }
            }
        }

        // 2. Render committed annotations onto canvas
        for item in self.annotations.items() {
            item.render(target, None);
        }

        // 3. Render active in-progress annotation if drawing
        if let Some(ref item) = self.in_progress_annotation {
            item.render(target, None);
        }

        // 4. Render active text input if editing
        if let Some(ref input) = self.active_text_input {
            self.render_active_text(target, input);
        }

        // 5. Apply Dimming Mask outside the selection box (Flameshot exact rgba(0, 0, 0, 190))
        let dim_alpha = config.general.dim_opacity;
        if dim_alpha > 0 {
            self.render_dimming_mask(target, dim_alpha);
        }

        // 6. Render Selection Box Border and 8 Handles
        if let Some(ref sel) = self.selection {
            if !sel.is_empty() {
                self.render_selection_overlay(target, sel, config);
            }
        }

        // 7. Render Floating Toolbars
        let accent = crate::config::parse_hex_color(&config.theme.accent)
            .map(|(r, g, b, _)| (r, g, b))
            .unwrap_or((184, 187, 38));
        toolbar.render(target, self.selection.as_ref(), accent);

        // 8. Render Circular Color Wheel (if open)
        if color_wheel.visible {
            color_wheel.render(target, toolbar.active_color());
        }

        // 9. Render Circular Magnifier Loupe
        if self.show_magnifier && !color_wheel.visible {
            if let Some((cx, cy)) = self.cursor_pos {
                let mag_cfg = MagnifierConfig {
                    zoom: config.general.magnifier_zoom,
                    grid_size: config.general.magnifier_size,
                };
                render_circular_magnifier(
                    target,
                    &self.base_frame.pixmap,
                    cx,
                    cy,
                    accent,
                    &mag_cfg,
                );
            }
        }

        // 10. Render Tool Size Notification (Flameshot size notification)
        if let Some((ref text, timestamp)) = self.size_notification {
            if timestamp.elapsed().as_millis() < 1200 {
                self.render_size_badge(target, text, accent);
            }
        }
    }

    fn render_active_text(&self, target: &mut Pixmap, input: &ActiveTextInput) {
        let text_w = measure_text_width(&input.text, input.font_size, None);
        let box_w = (text_w + 24.0).max(64.0);
        let box_h = input.font_size * 1.35;
        let box_x = input.x - 4.0;
        let box_y = input.y - 4.0;

        // Background box
        if let Some(r) = tiny_skia::Rect::from_xywh(box_x, box_y, box_w, box_h) {
            let mut bg_paint = Paint::default();
            bg_paint.set_color_rgba8(40, 40, 40, 210); // Gruvbox dark
            target.fill_rect(r, &bg_paint, Transform::identity(), None);

            let path = PathBuilder::from_rect(r);
            let mut border_paint = Paint::default();
            border_paint.set_color_rgba8(input.color.0, input.color.1, input.color.2, 220);
            let stroke = Stroke { width: 1.5, ..Default::default() };
            target.stroke_path(&path, &border_paint, &stroke, Transform::identity(), None);
        }

        // Text content
        if !input.text.is_empty() {
            render_text(
                target,
                &input.text,
                input.x + 2.0,
                input.y + 2.0,
                input.font_size,
                input.color,
                None,
            );
        }

        // Blinking cursor |
        if input.cursor_visible {
            let cursor_x = input.x + text_w + 3.0;
            let cursor_y = input.y;
            if let Some(cr) = tiny_skia::Rect::from_xywh(cursor_x, cursor_y, 2.0, input.font_size) {
                let mut cp = Paint::default();
                cp.set_color_rgba8(input.color.0, input.color.1, input.color.2, 255);
                target.fill_rect(cr, &cp, Transform::identity(), None);
            }
        }
    }

    fn render_size_badge(&self, target: &mut Pixmap, text: &str, accent: (u8, u8, u8)) {
        let w = 84.0;
        let h = 32.0;
        let (bx, by) = if let Some((cx, cy)) = self.cursor_pos {
            let mut x = cx as f32 + 20.0;
            let mut y = cy as f32 - 40.0;
            if x + w > target.width() as f32 - 10.0 {
                x = cx as f32 - w - 20.0;
            }
            if y < 10.0 {
                y = cy as f32 + 20.0;
            }
            (x, y)
        } else {
            (24.0, 24.0)
        };

        if let Some(r) = tiny_skia::Rect::from_xywh(bx, by, w, h) {
            let path = PathBuilder::from_rect(r);
            let mut bg = Paint::default();
            bg.set_color_rgba8(40, 40, 40, 240);
            target.fill_rect(r, &bg, Transform::identity(), None);

            let mut border = Paint::default();
            border.set_color_rgba8(accent.0, accent.1, accent.2, 255);
            let s = Stroke { width: 1.5, ..Default::default() };
            target.stroke_path(&path, &border, &s, Transform::identity(), None);

            crate::render::tools::render_text_centered(
                target,
                text,
                bx + w / 2.0,
                by + h / 2.0,
                14.0,
                (235, 219, 178, 255),
                None,
            );
        }
    }

    fn render_dimming_mask(&self, target: &mut Pixmap, dim_alpha: u8) {
        let dim_color = (0u8, 0u8, 0u8, dim_alpha); // Flameshot exact pure black overlay rgba(0, 0, 0, opacity)
        let mut paint = Paint::default();
        paint.set_color_rgba8(dim_color.0, dim_color.1, dim_color.2, dim_color.3);
        paint.anti_alias = false;

        let w = self.width as f32;
        let h = self.height as f32;

        match self.selection.as_ref() {
            Some(sel) if !sel.is_empty() => {
                // Top region
                if sel.y > 0.0 {
                    if let Some(r) = tiny_skia::Rect::from_xywh(0.0, 0.0, w, sel.y) {
                        target.fill_rect(r, &paint, Transform::identity(), None);
                    }
                }
                // Bottom region
                let bottom_y = sel.y + sel.h;
                if bottom_y < h {
                    if let Some(r) = tiny_skia::Rect::from_xywh(0.0, bottom_y, w, h - bottom_y) {
                        target.fill_rect(r, &paint, Transform::identity(), None);
                    }
                }
                // Left region
                if sel.x > 0.0 {
                    if let Some(r) = tiny_skia::Rect::from_xywh(0.0, sel.y, sel.x, sel.h) {
                        target.fill_rect(r, &paint, Transform::identity(), None);
                    }
                }
                // Right region
                let right_x = sel.x + sel.w;
                if right_x < w {
                    if let Some(r) = tiny_skia::Rect::from_xywh(right_x, sel.y, w - right_x, sel.h) {
                        target.fill_rect(r, &paint, Transform::identity(), None);
                    }
                }
            }
            _ => {
                // Dim entire screen when no selection
                if let Some(r) = tiny_skia::Rect::from_xywh(0.0, 0.0, w, h) {
                    target.fill_rect(r, &paint, Transform::identity(), None);
                }
            }
        }
    }

    fn render_selection_overlay(&self, target: &mut Pixmap, sel: &SelectionRect, config: &Config) {
        // 1. Draw Selection Border (Gruvbox green #b8bb26)
        let border_color = crate::config::parse_hex_color(&config.theme.selection_border)
            .unwrap_or((184, 187, 38, 255));

        if let Some(r) = tiny_skia::Rect::from_xywh(sel.x, sel.y, sel.w, sel.h) {
            let path = PathBuilder::from_rect(r);
            let mut paint = Paint::default();
            paint.set_color_rgba8(border_color.0, border_color.1, border_color.2, 255);
            paint.anti_alias = true;
            let stroke = Stroke {
                width: 2.0,
                ..Default::default()
            };
            target.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
        }

        // 2. Draw 8 Resize Handles
        let handle_color = crate::config::parse_hex_color(&config.theme.handle_color)
            .unwrap_or((184, 187, 38, 255));
        let handle_radius = 5.0;

        let mut handle_paint = Paint::default();
        handle_paint.set_color_rgba8(handle_color.0, handle_color.1, handle_color.2, 255);
        handle_paint.anti_alias = true;

        let mut outline_paint = Paint::default();
        outline_paint.set_color_rgba8(40, 40, 40, 240);
        let outline_stroke = Stroke { width: 1.5, ..Default::default() };

        for (_, hx, hy) in sel.handle_positions() {
            if let Some(hr) = tiny_skia::Rect::from_xywh(hx - handle_radius, hy - handle_radius, handle_radius * 2.0, handle_radius * 2.0) {
                if let Some(p) = PathBuilder::from_oval(hr) {
                    target.fill_path(&p, &handle_paint, tiny_skia::FillRule::Winding, Transform::identity(), None);
                    target.stroke_path(&p, &outline_paint, &outline_stroke, Transform::identity(), None);
                }
            }
        }
    }

    /// Extract the cropped sub-image corresponding to the selection rectangle
    pub fn get_cropped_frame(&self) -> Option<CaptureFrame> {
        let sel = self.selection.as_ref()?;
        if sel.is_empty() {
            return None;
        }

        // Render annotations into a cloned pixmap before cropping
        let mut full_rendered = self.base_frame.pixmap.clone();
        for item in self.annotations.items() {
            item.render(&mut full_rendered, None);
        }

        if let Some(ref input) = self.active_text_input {
            if !input.text.is_empty() {
                render_text(
                    &mut full_rendered,
                    &input.text,
                    input.x + 2.0,
                    input.y + 2.0,
                    input.font_size,
                    input.color,
                    None,
                );
            }
        }

        let full_frame = CaptureFrame::new(self.width, self.height, full_rendered);
        full_frame
            .crop(
                sel.x.round() as i32,
                sel.y.round() as i32,
                sel.w.round() as u32,
                sel.h.round() as u32,
            )
            .ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_active_text_input_rendering_and_cropping() {
        let mut base = Pixmap::new(200, 200).unwrap();
        base.fill(tiny_skia::Color::from_rgba8(30, 30, 30, 255));
        let frame = CaptureFrame::new(200, 200, base);
        let config = Config::default();
        let mut canvas = Canvas::new(frame, &config);

        canvas.selection = Some(SelectionRect::new(20.0, 20.0, 100.0, 100.0));
        canvas.active_text_input = Some(ActiveTextInput {
            x: 30.0,
            y: 30.0,
            text: "Hello Wayland".to_string(),
            color: (184, 187, 38, 255),
            font_size: 16.0,
            cursor_visible: true,
        });

        let cropped = canvas.get_cropped_frame();
        assert!(cropped.is_some());
        let frame = cropped.unwrap();
        assert_eq!(frame.width, 100);
        assert_eq!(frame.height, 100);
    }
}
