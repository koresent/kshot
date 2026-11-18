use crate::capture::CaptureFrame;
use crate::config::Config;
use crate::render::magnifier::{render_magnifier, MagnifierConfig};
use crate::render::selection::SelectionRect;
use crate::render::tools::{render_text, Annotation, AnnotationStack};
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
    pub show_magnifier: bool,
    pub magnifier_cfg: MagnifierConfig,
}

impl Canvas {
    pub fn new(frame: CaptureFrame, config: &Config) -> Self {
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
            show_magnifier: config.general.magnifier_enabled,
            magnifier_cfg: MagnifierConfig {
                zoom: config.general.magnifier_zoom,
                grid_size: config.general.magnifier_size,
            },
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

        // 1. Copy base frozen frame line by line
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

        // 5. Render Selection Box Border, Handles, and Geometry Badge
        if let Some(ref sel) = self.selection {
            if !sel.is_empty() {
                self.render_selection_overlay(target, sel, config);
            }
        }

        // 6. Render Floating Toolbars
        let accent = crate::config::parse_hex_color(&config.theme.accent)
            .map(|(r, g, b, _)| (r, g, b))
            .unwrap_or((254, 128, 25));
        toolbar.render(target, self.selection.as_ref(), accent);

        // 7. Render Magnifier Loupe if cursor is active and selection is empty or dragging
        if self.show_magnifier {
            if let Some((cx, cy)) = self.cursor_pos {
                let should_show = self.selection.is_none()
                    || self.selection.as_ref().map(|s| s.is_empty()).unwrap_or(true);
                if should_show {
                    render_magnifier(target, &self.base_frame.pixmap, cx, cy, &self.magnifier_cfg);
                }
            }
        }

        // 8. Render Precision Crosshair Cursor
        if let Some((cx, cy)) = self.cursor_pos {
            Self::render_crosshair(target, cx as f32, cy as f32);
        }
    }

fn render_crosshair(target: &mut Pixmap, cx: f32, cy: f32) {
    let mut dark_paint = Paint::default();
    dark_paint.set_color_rgba8(20, 20, 20, 220);
    dark_paint.anti_alias = false;

    let mut light_paint = Paint::default();
    light_paint.set_color_rgba8(235, 219, 178, 255);
    light_paint.anti_alias = false;

    let len = 12.0;
    let gap = 3.0;

    // Horizontal
    if let Some(r) = tiny_skia::Rect::from_xywh(cx - len - gap, cy - 1.0, len, 3.0) {
        target.fill_rect(r, &dark_paint, Transform::identity(), None);
    }
    if let Some(r) = tiny_skia::Rect::from_xywh(cx + gap, cy - 1.0, len, 3.0) {
        target.fill_rect(r, &dark_paint, Transform::identity(), None);
    }
    if let Some(r) = tiny_skia::Rect::from_xywh(cx - len - gap, cy, len, 1.0) {
        target.fill_rect(r, &light_paint, Transform::identity(), None);
    }
    if let Some(r) = tiny_skia::Rect::from_xywh(cx + gap, cy, len, 1.0) {
        target.fill_rect(r, &light_paint, Transform::identity(), None);
    }

    // Vertical
    if let Some(r) = tiny_skia::Rect::from_xywh(cx - 1.0, cy - len - gap, 3.0, len) {
        target.fill_rect(r, &dark_paint, Transform::identity(), None);
    }
    if let Some(r) = tiny_skia::Rect::from_xywh(cx - 1.0, cy + gap, 3.0, len) {
        target.fill_rect(r, &dark_paint, Transform::identity(), None);
    }
    if let Some(r) = tiny_skia::Rect::from_xywh(cx, cy - len - gap, 1.0, len) {
        target.fill_rect(r, &light_paint, Transform::identity(), None);
    }
    if let Some(r) = tiny_skia::Rect::from_xywh(cx, cy + gap, 1.0, len) {
        target.fill_rect(r, &light_paint, Transform::identity(), None);
    }

    // Center dot
    if let Some(r) = tiny_skia::Rect::from_xywh(cx - 1.0, cy - 1.0, 3.0, 3.0) {
        target.fill_rect(r, &dark_paint, Transform::identity(), None);
    }
    if let Some(r) = tiny_skia::Rect::from_xywh(cx, cy, 1.0, 1.0) {
        target.fill_rect(r, &light_paint, Transform::identity(), None);
    }
}

    fn render_dimming_mask(&self, target: &mut Pixmap, dim_alpha: u8) {
        let dim_color = (0u8, 0u8, 0u8, dim_alpha);
        let mut paint = Paint::default();
        paint.set_color_rgba8(dim_color.0, dim_color.1, dim_color.2, dim_color.3);
        paint.anti_alias = false;

        let w = self.width as f32;
        let h = self.height as f32;

        if let Some(ref sel) = self.selection {
            if !sel.is_empty() {
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
        }
    }

    fn render_selection_overlay(&self, target: &mut Pixmap, sel: &SelectionRect, config: &Config) {
        // 1. Draw Selection Border (Gruvbox yellow / bright border)
        let border_color = crate::config::parse_hex_color(&config.theme.selection_border)
            .unwrap_or((250, 189, 47, 255));

        if let Some(r) = tiny_skia::Rect::from_xywh(sel.x, sel.y, sel.w, sel.h) {
            let path = PathBuilder::from_rect(r);
            let mut paint = Paint::default();
            paint.set_color_rgba8(border_color.0, border_color.1, border_color.2, 240);
            paint.anti_alias = true;
            let stroke = Stroke {
                width: 2.0,
                ..Default::default()
            };
            target.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
        }

        // 2. Draw 8 Resize Handles
        let handle_color = crate::config::parse_hex_color(&config.theme.handle_color)
            .unwrap_or((254, 128, 25, 255));
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

        // 3. Draw Size Tooltip Badge ({w} x {h})
        let size_text = format!("{} × {}", sel.w.round() as i32, sel.h.round() as i32);
        let badge_w = size_text.len() as f32 * 7.5 + 16.0;
        let badge_h = 22.0;

        let badge_x = sel.x;
        let badge_y = if sel.y >= badge_h + 6.0 {
            sel.y - badge_h - 4.0
        } else {
            sel.y + 6.0
        };

        if let Some(br) = tiny_skia::Rect::from_xywh(badge_x, badge_y, badge_w, badge_h) {
            let bp = PathBuilder::from_rect(br);
            let mut bg_paint = Paint::default();
            bg_paint.set_color_rgba8(40, 40, 40, 230);
            target.fill_path(&bp, &bg_paint, tiny_skia::FillRule::Winding, Transform::identity(), None);

            let mut sp = Paint::default();
            sp.set_color_rgba8(250, 189, 47, 200);
            let stroke = Stroke { width: 1.0, ..Default::default() };
            target.stroke_path(&bp, &sp, &stroke, Transform::identity(), None);

            render_text(
                target,
                &size_text,
                badge_x + 8.0,
                badge_y + 15.0,
                11.0,
                (235, 219, 178, 255),
                None,
            );
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
