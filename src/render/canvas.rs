use crate::capture::CaptureFrame;
use crate::config::Config;
use crate::render::selection::SelectionRect;
use crate::render::tools::{Annotation, AnnotationStack};
use crate::ui::ToolbarManager;
use tiny_skia::{Paint, PathBuilder, Pixmap, Stroke, Transform};

pub struct Canvas {
    pub base_frame: CaptureFrame,
    pub width: u32,
    pub height: u32,
    pub selection: Option<SelectionRect>,
    pub annotations: AnnotationStack,
    pub in_progress_annotation: Option<Annotation>,
    pub cursor_pos: Option<(i32, i32)>,
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
            cursor_pos: None,
        }
    }

    /// Composite everything into target surface pixmap
    pub fn render_to(
        &self,
        target: &mut Pixmap,
        toolbar: &ToolbarManager,
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

        // 4. Apply Dimming Mask outside the selection box
        let dim_alpha = config.general.dim_opacity;
        if dim_alpha > 0 {
            self.render_dimming_mask(target, dim_alpha);
        }

        // 5. Render Selection Box Border and 8 Handles
        if let Some(ref sel) = self.selection {
            if !sel.is_empty() {
                self.render_selection_overlay(target, sel, config);
            }
        }

        // 6. Render Floating Toolbars
        let accent = crate::config::parse_hex_color(&config.theme.accent)
            .map(|(r, g, b, _)| (r, g, b))
            .unwrap_or((184, 187, 38));
        toolbar.render(target, self.selection.as_ref(), accent);
    }

    fn render_dimming_mask(&self, target: &mut Pixmap, dim_alpha: u8) {
        let dim_color = (29u8, 32u8, 33u8, dim_alpha); // Gruvbox dark veil #1d2021
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
