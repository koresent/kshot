use tiny_skia::{LineCap, LineJoin, Paint, PathBuilder, Pixmap, Stroke, Transform};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonId {
    ToolSelection,
    ToolPencil,
    ToolLine,
    ToolArrow,
    ToolRectangle,
    ToolRectFilled,
    ToolCircle,
    ToolMarker,
    ToolText,
    ToolCounter,
    ToolBlur,
    ToolPixelate,
    #[allow(dead_code)]
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
            ButtonId::ToolSelection => "Move Selection (S)",
            ButtonId::ToolPencil => "Pencil (P)",
            ButtonId::ToolLine => "Line (L)",
            ButtonId::ToolArrow => "Arrow (A)",
            ButtonId::ToolRectangle => "Rectangle (R)",
            ButtonId::ToolRectFilled => "Filled Rectangle",
            ButtonId::ToolCircle => "Circle (C)",
            ButtonId::ToolMarker => "Marker (M)",
            ButtonId::ToolText => "Text (T)",
            ButtonId::ToolCounter => "Step Counter (N)",
            ButtonId::ToolBlur => "Blur (B)",
            ButtonId::ToolPixelate => "Pixelate",
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
            center_x: -1000.0,
            center_y: -1000.0,
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

        let circle_rect = match tiny_skia::Rect::from_xywh(cx - r, cy - r, r * 2.0, r * 2.0) {
            Some(rect) => rect,
            None => return,
        };

        let path = match PathBuilder::from_oval(circle_rect) {
            Some(p) => p,
            None => return,
        };

        let mut bg_paint = Paint::default();
        if self.is_active {
            bg_paint.set_color_rgba8(accent_color.0, accent_color.1, accent_color.2, 255);
        } else if self.is_hovered {
            bg_paint.set_color_rgba8(60, 56, 54, 245);
        } else {
            bg_paint.set_color_rgba8(40, 40, 40, 240);
        }
        bg_paint.anti_alias = true;
        pixmap.fill_path(&path, &bg_paint, tiny_skia::FillRule::Winding, Transform::identity(), None);

        let mut border_paint = Paint::default();
        if self.is_active {
            border_paint.set_color_rgba8(accent_color.0, accent_color.1, accent_color.2, 255);
        } else if self.is_hovered {
            border_paint.set_color_rgba8(102, 92, 84, 255);
        } else {
            border_paint.set_color_rgba8(80, 73, 69, 220);
        }
        border_paint.anti_alias = true;
        let border_stroke = Stroke {
            width: if self.is_active { 1.5 } else { 1.0 },
            ..Default::default()
        };
        pixmap.stroke_path(&path, &border_paint, &border_stroke, Transform::identity(), None);

        let icon_color = if self.is_active {
            (29, 32, 33, 255) // #1d2021 dark contrast on active green background
        } else if self.is_hovered {
            (251, 241, 199, 255) // #fbf1c7 light high contrast
        } else {
            (235, 219, 178, 245) // #ebdbb2 Gruvbox fg
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
        width: 1.8,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Default::default()
    };

    match id {
        ButtonId::ToolSelection => {
            // 4-way compass arrow (move selection)
            let mut pb = PathBuilder::new();
            // Horizontal line & heads
            pb.move_to(cx - size * 0.7, cy);
            pb.line_to(cx + size * 0.7, cy);
            pb.move_to(cx - size * 0.4, cy - size * 0.3);
            pb.line_to(cx - size * 0.7, cy);
            pb.line_to(cx - size * 0.4, cy + size * 0.3);
            pb.move_to(cx + size * 0.4, cy - size * 0.3);
            pb.line_to(cx + size * 0.7, cy);
            pb.line_to(cx + size * 0.4, cy + size * 0.3);

            // Vertical line & heads
            pb.move_to(cx, cy - size * 0.7);
            pb.line_to(cx, cy + size * 0.7);
            pb.move_to(cx - size * 0.3, cy - size * 0.4);
            pb.line_to(cx, cy - size * 0.7);
            pb.line_to(cx + size * 0.3, cy - size * 0.4);
            pb.move_to(cx - size * 0.3, cy + size * 0.4);
            pb.line_to(cx, cy + size * 0.7);
            pb.line_to(cx + size * 0.3, cy + size * 0.4);

            if let Some(p) = pb.finish() {
                pixmap.stroke_path(&p, paint, &stroke_2, Transform::identity(), None);
            }
        }
        ButtonId::ToolLine => {
            // Straight diagonal line
            let mut pb = PathBuilder::new();
            pb.move_to(cx - size * 0.6, cy + size * 0.6);
            pb.line_to(cx + size * 0.6, cy - size * 0.6);
            if let Some(p) = pb.finish() {
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
            // Stroked rectangle
            if let Some(r) = tiny_skia::Rect::from_xywh(cx - size * 0.65, cy - size * 0.55, size * 1.3, size * 1.1) {
                let p = PathBuilder::from_rect(r);
                pixmap.stroke_path(&p, paint, &stroke_2, Transform::identity(), None);
            }
        }
        ButtonId::ToolRectFilled => {
            // Filled rectangle
            if let Some(r) = tiny_skia::Rect::from_xywh(cx - size * 0.65, cy - size * 0.55, size * 1.3, size * 1.1) {
                let p = PathBuilder::from_rect(r);
                pixmap.fill_path(&p, paint, tiny_skia::FillRule::Winding, Transform::identity(), None);
            }
        }
        ButtonId::ToolCircle => {
            // Stroked circle
            if let Some(r) = tiny_skia::Rect::from_xywh(cx - size * 0.65, cy - size * 0.65, size * 1.3, size * 1.3) {
                if let Some(p) = PathBuilder::from_oval(r) {
                    pixmap.stroke_path(&p, paint, &stroke_2, Transform::identity(), None);
                }
            }
        }
        ButtonId::ToolPencil => {
            // Pencil
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
            // Marker
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
            // Capital T
            let mut pb = PathBuilder::new();
            pb.move_to(cx - size * 0.65, cy - size * 0.6);
            pb.line_to(cx + size * 0.65, cy - size * 0.6);
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
        ButtonId::ToolBlur => {
            // Water droplet (blur)
            let mut pb = PathBuilder::new();
            pb.move_to(cx, cy - size * 0.75);
            pb.quad_to(cx + size * 0.65, cy + size * 0.1, cx, cy + size * 0.75);
            pb.quad_to(cx - size * 0.65, cy + size * 0.1, cx, cy - size * 0.75);
            pb.close();
            if let Some(p) = pb.finish() {
                pixmap.fill_path(&p, paint, tiny_skia::FillRule::Winding, Transform::identity(), None);
            }
        }
        ButtonId::ToolPixelate => {
            // 3x3 small grid
            let s = size * 0.35;
            let step = size * 0.45;
            for row in -1..=1 {
                for col in -1..=1 {
                    let rx = cx + col as f32 * step - s * 0.5;
                    let ry = cy + row as f32 * step - s * 0.5;
                    if let Some(r) = tiny_skia::Rect::from_xywh(rx, ry, s, s) {
                        let p = PathBuilder::from_rect(r);
                        pixmap.fill_path(&p, paint, tiny_skia::FillRule::Winding, Transform::identity(), None);
                    }
                }
            }
        }
        ButtonId::ToolInvert => {
            // Half-circle / Yin-yang
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
                pixmap.stroke_path(&p, paint, &stroke_2, Transform::identity(), None);
            }
        }
        ButtonId::ActionSave => {
            // Floppy disk
            let mut pb = PathBuilder::new();
            pb.move_to(cx - size * 0.65, cy - size * 0.65);
            pb.line_to(cx + size * 0.4, cy - size * 0.65);
            pb.line_to(cx + size * 0.65, cy - size * 0.4);
            pb.line_to(cx + size * 0.65, cy + size * 0.65);
            pb.line_to(cx - size * 0.65, cy + size * 0.65);
            pb.close();
            // Shutter
            pb.move_to(cx - size * 0.35, cy - size * 0.65);
            pb.line_to(cx + size * 0.2, cy - size * 0.65);
            pb.line_to(cx + size * 0.2, cy - size * 0.15);
            pb.line_to(cx - size * 0.35, cy - size * 0.15);
            pb.close();
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
