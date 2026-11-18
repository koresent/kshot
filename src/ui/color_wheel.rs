use tiny_skia::{Paint, PathBuilder, Pixmap, Stroke, Transform};

#[derive(Debug, Clone)]
pub struct ColorWheel {
    pub visible: bool,
    pub center_x: f32,
    pub center_y: f32,
    pub swatches: Vec<(u8, u8, u8, u8)>,
    pub ring_radius: f32,
    pub swatch_radius: f32,
    pub hovered_index: Option<usize>,
}

impl ColorWheel {
    pub fn new() -> Self {
        // Gruvbox-aligned rich palette
        let swatches = vec![
            (184, 187, 38, 255),  // #b8bb26 Green
            (142, 192, 124, 255), // #8ec07c Aqua
            (131, 165, 152, 255), // #83a598 Blue
            (211, 134, 155, 255), // #d3869b Purple
            (251, 73, 52, 255),   // #fb4934 Red
            (254, 128, 25, 255),  // #fe8019 Orange
            (250, 189, 47, 255),  // #fabd2f Yellow
            (235, 219, 178, 255), // #ebdbb2 Cream/White
            (146, 131, 116, 255), // #928374 Gray
            (40, 40, 40, 255),    // #282828 Dark
        ];

        Self {
            visible: false,
            center_x: 0.0,
            center_y: 0.0,
            swatches,
            ring_radius: 56.0,
            swatch_radius: 11.0,
            hovered_index: None,
        }
    }

    pub fn show_at(&mut self, cx: f32, cy: f32, max_w: f32, max_h: f32) {
        let margin = self.ring_radius + self.swatch_radius + 16.0;
        self.center_x = cx.clamp(margin, max_w - margin);
        self.center_y = cy.clamp(margin, max_h - margin);
        self.visible = true;
        self.hovered_index = None;
    }

    pub fn hide(&mut self) {
        self.visible = false;
        self.hovered_index = None;
    }

    pub fn swatch_centers(&self) -> Vec<(f32, f32)> {
        let n = self.swatches.len();
        let angle_step = std::f32::consts::TAU / n as f32;
        // Start from top (-PI/2)
        let mut angle = -std::f32::consts::FRAC_PI_2;

        let mut centers = Vec::with_capacity(n);
        for _ in 0..n {
            let sx = self.center_x + angle.cos() * self.ring_radius;
            let sy = self.center_y + angle.sin() * self.ring_radius;
            centers.push((sx, sy));
            angle += angle_step;
        }
        centers
    }

    pub fn handle_mouse_move(&mut self, px: f32, py: f32) -> bool {
        if !self.visible {
            return false;
        }
        let centers = self.swatch_centers();
        let mut found = None;
        for (i, (sx, sy)) in centers.into_iter().enumerate() {
            let dx = px - sx;
            let dy = py - sy;
            if dx * dx + dy * dy <= (self.swatch_radius + 3.0).powi(2) {
                found = Some(i);
                break;
            }
        }
        let changed = self.hovered_index != found;
        self.hovered_index = found;
        changed
    }

    pub fn handle_click(&mut self, px: f32, py: f32) -> Option<(u8, u8, u8, u8)> {
        if !self.visible {
            return None;
        }
        let centers = self.swatch_centers();
        for (i, (sx, sy)) in centers.into_iter().enumerate() {
            let dx = px - sx;
            let dy = py - sy;
            if dx * dx + dy * dy <= (self.swatch_radius + 4.0).powi(2) {
                let chosen = self.swatches[i];
                self.hide();
                return Some(chosen);
            }
        }
        None
    }

    pub fn render(&self, pixmap: &mut Pixmap, active_color: (u8, u8, u8, u8)) {
        if !self.visible {
            return;
        }

        let bg_radius = self.ring_radius + self.swatch_radius + 10.0;
        let bg_d = bg_radius * 2.0;

        // 1. Draw circular background backdrop
        if let Some(bg_rect) = tiny_skia::Rect::from_xywh(
            self.center_x - bg_radius,
            self.center_y - bg_radius,
            bg_d,
            bg_d,
        ) {
            if let Some(path) = PathBuilder::from_oval(bg_rect) {
                // Drop shadow
                let mut shadow_paint = Paint::default();
                shadow_paint.set_color_rgba8(20, 20, 20, 140);
                shadow_paint.anti_alias = true;
                let shadow_stroke = Stroke { width: 4.0, ..Default::default() };
                pixmap.stroke_path(&path, &shadow_paint, &shadow_stroke, Transform::identity(), None);

                // Backdrop fill
                let mut bg_paint = Paint::default();
                bg_paint.set_color_rgba8(40, 40, 40, 245); // Gruvbox dark
                bg_paint.anti_alias = true;
                pixmap.fill_path(&path, &bg_paint, tiny_skia::FillRule::Winding, Transform::identity(), None);

                // Backdrop border
                let mut border_paint = Paint::default();
                border_paint.set_color_rgba8(80, 73, 69, 255); // #504945
                border_paint.anti_alias = true;
                let stroke = Stroke { width: 1.5, ..Default::default() };
                pixmap.stroke_path(&path, &border_paint, &stroke, Transform::identity(), None);
            }
        }

        // 2. Draw Center Current Color Preview Disk
        let center_disk_radius = 16.0;
        if let Some(cdr) = tiny_skia::Rect::from_xywh(
            self.center_x - center_disk_radius,
            self.center_y - center_disk_radius,
            center_disk_radius * 2.0,
            center_disk_radius * 2.0,
        ) {
            if let Some(path) = PathBuilder::from_oval(cdr) {
                let mut cp = Paint::default();
                cp.set_color_rgba8(active_color.0, active_color.1, active_color.2, 255);
                cp.anti_alias = true;
                pixmap.fill_path(&path, &cp, tiny_skia::FillRule::Winding, Transform::identity(), None);

                let mut stroke_paint = Paint::default();
                stroke_paint.set_color_rgba8(235, 219, 178, 200);
                let s = Stroke { width: 1.5, ..Default::default() };
                pixmap.stroke_path(&path, &stroke_paint, &s, Transform::identity(), None);
            }
        }

        // 3. Draw Color Swatches around the Ring
        let centers = self.swatch_centers();
        for (i, (sx, sy)) in centers.into_iter().enumerate() {
            let swatch_color = self.swatches[i];
            let is_selected = (swatch_color.0, swatch_color.1, swatch_color.2)
                == (active_color.0, active_color.1, active_color.2);
            let is_hovered = self.hovered_index == Some(i);

            let r = if is_hovered {
                self.swatch_radius + 2.0
            } else {
                self.swatch_radius
            };

            // Selected or hovered outer highlight ring (Flameshot style)
            if is_selected || is_hovered {
                let highlight_r = r + 4.0;
                if let Some(hr) = tiny_skia::Rect::from_xywh(
                    sx - highlight_r,
                    sy - highlight_r,
                    highlight_r * 2.0,
                    highlight_r * 2.0,
                ) {
                    if let Some(hp) = PathBuilder::from_oval(hr) {
                        let mut hl_paint = Paint::default();
                        hl_paint.set_color_rgba8(swatch_color.0, swatch_color.1, swatch_color.2, 120);
                        hl_paint.anti_alias = true;
                        pixmap.fill_path(&hp, &hl_paint, tiny_skia::FillRule::Winding, Transform::identity(), None);

                        let mut hl_stroke_paint = Paint::default();
                        hl_stroke_paint.set_color_rgba8(235, 219, 178, 220);
                        let hl_stroke = Stroke { width: 1.5, ..Default::default() };
                        pixmap.stroke_path(&hp, &hl_stroke_paint, &hl_stroke, Transform::identity(), None);
                    }
                }
            }

            // Swatch circle fill
            if let Some(sr) = tiny_skia::Rect::from_xywh(sx - r, sy - r, r * 2.0, r * 2.0) {
                if let Some(sp) = PathBuilder::from_oval(sr) {
                    let mut paint = Paint::default();
                    paint.set_color_rgba8(swatch_color.0, swatch_color.1, swatch_color.2, 255);
                    paint.anti_alias = true;
                    pixmap.fill_path(&sp, &paint, tiny_skia::FillRule::Winding, Transform::identity(), None);

                    let mut border_paint = Paint::default();
                    border_paint.set_color_rgba8(20, 20, 20, 200);
                    let border_stroke = Stroke { width: 1.0, ..Default::default() };
                    pixmap.stroke_path(&sp, &border_paint, &border_stroke, Transform::identity(), None);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_wheel_geometry_and_hit_testing() {
        let mut wheel = ColorWheel::new();
        assert!(!wheel.visible);
        assert_eq!(wheel.swatches.len(), 10);

        wheel.show_at(200.0, 200.0, 1920.0, 1080.0);
        assert!(wheel.visible);
        assert_eq!(wheel.center_x, 200.0);
        assert_eq!(wheel.center_y, 200.0);

        let centers = wheel.swatch_centers();
        assert_eq!(centers.len(), 10);

        // Top swatch is at (center_x, center_y - ring_radius)
        let (first_x, first_y) = centers[0];
        assert!((first_x - 200.0).abs() < 1e-4);
        assert!((first_y - (200.0 - wheel.ring_radius)).abs() < 1e-4);

        // Test mouse move hover detection
        let hover_changed = wheel.handle_mouse_move(first_x, first_y);
        assert!(hover_changed);
        assert_eq!(wheel.hovered_index, Some(0));

        // Test hit click
        let picked = wheel.handle_click(first_x, first_y);
        assert_eq!(picked, Some(wheel.swatches[0]));
        assert!(!wheel.visible);
    }
}
