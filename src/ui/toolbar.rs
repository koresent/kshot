use crate::render::selection::SelectionRect;
use crate::render::tools::render_text_centered;
use crate::ui::button::{ButtonId, UiButton};
use tiny_skia::{Paint, PathBuilder, Pixmap, Stroke, Transform};

pub struct ToolbarManager {
    pub top_buttons: Vec<UiButton>,
    pub bottom_buttons: Vec<UiButton>,
    pub right_buttons: Vec<UiButton>,
    pub swatches: Vec<(u8, u8, u8)>,
    pub selected_swatch: usize,
    pub active_tool: ButtonId,
    pub thickness: f32,
    pub hovered_button: Option<ButtonId>,
    pub size_badge_pos: Option<(f32, f32)>,
}

impl ToolbarManager {
    pub fn new(swatches_hex: &[String], initial_thickness: f32) -> Self {
        let btn_radius = 16.0;

        // Top edge actions: Pin, Confirm, Cancel, Save
        let top_ids = [
            ButtonId::ActionPin,
            ButtonId::ActionConfirm,
            ButtonId::ActionCancel,
            ButtonId::ActionSave,
        ];

        // Bottom edge drawing tools: Pencil, Line, Arrow, Rect, RectFilled, Circle, Marker, Text, Counter
        let bottom_ids = [
            ButtonId::ToolPencil,
            ButtonId::ToolLine,
            ButtonId::ToolArrow,
            ButtonId::ToolRectangle,
            ButtonId::ToolRectFilled,
            ButtonId::ToolCircle,
            ButtonId::ToolMarker,
            ButtonId::ToolText,
            ButtonId::ToolCounter,
        ];

        // Right edge clipboard & history & redactions: Copy, Redo, Undo, Selection, Blur, Pixelate
        // (Size badge is placed in slot 4 between Selection and Blur)
        let right_ids = [
            ButtonId::ActionCopy,
            ButtonId::ActionRedo,
            ButtonId::ActionUndo,
            ButtonId::ToolSelection,
            ButtonId::ToolBlur,
            ButtonId::ToolPixelate,
        ];

        let top_buttons = top_ids.iter().map(|&id| UiButton::new(id, btn_radius)).collect();
        let bottom_buttons = bottom_ids.iter().map(|&id| UiButton::new(id, btn_radius)).collect();
        let right_buttons = right_ids.iter().map(|&id| UiButton::new(id, btn_radius)).collect();

        let swatches = swatches_hex
            .iter()
            .filter_map(|hex| {
                let s = hex.trim().trim_start_matches('#');
                if s.len() == 6 {
                    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
                    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
                    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
                    Some((r, g, b))
                } else {
                    None
                }
            })
            .collect();

        let mut mgr = Self {
            top_buttons,
            bottom_buttons,
            right_buttons,
            swatches,
            selected_swatch: 0,
            active_tool: ButtonId::ToolSelection,
            thickness: initial_thickness,
            hovered_button: None,
            size_badge_pos: None,
        };
        mgr.set_active_tool(ButtonId::ToolSelection);
        mgr
    }

    pub fn all_buttons(&self) -> impl Iterator<Item = &UiButton> {
        self.top_buttons
            .iter()
            .chain(self.bottom_buttons.iter())
            .chain(self.right_buttons.iter())
    }

    pub fn all_buttons_mut(&mut self) -> impl Iterator<Item = &mut UiButton> {
        self.top_buttons
            .iter_mut()
            .chain(self.bottom_buttons.iter_mut())
            .chain(self.right_buttons.iter_mut())
    }

    pub fn active_color(&self) -> (u8, u8, u8, u8) {
        if let Some(&(r, g, b)) = self.swatches.get(self.selected_swatch) {
            (r, g, b, 255)
        } else {
            (184, 187, 38, 255) // Default Gruvbox green #b8bb26
        }
    }

    pub fn set_active_tool(&mut self, tool: ButtonId) {
        self.active_tool = tool;
        for btn in self.all_buttons_mut() {
            btn.is_active = btn.id == tool;
        }
    }

    pub fn update_layout(
        &mut self,
        selection: Option<&SelectionRect>,
        screen_w: f32,
        screen_h: f32,
    ) {
        let sel = match selection {
            Some(s) if !s.is_empty() => s,
            _ => {
                self.size_badge_pos = None;
                return;
            }
        };

        let r = 16.0;
        let spacing = 38.0;
        let margin = 10.0;
        let pad = 8.0;

        // 1. TOP BAR (Pin, Confirm, Cancel, Save)
        let top_count = self.top_buttons.len() as f32;
        let top_w = (top_count - 1.0) * spacing;
        let mut top_x = sel.x + (sel.w - top_w) * 0.5;
        top_x = top_x.clamp(pad + r, screen_w - pad - r - top_w);

        let mut top_y = sel.y - margin - r;
        if top_y - r < pad {
            // Flip inside selection
            top_y = sel.y + margin + r;
        }

        for (i, btn) in self.top_buttons.iter_mut().enumerate() {
            btn.center_x = top_x + i as f32 * spacing;
            btn.center_y = top_y;
        }

        // 2. BOTTOM BAR (Pencil, Line, Arrow, Rect, RectFilled, Circle, Marker, Text, Counter)
        let bot_count = self.bottom_buttons.len() as f32;
        let bot_w = (bot_count - 1.0) * spacing;
        let mut bot_x = sel.x + (sel.w - bot_w) * 0.5;
        bot_x = bot_x.clamp(pad + r, screen_w - pad - r - bot_w);

        let mut bot_y = sel.y + sel.h + margin + r;
        if bot_y + r > screen_h - pad {
            // Flip inside selection
            bot_y = sel.y + sel.h - margin - r;
        }

        for (i, btn) in self.bottom_buttons.iter_mut().enumerate() {
            btn.center_x = bot_x + i as f32 * spacing;
            btn.center_y = bot_y;
        }

        // 3. RIGHT BAR (Copy, Redo, Undo, Move, SizeBadge, Blur, Pixelate)
        let total_right_slots = 7.0;
        let right_h = (total_right_slots - 1.0) * spacing;
        let mut right_y = sel.y + (sel.h - right_h) * 0.5;
        right_y = right_y.clamp(pad + r, screen_h - pad - r - right_h);

        let mut right_x = sel.x + sel.w + margin + r;
        if right_x + r > screen_w - pad {
            // Flip inside selection
            right_x = sel.x + sel.w - margin - r;
        }

        // Slot 0: Copy
        self.right_buttons[0].center_x = right_x;
        self.right_buttons[0].center_y = right_y + 0.0 * spacing;

        // Slot 1: Redo
        self.right_buttons[1].center_x = right_x;
        self.right_buttons[1].center_y = right_y + 1.0 * spacing;

        // Slot 2: Undo
        self.right_buttons[2].center_x = right_x;
        self.right_buttons[2].center_y = right_y + 2.0 * spacing;

        // Slot 3: Selection (Move)
        self.right_buttons[3].center_x = right_x;
        self.right_buttons[3].center_y = right_y + 3.0 * spacing;

        // Slot 4: Size Badge
        self.size_badge_pos = Some((right_x, right_y + 4.0 * spacing));

        // Slot 5: Blur
        self.right_buttons[4].center_x = right_x;
        self.right_buttons[4].center_y = right_y + 5.0 * spacing;

        // Slot 6: Pixelate
        self.right_buttons[5].center_x = right_x;
        self.right_buttons[5].center_y = right_y + 6.0 * spacing;
    }

    pub fn handle_mouse_move(&mut self, px: f32, py: f32) -> Option<&'static str> {
        let mut found_hover = None;
        let mut tooltip = None;

        for btn in self.all_buttons_mut() {
            let hit = btn.contains(px, py);
            btn.is_hovered = hit;
            if hit {
                found_hover = Some(btn.id);
                tooltip = Some(btn.id.tooltip());
            }
        }

        self.hovered_button = found_hover;
        tooltip
    }

    pub fn handle_click(&mut self, px: f32, py: f32) -> Option<ButtonId> {
        for btn in self.all_buttons() {
            if btn.contains(px, py) {
                return Some(btn.id);
            }
        }
        None
    }

    pub fn render(
        &self,
        pixmap: &mut Pixmap,
        selection: Option<&SelectionRect>,
        accent_color: (u8, u8, u8),
    ) {
        let sel = match selection {
            Some(s) if !s.is_empty() => s,
            _ => return, // No selection: do not draw any banner or buttons
        };

        // Render all buttons
        for btn in self.all_buttons() {
            btn.render(pixmap, accent_color);
        }

        // Render Size Badge on the right bar
        if let Some((bx, by)) = self.size_badge_pos {
            let r = 16.0;
            if let Some(cr) = tiny_skia::Rect::from_xywh(bx - r, by - r, r * 2.0, r * 2.0) {
                if let Some(p) = PathBuilder::from_oval(cr) {
                    // Badge background: Gruvbox bg0 (#282828)
                    let mut bg = Paint::default();
                    bg.set_color_rgba8(40, 40, 40, 240);
                    bg.anti_alias = true;
                    pixmap.fill_path(&p, &bg, tiny_skia::FillRule::Winding, Transform::identity(), None);

                    // Badge border: Gruvbox dark2 (#504945)
                    let mut bp = Paint::default();
                    bp.set_color_rgba8(80, 73, 69, 230);
                    bp.anti_alias = true;
                    let stroke = Stroke { width: 1.0, ..Default::default() };
                    pixmap.stroke_path(&p, &bp, &stroke, Transform::identity(), None);

                    // Render width on top line, height on bottom line in Gruvbox Green #b8bb26
                    let w_str = format!("{}", sel.w.round() as i32);
                    let h_str = format!("{}", sel.h.round() as i32);
                    let text_color = (184, 187, 38, 255); // #b8bb26
                    render_text_centered(pixmap, &w_str, bx, by - 4.5, 9.5, text_color, None);
                    render_text_centered(pixmap, &h_str, bx, by + 5.5, 9.5, text_color, None);
                }
            }
        }

        // Render Tooltip if hovered
        if let Some(btn_id) = self.hovered_button {
            for btn in self.all_buttons() {
                if btn.id == btn_id {
                    let tip = btn_id.tooltip();
                    let tip_w = tip.len() as f32 * 6.8 + 16.0;
                    let tip_h = 22.0;

                    let (tip_x, tip_y) = if self.bottom_buttons.iter().any(|b| b.id == btn_id) {
                        // Bottom bar: show above
                        ((btn.center_x - tip_w * 0.5).clamp(8.0, pixmap.width() as f32 - tip_w - 8.0),
                         btn.center_y - btn.radius - tip_h - 6.0)
                    } else if self.top_buttons.iter().any(|b| b.id == btn_id) {
                        // Top bar: show below
                        ((btn.center_x - tip_w * 0.5).clamp(8.0, pixmap.width() as f32 - tip_w - 8.0),
                         btn.center_y + btn.radius + 6.0)
                    } else {
                        // Right bar: show to the left
                        (btn.center_x - btn.radius - tip_w - 6.0,
                         btn.center_y - tip_h * 0.5)
                    };

                    if let Some(path) = rounded_rect_path(tip_x, tip_y, tip_w, tip_h, 6.0) {
                        let mut tp = Paint::default();
                        tp.set_color_rgba8(29, 32, 33, 245); // Gruvbox #1d2021
                        tp.anti_alias = true;
                        pixmap.fill_path(&path, &tp, tiny_skia::FillRule::Winding, Transform::identity(), None);

                        let mut bp = Paint::default();
                        bp.set_color_rgba8(80, 73, 69, 230); // Gruvbox #504945
                        bp.anti_alias = true;
                        let s = Stroke { width: 1.0, ..Default::default() };
                        pixmap.stroke_path(&path, &bp, &s, Transform::identity(), None);

                        render_text_centered(
                            pixmap,
                            tip,
                            tip_x + tip_w * 0.5,
                            tip_y + tip_h * 0.5,
                            10.5,
                            (235, 219, 178, 255), // #ebdbb2
                            None,
                        );
                    }
                    break;
                }
            }
        }
    }
}

pub fn rounded_rect_path(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<tiny_skia::Path> {
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.quad_to(x + w, y, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.quad_to(x + w, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.quad_to(x, y + h, x, y + h - r);
    pb.line_to(x, y + r);
    pb.quad_to(x, y, x + r, y);
    pb.close();
    pb.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_toolbar_layout_and_hit_testing() {
        let swatches = vec!["#b8bb26".to_string(), "#fabd2f".to_string()];
        let mut mgr = ToolbarManager::new(&swatches, 3.0);
        assert_eq!(mgr.top_buttons.len(), 4);
        assert_eq!(mgr.bottom_buttons.len(), 9);
        assert_eq!(mgr.right_buttons.len(), 6);

        let sel = SelectionRect::new(200.0, 200.0, 400.0, 300.0);
        mgr.update_layout(Some(&sel), 1920.0, 1080.0);

        assert!(mgr.size_badge_pos.is_some());
        let (bx, by) = mgr.size_badge_pos.unwrap();
        assert!(bx > 600.0); // Placed along the right edge of selection
        assert!(by >= 200.0 && by <= 500.0);

        // Top buttons should be above sel.y
        for btn in &mgr.top_buttons {
            assert!(btn.center_y < 200.0);
        }

        // Bottom buttons should be below sel.y + sel.h
        for btn in &mgr.bottom_buttons {
            assert!(btn.center_y > 500.0);
        }

        // Test button hover and hit
        let top_btn_pos = (mgr.top_buttons[0].center_x, mgr.top_buttons[0].center_y);
        let tip = mgr.handle_mouse_move(top_btn_pos.0, top_btn_pos.1);
        assert!(tip.is_some());
        assert_eq!(mgr.handle_click(top_btn_pos.0, top_btn_pos.1), Some(ButtonId::ActionPin));
    }
}
