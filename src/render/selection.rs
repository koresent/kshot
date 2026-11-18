#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handle {
    TopLeft,
    TopCenter,
    TopRight,
    MiddleLeft,
    MiddleRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SelectionRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl SelectionRect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }.normalized()
    }

    pub fn from_points(x1: f32, y1: f32, x2: f32, y2: f32) -> Self {
        let x = x1.min(x2);
        let y = y1.min(y2);
        let w = (x2 - x1).abs();
        let h = (y2 - y1).abs();
        Self { x, y, w, h }
    }

    pub fn normalized(self) -> Self {
        let mut r = self;
        if r.w < 0.0 {
            r.x += r.w;
            r.w = -r.w;
        }
        if r.h < 0.0 {
            r.y += r.h;
            r.h = -r.h;
        }
        r
    }

    pub fn is_empty(&self) -> bool {
        self.w <= 1.0 || self.h <= 1.0
    }

    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px <= self.x + self.w && py >= self.y && py <= self.y + self.h
    }

    pub fn handle_positions(&self) -> [(Handle, f32, f32); 8] {
        let left = self.x;
        let mid_x = self.x + self.w * 0.5;
        let right = self.x + self.w;

        let top = self.y;
        let mid_y = self.y + self.h * 0.5;
        let bottom = self.y + self.h;

        [
            (Handle::TopLeft, left, top),
            (Handle::TopCenter, mid_x, top),
            (Handle::TopRight, right, top),
            (Handle::MiddleLeft, left, mid_y),
            (Handle::MiddleRight, right, mid_y),
            (Handle::BottomLeft, left, bottom),
            (Handle::BottomCenter, mid_x, bottom),
            (Handle::BottomRight, right, bottom),
        ]
    }

    pub fn hit_handle(&self, px: f32, py: f32, handle_radius: f32) -> Option<Handle> {
        let threshold_sq = handle_radius * handle_radius;
        for (handle, hx, hy) in self.handle_positions() {
            let dx = px - hx;
            let dy = py - hy;
            if dx * dx + dy * dy <= threshold_sq {
                return Some(handle);
            }
        }
        None
    }

    pub fn resize_handle(&self, handle: Handle, new_x: f32, new_y: f32) -> Self {
        let right = self.x + self.w;
        let bottom = self.y + self.h;

        let (x1, y1, x2, y2) = match handle {
            Handle::TopLeft => (new_x, new_y, right, bottom),
            Handle::TopCenter => (self.x, new_y, right, bottom),
            Handle::TopRight => (self.x, new_y, new_x, bottom),
            Handle::MiddleLeft => (new_x, self.y, right, bottom),
            Handle::MiddleRight => (self.x, self.y, new_x, bottom),
            Handle::BottomLeft => (new_x, self.y, right, new_y),
            Handle::BottomCenter => (self.x, self.y, right, new_y),
            Handle::BottomRight => (self.x, self.y, new_x, new_y),
        };

        Self::from_points(x1, y1, x2, y2)
    }

    pub fn nudge(&mut self, dx: f32, dy: f32, max_w: f32, max_h: f32) {
        self.x = (self.x + dx).clamp(0.0, (max_w - self.w).max(0.0));
        self.y = (self.y + dy).clamp(0.0, (max_h - self.h).max(0.0));
    }

    #[allow(dead_code)]
    pub fn nudge_size(&mut self, dw: f32, dh: f32, max_w: f32, max_h: f32) {
        self.w = (self.w + dw).clamp(1.0, max_w - self.x);
        self.h = (self.h + dh).clamp(1.0, max_h - self.y);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_selection_rect() {
        let s = SelectionRect::from_points(100.0, 100.0, 50.0, 40.0);
        assert_eq!(s.x, 50.0);
        assert_eq!(s.y, 40.0);
        assert_eq!(s.w, 50.0);
        assert_eq!(s.h, 60.0);
        assert!(s.contains(70.0, 50.0));
        assert!(!s.contains(10.0, 10.0));
    }

    #[test]
    fn test_hit_handle() {
        let s = SelectionRect::new(10.0, 10.0, 100.0, 100.0);
        assert_eq!(s.hit_handle(10.0, 10.0, 8.0), Some(Handle::TopLeft));
        assert_eq!(s.hit_handle(110.0, 110.0, 8.0), Some(Handle::BottomRight));
        assert_eq!(s.hit_handle(60.0, 60.0, 8.0), None);
    }
}
