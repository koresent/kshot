use tiny_skia::{LineCap, Paint, PathBuilder, Pixmap, Stroke, Transform};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonId {
    ToolSelection,
    ToolArrow,
    ToolRectangle,
    ToolCircle,
    ToolMarker,
    ToolPencil,
    ToolText,
    ToolCounter,
    ToolPixelate,
    ToolInvert,
    ActionUndo,
    ActionRedo,
    ActionCopy,
    ActionSave,
    ActionPin,
    ActionCancel,
    ActionConfirm,
}

impl ButtonId {
    pub fn tooltip(&self) -> &'static str {
        match self {
            ButtonId::ToolSelection => "Selection (S)",
            ButtonId::ToolArrow => "Arrow (A)",
            ButtonId::ToolRectangle => "Rectangle (R)",
            ButtonId::ToolCircle => "Circle (C)",
            ButtonId::ToolMarker => "Marker (M)",
            ButtonId::ToolPencil => "Pencil (P)",
            ButtonId::ToolText => "Text (T)",
            ButtonId::ToolCounter => "Step Counter (N)",
            ButtonId::ToolPixelate => "Pixelate/Blur (B)",
            ButtonId::ToolInvert => "Invert Colors (I)",
            ButtonId::ActionUndo => "Undo (Ctrl+Z)",
            ButtonId::ActionRedo => "Redo (Ctrl+Shift+Z)",
            ButtonId::ActionCopy => "Copy to Clipboard (Ctrl+C)",
            ButtonId::ActionSave => "Save to File (Ctrl+S)",
            ButtonId::ActionPin => "Pin to Screen (Space)",
            ButtonId::ActionCancel => "Cancel (Esc)",
            ButtonId::ActionConfirm => "Confirm (Enter)",
        }
    }
}

pub struct UiButton {
    pub id: ButtonId,
    pub center_x: f32,
    pub center_y: f32,
    pub radius: f32,
    pub is_active: bool,
    pub is_hovered: bool,
}

impl UiButton {
    pub fn new(id: ButtonId, radius: f32) -> Self {
        Self {
            id,
            center_x: 0.0,
            center_y: 0.0,
            radius,
            is_active: false,
            is_hovered: false,
        }
    }

    pub fn contains(&self, px: f32, py: f32) -> bool {
        let dx = px - self.center_x;
        let dy = py - self.center_y;
        (dx * dx + dy * dy) <= (self.radius * self.radius)
    }

    pub fn render(&self, pixmap: &mut Pixmap, accent_color: (u8, u8, u8)) {
        let cx = self.center_x;
        let cy = self.center_y;
        let r = self.radius;

        let circle_rect = tiny_skia::Rect::from_xywh(cx - r, cy - r, r * 2.0, r * 2.0);
        let cr = match circle_rect {
            Some(rect) => rect,
            None => return,
        };

        let path = match PathBuilder::from_oval(cr) {
            Some(p) => p,
            None => return,
        };

        // 1. Draw Button Circle Background
        let mut bg_paint = Paint::default();
        if self.is_active {
            // Active tool background
            bg_paint.set_color_rgba8(accent_color.0, accent_color.1, accent_color.2, 230);
        } else if self.is_hovered {
            // Hovered background
            bg_paint.set_color_rgba8(80, 73, 69, 230); // Gruvbox bg3
        } else {
            // Normal background
            bg_paint.set_color_rgba8(50, 48, 47, 210); // Gruvbox bg1
        }
        bg_paint.anti_alias = true;
        pixmap.fill_path(&path, &bg_paint, tiny_skia::FillRule::Winding, Transform::identity(), None);

        // 2. Draw Circle Border
        let mut border_paint = Paint::default();
        if self.is_active {
            border_paint.set_color_rgba8(250, 189, 47, 255); // Gruvbox yellow
        } else if self.is_hovered {
            border_paint.set_color_rgba8(235, 219, 178, 200);
        } else {
            border_paint.set_color_rgba8(102, 92, 84, 180);
        }
        border_paint.anti_alias = true;
        let border_stroke = Stroke {
            width: if self.is_active { 2.0 } else { 1.5 },
            ..Default::default()
        };
        pixmap.stroke_path(&path, &border_paint, &border_stroke, Transform::identity(), None);

        // 3. Draw Icon Glyph
        let icon_color = if self.is_active {
            (40, 40, 40, 255) // Dark icon on bright active background
        } else if self.is_hovered {
            (253, 244, 193, 255)
        } else {
            (235, 219, 178, 240)
        };

        let mut icon_paint = Paint::default();
        icon_paint.set_color_rgba8(icon_color.0, icon_color.1, icon_color.2, icon_color.3);
        icon_paint.anti_alias = true;

        render_button_icon(pixmap, self.id, cx, cy, r * 0.55, &icon_paint);
    }
}

fn render_button_icon(
    pixmap: &mut Pixmap,
    id: ButtonId,
    cx: f32,
    cy: f32,
    size: f32,
    paint: &Paint,
) {
    let stroke_2 = Stroke {
        width: 2.0,
        line_cap: LineCap::Round,
        ..Default::default()
    };

    match id {
        ButtonId::ToolSelection => {
            // Dashed or border selection icon
            if let Some(r) = tiny_skia::Rect::from_xywh(cx - size * 0.7, cy - size * 0.7, size * 1.4, size * 1.4) {
                let p = PathBuilder::from_rect(r);
                pixmap.stroke_path(&p, paint, &stroke_2, Transform::identity(), None);
            }
        }
        ButtonId::ToolArrow => {
            // Arrow pointing up-right
            let mut pb = PathBuilder::new();
            pb.move_to(cx - size * 0.6, cy + size * 0.6);
            pb.line_to(cx + size * 0.6, cy - size * 0.6);
            pb.move_to(cx + size * 0.1, cy - size * 0.6);
            pb.line_to(cx + size * 0.6, cy - size * 0.6);
            pb.line_to(cx + size * 0.6, cy - size * 0.1);
            if let Some(p) = pb.finish() {
                pixmap.stroke_path(&p, paint, &stroke_2, Transform::identity(), None);
            }
        }
        ButtonId::ToolRectangle => {
            if let Some(r) = tiny_skia::Rect::from_xywh(cx - size * 0.7, cy - size * 0.5, size * 1.4, size) {
                let p = PathBuilder::from_rect(r);
                pixmap.stroke_path(&p, paint, &stroke_2, Transform::identity(), None);
            }
        }
        ButtonId::ToolCircle => {
            if let Some(r) = tiny_skia::Rect::from_xywh(cx - size * 0.65, cy - size * 0.65, size * 1.3, size * 1.3) {
                if let Some(p) = PathBuilder::from_oval(r) {
                    pixmap.stroke_path(&p, paint, &stroke_2, Transform::identity(), None);
                }
            }
        }
        ButtonId::ToolPencil => {
            let mut pb = PathBuilder::new();
            pb.move_to(cx - size * 0.6, cy + size * 0.6);
            pb.line_to(cx + size * 0.5, cy - size * 0.5);
            pb.line_to(cx + size * 0.7, cy - size * 0.3);
            pb.line_to(cx - size * 0.4, cy + size * 0.8);
            pb.close();
            if let Some(p) = pb.finish() {
                pixmap.stroke_path(&p, paint, &stroke_2, Transform::identity(), None);
            }
        }
        ButtonId::ToolMarker => {
            let mut pb = PathBuilder::new();
            pb.move_to(cx - size * 0.7, cy + size * 0.7);
            pb.line_to(cx - size * 0.3, cy + size * 0.7);
            pb.line_to(cx + size * 0.6, cy - size * 0.2);
            pb.line_to(cx + size * 0.2, cy - size * 0.6);
            pb.close();
            if let Some(p) = pb.finish() {
                pixmap.fill_path(&p, paint, tiny_skia::FillRule::Winding, Transform::identity(), None);
            }
        }
        ButtonId::ToolText => {
            let mut pb = PathBuilder::new();
            // Capital T
            pb.move_to(cx - size * 0.7, cy - size * 0.6);
            pb.line_to(cx + size * 0.7, cy - size * 0.6);
            pb.move_to(cx, cy - size * 0.6);
            pb.line_to(cx, cy + size * 0.7);
            if let Some(p) = pb.finish() {
                pixmap.stroke_path(&p, paint, &stroke_2, Transform::identity(), None);
            }
        }
        ButtonId::ToolCounter => {
            // Circle with '1'
            if let Some(r) = tiny_skia::Rect::from_xywh(cx - size * 0.7, cy - size * 0.7, size * 1.4, size * 1.4) {
                if let Some(p) = PathBuilder::from_oval(r) {
                    pixmap.stroke_path(&p, paint, &stroke_2, Transform::identity(), None);
                }
            }
            let mut pb = PathBuilder::new();
            pb.move_to(cx - size * 0.2, cy - size * 0.2);
            pb.line_to(cx, cy - size * 0.4);
            pb.line_to(cx, cy + size * 0.4);
            if let Some(p) = pb.finish() {
                pixmap.stroke_path(&p, paint, &stroke_2, Transform::identity(), None);
            }
        }
        ButtonId::ToolPixelate => {
            // 2x2 grid
            let s = size * 0.55;
            let gap = 1.0;
            for (gx, gy) in [(-s, -s), (gap, -s), (-s, gap), (gap, gap)] {
                if let Some(r) = tiny_skia::Rect::from_xywh(cx + gx, cy + gy, s - gap, s - gap) {
                    let p = PathBuilder::from_rect(r);
                    pixmap.fill_path(&p, paint, tiny_skia::FillRule::Winding, Transform::identity(), None);
                }
            }
        }
        ButtonId::ToolInvert => {
            // Yin-yang / half circle
            if let Some(r) = tiny_skia::Rect::from_xywh(cx - size * 0.7, cy - size * 0.7, size * 1.4, size * 1.4) {
                if let Some(p) = PathBuilder::from_oval(r) {
                    pixmap.stroke_path(&p, paint, &stroke_2, Transform::identity(), None);
                }
            }
            let mut pb = PathBuilder::new();
            pb.move_to(cx, cy - size * 0.7);
            pb.line_to(cx, cy + size * 0.7);
            if let Some(p) = pb.finish() {
                pixmap.stroke_path(&p, paint, &stroke_2, Transform::identity(), None);
            }
        }
        ButtonId::ActionUndo => {
            let mut pb = PathBuilder::new();
            pb.move_to(cx + size * 0.5, cy + size * 0.5);
            pb.quad_to(cx + size * 0.5, cy - size * 0.3, cx - size * 0.3, cy - size * 0.3);
            pb.move_to(cx, cy - size * 0.6);
            pb.line_to(cx - size * 0.4, cy - size * 0.3);
            pb.line_to(cx, cy);
            if let Some(p) = pb.finish() {
                pixmap.stroke_path(&p, paint, &stroke_2, Transform::identity(), None);
            }
        }
        ButtonId::ActionRedo => {
            let mut pb = PathBuilder::new();
            pb.move_to(cx - size * 0.5, cy + size * 0.5);
            pb.quad_to(cx - size * 0.5, cy - size * 0.3, cx + size * 0.3, cy - size * 0.3);
            pb.move_to(cx, cy - size * 0.6);
            pb.line_to(cx + size * 0.4, cy - size * 0.3);
            pb.line_to(cx, cy);
            if let Some(p) = pb.finish() {
                pixmap.stroke_path(&p, paint, &stroke_2, Transform::identity(), None);
            }
        }
        ButtonId::ActionCopy => {
            // Two overlapping rectangles
            if let Some(r1) = tiny_skia::Rect::from_xywh(cx - size * 0.3, cy - size * 0.6, size * 0.8, size * 0.9) {
                let p = PathBuilder::from_rect(r1);
                pixmap.stroke_path(&p, paint, &stroke_2, Transform::identity(), None);
            }
            if let Some(r2) = tiny_skia::Rect::from_xywh(cx - size * 0.6, cy - size * 0.2, size * 0.8, size * 0.9) {
                let p = PathBuilder::from_rect(r2);
                pixmap.fill_path(&p, paint, tiny_skia::FillRule::Winding, Transform::identity(), None);
            }
        }
        ButtonId::ActionSave => {
            // Tray with down-arrow
            let mut pb = PathBuilder::new();
            pb.move_to(cx - size * 0.7, cy + size * 0.2);
            pb.line_to(cx - size * 0.7, cy + size * 0.6);
            pb.line_to(cx + size * 0.7, cy + size * 0.6);
            pb.line_to(cx + size * 0.7, cy + size * 0.2);
            pb.move_to(cx, cy - size * 0.6);
            pb.line_to(cx, cy + size * 0.2);
            pb.move_to(cx - size * 0.35, cy - size * 0.15);
            pb.line_to(cx, cy + size * 0.2);
            pb.line_to(cx + size * 0.35, cy - size * 0.15);
            if let Some(p) = pb.finish() {
                pixmap.stroke_path(&p, paint, &stroke_2, Transform::identity(), None);
            }
        }
        ButtonId::ActionPin => {
            // Pushpin
            let mut pb = PathBuilder::new();
            pb.move_to(cx - size * 0.4, cy - size * 0.6);
            pb.line_to(cx + size * 0.4, cy - size * 0.6);
            pb.line_to(cx + size * 0.2, cy - size * 0.1);
            pb.line_to(cx - size * 0.2, cy - size * 0.1);
            pb.close();
            pb.move_to(cx, cy - size * 0.1);
            pb.line_to(cx, cy + size * 0.7);
            if let Some(p) = pb.finish() {
                pixmap.stroke_path(&p, paint, &stroke_2, Transform::identity(), None);
            }
        }
        ButtonId::ActionCancel => {
            // X
            let mut pb = PathBuilder::new();
            pb.move_to(cx - size * 0.5, cy - size * 0.5);
            pb.line_to(cx + size * 0.5, cy + size * 0.5);
            pb.move_to(cx + size * 0.5, cy - size * 0.5);
            pb.line_to(cx - size * 0.5, cy + size * 0.5);
            if let Some(p) = pb.finish() {
                pixmap.stroke_path(&p, paint, &stroke_2, Transform::identity(), None);
            }
        }
        ButtonId::ActionConfirm => {
            // Checkmark
            let mut pb = PathBuilder::new();
            pb.move_to(cx - size * 0.6, cy);
            pb.line_to(cx - size * 0.1, cy + size * 0.5);
            pb.line_to(cx + size * 0.6, cy - size * 0.5);
            if let Some(p) = pb.finish() {
                pixmap.stroke_path(&p, paint, &stroke_2, Transform::identity(), None);
            }
        }
    }
}
