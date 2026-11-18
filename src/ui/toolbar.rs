use crate::render::selection::SelectionRect;
use crate::render::tools::{render_text, render_text_centered};
use crate::ui::button::{ButtonId, UiButton};
use tiny_skia::{Paint, PathBuilder, Pixmap, Stroke, Transform};

pub struct ToolbarManager {
    pub tool_buttons: Vec<UiButton>,
    pub action_buttons: Vec<UiButton>,
    pub swatches: Vec<(u8, u8, u8)>,
    pub selected_swatch: usize,
    pub active_tool: ButtonId,
    pub thickness: f32,
    pub hovered_button: Option<ButtonId>,
}

impl ToolbarManager {
    pub fn new(swatches_hex: &[String], initial_thickness: f32) -> Self {
        let tool_ids = [
            ButtonId::ToolSelection,
            ButtonId::ToolArrow,
            ButtonId::ToolRectangle,
            ButtonId::ToolCircle,
            ButtonId::ToolMarker,
            ButtonId::ToolPencil,
            ButtonId::ToolText,
            ButtonId::ToolCounter,
            ButtonId::ToolPixelate,
            ButtonId::ToolInvert,
        ];

        let action_ids = [
            ButtonId::ActionUndo,
            ButtonId::ActionRedo,
            ButtonId::ActionCopy,
            ButtonId::ActionSave,
            ButtonId::ActionPin,
            ButtonId::ActionCancel,
            ButtonId::ActionConfirm,
        ];

        let btn_radius = 16.0;

        let tool_buttons = tool_ids.iter().map(|&id| UiButton::new(id, btn_radius)).collect();
        let action_buttons = action_ids.iter().map(|&id| UiButton::new(id, btn_radius)).collect();

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

        Self {
            tool_buttons,
            action_buttons,
            swatches,
            selected_swatch: 0,
            active_tool: ButtonId::ToolSelection,
            thickness: initial_thickness,
            hovered_button: None,
        }
    }

    pub fn active_color(&self) -> (u8, u8, u8, u8) {
        if let Some(&(r, g, b)) = self.swatches.get(self.selected_swatch) {
            (r, g, b, 255)
        } else {
            (204, 36, 29, 255) // Default red
        }
    }

    pub fn set_active_tool(&mut self, tool: ButtonId) {
        self.active_tool = tool;
        for btn in &mut self.tool_buttons {
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
            _ => return,
        };

        let btn_spacing = 38.0;
        let pad = 12.0;

        // 1. Layout Action Toolbar (bottom row)
        let actions_count = self.action_buttons.len() as f32;
        let actions_total_w = actions_count * btn_spacing;
        let mut actions_x = sel.x + sel.w - actions_total_w;
        if actions_x < pad {
            actions_x = pad;
        }
        if actions_x + actions_total_w > screen_w - pad {
            actions_x = screen_w - pad - actions_total_w;
        }

        let mut actions_y = sel.y + sel.h + 24.0;
        // Flip to inside or top if near bottom
        if actions_y + 40.0 > screen_h {
            actions_y = sel.y - 24.0;
            if actions_y < 40.0 {
                actions_y = sel.y + sel.h - 24.0;
            }
        }

        for (i, btn) in self.action_buttons.iter_mut().enumerate() {
            btn.center_x = actions_x + (i as f32 + 0.5) * btn_spacing;
            btn.center_y = actions_y;
        }

        // 2. Layout Tool Toolbar (top row or opposite side)
        let tools_count = self.tool_buttons.len() as f32;
        let tools_total_w = tools_count * btn_spacing;
        let mut tools_x = sel.x;
        if tools_x < pad {
            tools_x = pad;
        }
        if tools_x + tools_total_w > screen_w - pad {
            tools_x = screen_w - pad - tools_total_w;
        }

        let mut tools_y = sel.y - 24.0;
        if tools_y < 24.0 {
            tools_y = sel.y + sel.h + 24.0;
            if tools_y + 40.0 > screen_h {
                tools_y = sel.y + 24.0;
            }
        }

        for (i, btn) in self.tool_buttons.iter_mut().enumerate() {
            btn.center_x = tools_x + (i as f32 + 0.5) * btn_spacing;
            btn.center_y = tools_y;
        }
    }

    pub fn handle_mouse_move(&mut self, px: f32, py: f32) -> Option<&'static str> {
        self.hovered_button = None;
        let mut tooltip = None;

        for btn in self.tool_buttons.iter_mut().chain(self.action_buttons.iter_mut()) {
            let hit = btn.contains(px, py);
            btn.is_hovered = hit;
            if hit {
                self.hovered_button = Some(btn.id);
                tooltip = Some(btn.id.tooltip());
            }
        }

        tooltip
    }

    pub fn handle_click(&mut self, px: f32, py: f32) -> Option<ButtonId> {
        for btn in self.tool_buttons.iter().chain(self.action_buttons.iter()) {
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
        if selection.is_none() || selection.unwrap().is_empty() {
            // Draw top hint banner with rounded corners
            let banner_w = 560.0;
            let banner_h = 36.0;
            let bx = (pixmap.width() as f32 - banner_w) * 0.5;
            let by = 24.0;
            let radius = 8.0;

            if let Some(path) = rounded_rect_path(bx, by, banner_w, banner_h, radius) {
                let mut paint = Paint::default();
                paint.set_color_rgba8(40, 40, 40, 235); // Gruvbox bg0_h
                paint.anti_alias = true;
                pixmap.fill_path(&path, &paint, tiny_skia::FillRule::Winding, Transform::identity(), None);

                let mut bp = Paint::default();
                bp.set_color_rgba8(254, 128, 25, 220); // Gruvbox orange
                bp.anti_alias = true;
                let stroke = Stroke { width: 1.5, ..Default::default() };
                pixmap.stroke_path(&path, &bp, &stroke, Transform::identity(), None);

                let cx = bx + banner_w * 0.5;
                let cy = by + banner_h * 0.5;
                render_text_centered(
                    pixmap,
                    "Select area • Double-click full screen • Esc / Right-click to exit",
                    cx,
                    cy,
                    12.0,
                    (235, 219, 178, 255),
                    None,
                );
            }
            return;
        }

        // Render all buttons
        for btn in &self.tool_buttons {
            btn.render(pixmap, accent_color);
        }
        for btn in &self.action_buttons {
            btn.render(pixmap, accent_color);
        }

        // Render tooltip if hovered
        if let Some(btn_id) = self.hovered_button {
            for btn in self.tool_buttons.iter().chain(self.action_buttons.iter()) {
                if btn.id == btn_id {
                    let tip = btn_id.tooltip();
                    let tip_w = tip.len() as f32 * 7.5 + 16.0;
                    let tip_h = 24.0;
                    let tip_x = (btn.center_x - tip_w * 0.5).clamp(10.0, pixmap.width() as f32 - tip_w - 10.0);
                    let tip_y = if btn.center_y > 40.0 { btn.center_y - 34.0 } else { btn.center_y + 24.0 };

                    if let Some(r) = tiny_skia::Rect::from_xywh(tip_x, tip_y, tip_w, tip_h) {
                        let p = PathBuilder::from_rect(r);
                        let mut tp = Paint::default();
                        tp.set_color_rgba8(29, 32, 33, 240); // Darker Gruvbox
                        pixmap.fill_path(&p, &tp, tiny_skia::FillRule::Winding, Transform::identity(), None);

                        let mut bp = Paint::default();
                        bp.set_color_rgba8(250, 189, 47, 220); // Yellow border
                        let s = Stroke { width: 1.0, ..Default::default() };
                        pixmap.stroke_path(&p, &bp, &s, Transform::identity(), None);

                        render_text(pixmap, tip, tip_x + 8.0, tip_y + 16.0, 11.0, (235, 219, 178, 255), None);
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
