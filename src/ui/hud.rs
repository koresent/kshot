use crate::error::{KshotError, Result};
use crate::render::beautify::build_rounded_rect_path;
use crate::render::tools::{render_text, render_text_centered};
use crate::wayland::shm::{create_shm_buffer, copy_rgba_to_argb8888, ShmBuffer};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Stroke, Transform};
use wayland_client::protocol::{
    wl_buffer, wl_compositor, wl_keyboard, wl_pointer, wl_registry, wl_seat, wl_shm,
    wl_shm_pool, wl_surface,
};
use wayland_client::{Connection, Dispatch, QueueHandle, WEnum};
use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1, zwlr_layer_surface_v1};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HudAction {
    Area,
    Window,
    Screen,
    Scroll,
    Picker,
    Ruler,
    Scanner,
    Ocr,
}

#[derive(Debug, Clone)]
pub struct ModeSpec {
    pub action: HudAction,
    pub title: &'static str,
    pub key_hint: &'static str,
    pub key_code: u32,
    pub width: f32,
}

pub const SCREENSHOT_MODES: &[ModeSpec] = &[
    ModeSpec { action: HudAction::Area, title: "Area", key_hint: "1", key_code: 2, width: 104.0 },
    ModeSpec { action: HudAction::Window, title: "Window", key_hint: "2", key_code: 3, width: 118.0 },
    ModeSpec { action: HudAction::Screen, title: "Screen", key_hint: "3", key_code: 4, width: 118.0 },
    ModeSpec { action: HudAction::Scroll, title: "Scroll", key_hint: "4", key_code: 5, width: 118.0 },
];

pub const ALL_MODES: &[ModeSpec] = &[
    ModeSpec { action: HudAction::Area, title: "Area", key_hint: "1", key_code: 2, width: 104.0 },
    ModeSpec { action: HudAction::Window, title: "Window", key_hint: "2", key_code: 3, width: 118.0 },
    ModeSpec { action: HudAction::Screen, title: "Screen", key_hint: "3", key_code: 4, width: 118.0 },
    ModeSpec { action: HudAction::Scroll, title: "Scroll", key_hint: "4", key_code: 5, width: 118.0 },
    ModeSpec { action: HudAction::Picker, title: "Picker", key_hint: "A", key_code: 30, width: 118.0 },
    ModeSpec { action: HudAction::Ruler, title: "Ruler", key_hint: "S", key_code: 31, width: 112.0 },
    ModeSpec { action: HudAction::Scanner, title: "Scanner", key_hint: "D", key_code: 32, width: 126.0 },
    ModeSpec { action: HudAction::Ocr, title: "OCR", key_hint: "F", key_code: 33, width: 98.0 },
];

pub struct HudSession {
    pub modes: &'static [ModeSpec],
    pub pretty: bool,
    pub hovered_idx: Option<usize>,
    pub pressed_idx: Option<usize>,
    pub selected_action: Option<HudAction>,

    pub compositor: Option<wl_compositor::WlCompositor>,
    pub shm: Option<wl_shm::WlShm>,
    pub layer_shell: Option<zwlr_layer_shell_v1::ZwlrLayerShellV1>,
    pub seat: Option<wl_seat::WlSeat>,
    pub pointer: Option<wl_pointer::WlPointer>,
    pub keyboard: Option<wl_keyboard::WlKeyboard>,
    pub xkb_state: Option<xkbcommon::xkb::State>,

    pub surface: Option<wl_surface::WlSurface>,
    pub layer_surface: Option<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1>,
    pub configured: bool,

    pub width: u32,
    pub height: u32,
    pub shm_buffers: Option<[ShmBuffer; 2]>,
    pub current_buf_idx: usize,
    pub render_buffer: Pixmap,
    pub running: Arc<AtomicBool>,
}

impl HudSession {
    pub fn new(pretty: bool, running: Arc<AtomicBool>) -> Result<Self> {
        let modes = if pretty { SCREENSHOT_MODES } else { ALL_MODES };

        let gap = 6.0f32;
        let pad_x = 10.0f32;
        let mut inner_w = pad_x * 2.0;
        for (i, m) in modes.iter().enumerate() {
            inner_w += m.width;
            if i > 0 {
                inner_w += gap;
            }
        }

        let margin = 16.0f32;
        let total_w = (inner_w + margin * 2.0).ceil() as u32;
        let total_h = (56.0 + margin * 2.0) as u32;

        let render_buffer = Pixmap::new(total_w, total_h)
            .ok_or_else(|| KshotError::CaptureFailed("Failed to allocate HUD pixmap".into()))?;

        Ok(Self {
            modes,
            pretty,
            hovered_idx: None,
            pressed_idx: None,
            selected_action: None,
            compositor: None,
            shm: None,
            layer_shell: None,
            seat: None,
            pointer: None,
            keyboard: None,
            xkb_state: None,
            surface: None,
            layer_surface: None,
            configured: false,
            width: total_w,
            height: total_h,
            shm_buffers: None,
            current_buf_idx: 0,
            render_buffer,
            running,
        })
    }

    pub fn render(&mut self, qh: &QueueHandle<Self>) {
        if !self.configured || self.width == 0 || self.height == 0 {
            return;
        }

        let shm = match self.shm.as_ref() {
            Some(s) => s,
            None => return,
        };

        if self.shm_buffers.is_none() {
            let b0 = match create_shm_buffer(shm, qh, self.width, self.height) {
                Ok(b) => b,
                Err(_) => return,
            };
            let b1 = match create_shm_buffer(shm, qh, self.width, self.height) {
                Ok(b) => b,
                Err(_) => return,
            };
            self.shm_buffers = Some([b0, b1]);
        }

        // Clear buffer
        self.render_buffer.fill(Color::TRANSPARENT);

        let margin = 16.0f32;
        let pill_x = margin;
        let pill_y = margin;
        let pill_w = self.width as f32 - margin * 2.0;
        let pill_h = 56.0f32;
        let pill_radius = 14.0f32;

        // 1. Draw outer soft drop shadow
        let shadow_y = pill_y + 4.0;
        if let Some(shadow_path) = build_rounded_rect_path(pill_x, shadow_y, pill_w, pill_h, pill_radius) {
            let mut s_paint = Paint::default();
            s_paint.set_color(Color::from_rgba8(0, 0, 0, 75));
            s_paint.anti_alias = true;
            self.render_buffer.fill_path(
                &shadow_path,
                &s_paint,
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }

        // 2. Draw HUD pill background (#282828 Gruvbox Dark)
        if let Some(pill_path) = build_rounded_rect_path(pill_x, pill_y, pill_w, pill_h, pill_radius) {
            let mut bg_paint = Paint::default();
            bg_paint.set_color(Color::from_rgba8(40, 40, 40, 248)); // #282828
            bg_paint.anti_alias = true;
            self.render_buffer.fill_path(
                &pill_path,
                &bg_paint,
                FillRule::Winding,
                Transform::identity(),
                None,
            );

            // Border (#504945 Gruvbox Dark2)
            let mut border_paint = Paint::default();
            border_paint.set_color(Color::from_rgba8(80, 73, 69, 255));
            border_paint.anti_alias = true;
            let mut stroke = Stroke::default();
            stroke.width = 1.5;
            self.render_buffer.stroke_path(
                &pill_path,
                &border_paint,
                &stroke,
                Transform::identity(),
                None,
            );
        }

        // 3. Render Mode Buttons
        let pad_x = 10.0f32;
        let pad_y = 8.0f32;
        let gap = 6.0f32;
        let btn_h = 40.0f32;
        let mut cur_x = pill_x + pad_x;
        let btn_y = pill_y + pad_y;

        for (idx, mode) in self.modes.iter().enumerate() {
            let btn_w = mode.width;
            let is_hovered = self.hovered_idx == Some(idx);
            let is_pressed = self.pressed_idx == Some(idx);

            if let Some(btn_path) = build_rounded_rect_path(cur_x, btn_y, btn_w, btn_h, 10.0) {
                if is_pressed {
                    let mut p_paint = Paint::default();
                    p_paint.set_color(Color::from_rgba8(80, 73, 69, 255)); // #504945
                    p_paint.anti_alias = true;
                    self.render_buffer.fill_path(
                        &btn_path,
                        &p_paint,
                        FillRule::Winding,
                        Transform::identity(),
                        None,
                    );
                } else if is_hovered {
                    let mut h_paint = Paint::default();
                    h_paint.set_color(Color::from_rgba8(60, 56, 54, 255)); // #3c3836
                    h_paint.anti_alias = true;
                    self.render_buffer.fill_path(
                        &btn_path,
                        &h_paint,
                        FillRule::Winding,
                        Transform::identity(),
                        None,
                    );

                    let mut b_paint = Paint::default();
                    b_paint.set_color(Color::from_rgba8(80, 73, 69, 255));
                    b_paint.anti_alias = true;
                    let mut b_stroke = Stroke::default();
                    b_stroke.width = 1.0;
                    self.render_buffer.stroke_path(
                        &btn_path,
                        &b_paint,
                        &b_stroke,
                        Transform::identity(),
                        None,
                    );
                }
            }

            // Draw Icon
            let icon_x = cur_x + 9.0;
            let icon_y = btn_y + 11.0;
            self.draw_mode_icon(mode.action, icon_x, icon_y);

            // Draw Title Text
            let text_x = cur_x + 31.0;
            let text_y = btn_y + 12.0;
            render_text(
                &mut self.render_buffer,
                mode.title,
                text_x,
                text_y,
                12.0,
                (235, 219, 178, 255), // #ebdbb2
                None,
            );

            // Draw Key Hint Badge
            let badge_w = 20.0f32;
            let badge_h = 18.0f32;
            let badge_x = cur_x + btn_w - badge_w - 8.0;
            let badge_y = btn_y + 11.0;

            if let Some(b_path) = build_rounded_rect_path(badge_x, badge_y, badge_w, badge_h, 5.0) {
                let mut b_paint = Paint::default();
                b_paint.set_color(Color::from_rgba8(60, 56, 54, 255)); // #3c3836
                b_paint.anti_alias = true;
                self.render_buffer.fill_path(
                    &b_path,
                    &b_paint,
                    FillRule::Winding,
                    Transform::identity(),
                    None,
                );

                let mut stroke_paint = Paint::default();
                stroke_paint.set_color(Color::from_rgba8(80, 73, 69, 255));
                stroke_paint.anti_alias = true;
                let mut strk = Stroke::default();
                strk.width = 1.0;
                self.render_buffer.stroke_path(
                    &b_path,
                    &stroke_paint,
                    &strk,
                    Transform::identity(),
                    None,
                );

                // Badge text Gruvbox green #b8bb26 centered
                render_text_centered(
                    &mut self.render_buffer,
                    mode.key_hint,
                    badge_x + badge_w * 0.5,
                    badge_y + badge_h * 0.5,
                    10.5,
                    (184, 187, 38, 255), // #b8bb26
                    None,
                );
            }

            cur_x += btn_w + gap;
        }

        // 4. Draw Pretty Mode Dot Indicator if active
        if self.pretty {
            let dot_x = pill_x + pill_w - 9.0;
            let dot_y = pill_y + 9.0;
            let mut dot_pb = PathBuilder::new();
            dot_pb.push_circle(dot_x, dot_y, 3.5);
            if let Some(dot_path) = dot_pb.finish() {
                let mut dot_paint = Paint::default();
                dot_paint.set_color(Color::from_rgba8(184, 187, 38, 255)); // #b8bb26
                dot_paint.anti_alias = true;
                self.render_buffer.fill_path(
                    &dot_path,
                    &dot_paint,
                    FillRule::Winding,
                    Transform::identity(),
                    None,
                );
            }
        }

        // Copy render buffer to shm buffer and commit
        let back_idx = 1 - self.current_buf_idx;
        if let Some(buffers) = self.shm_buffers.as_mut() {
            let shm_buf = &mut buffers[back_idx];
            let total_pixels = (self.width * self.height) as usize;
            copy_rgba_to_argb8888(self.render_buffer.data(), shm_buf.mmap_ptr, total_pixels);

            if let Some(surface) = &self.surface {
                surface.attach(Some(&shm_buf.buffer), 0, 0);
                surface.damage(0, 0, self.width as i32, self.height as i32);
                surface.commit();
            }
            self.current_buf_idx = back_idx;
        }
    }

    fn draw_mode_icon(&mut self, action: HudAction, x: f32, y: f32) {
        let green = Color::from_rgba8(184, 187, 38, 255);
        let yellow = Color::from_rgba8(250, 189, 47, 255);
        let mut stroke_green = Stroke::default();
        stroke_green.width = 1.6;
        let mut paint_green = Paint::default();
        paint_green.set_color(green);
        paint_green.anti_alias = true;

        let mut paint_yellow = Paint::default();
        paint_yellow.set_color(yellow);
        paint_yellow.anti_alias = true;

        match action {
            HudAction::Area => {
                if let Some(p) = build_rounded_rect_path(x + 1.0, y + 1.0, 14.0, 14.0, 2.0) {
                    self.render_buffer.stroke_path(&p, &paint_green, &stroke_green, Transform::identity(), None);
                }
                let mut cross_pb = PathBuilder::new();
                cross_pb.move_to(x + 15.0, y + 11.0);
                cross_pb.line_to(x + 15.0, y + 17.0);
                cross_pb.move_to(x + 12.0, y + 14.0);
                cross_pb.line_to(x + 18.0, y + 14.0);
                if let Some(cp) = cross_pb.finish() {
                    self.render_buffer.stroke_path(&cp, &paint_yellow, &stroke_green, Transform::identity(), None);
                }
            }
            HudAction::Window => {
                if let Some(p) = build_rounded_rect_path(x, y + 1.0, 17.0, 14.0, 2.5) {
                    self.render_buffer.stroke_path(&p, &paint_green, &stroke_green, Transform::identity(), None);
                }
                let mut line_pb = PathBuilder::new();
                line_pb.move_to(x, y + 5.5);
                line_pb.line_to(x + 17.0, y + 5.5);
                if let Some(lp) = line_pb.finish() {
                    self.render_buffer.stroke_path(&lp, &paint_green, &stroke_green, Transform::identity(), None);
                }
                let mut dot_pb = PathBuilder::new();
                dot_pb.push_circle(x + 3.5, y + 3.5, 0.9);
                dot_pb.push_circle(x + 6.5, y + 3.5, 0.9);
                if let Some(dp) = dot_pb.finish() {
                    self.render_buffer.fill_path(&dp, &paint_yellow, FillRule::Winding, Transform::identity(), None);
                }
            }
            HudAction::Screen => {
                if let Some(p) = build_rounded_rect_path(x, y + 1.0, 17.0, 12.0, 2.0) {
                    self.render_buffer.stroke_path(&p, &paint_green, &stroke_green, Transform::identity(), None);
                }
                let mut stand_pb = PathBuilder::new();
                stand_pb.move_to(x + 8.5, y + 13.0);
                stand_pb.line_to(x + 8.5, y + 16.5);
                stand_pb.move_to(x + 4.5, y + 16.5);
                stand_pb.line_to(x + 12.5, y + 16.5);
                if let Some(sp) = stand_pb.finish() {
                    self.render_buffer.stroke_path(&sp, &paint_yellow, &stroke_green, Transform::identity(), None);
                }
            }
            HudAction::Scroll => {
                if let Some(p) = build_rounded_rect_path(x + 1.5, y, 14.0, 16.5, 2.0) {
                    self.render_buffer.stroke_path(&p, &paint_green, &stroke_green, Transform::identity(), None);
                }
                let mut arr_pb = PathBuilder::new();
                arr_pb.move_to(x + 8.5, y + 3.5);
                arr_pb.line_to(x + 8.5, y + 12.5);
                arr_pb.move_to(x + 5.5, y + 9.5);
                arr_pb.line_to(x + 8.5, y + 12.5);
                arr_pb.line_to(x + 11.5, y + 9.5);
                if let Some(ap) = arr_pb.finish() {
                    self.render_buffer.stroke_path(&ap, &paint_yellow, &stroke_green, Transform::identity(), None);
                }
            }
            HudAction::Picker => {
                let mut pip_pb = PathBuilder::new();
                pip_pb.move_to(x + 3.0, y + 15.0);
                pip_pb.line_to(x + 7.0, y + 11.0);
                pip_pb.line_to(x + 13.0, y + 5.0);
                pip_pb.cubic_to(x + 15.0, y + 3.0, x + 16.0, y + 4.0, x + 14.0, y + 6.0);
                pip_pb.line_to(x + 10.0, y + 10.0);
                pip_pb.line_to(x + 6.0, y + 14.0);
                pip_pb.close();
                if let Some(pp) = pip_pb.finish() {
                    self.render_buffer.stroke_path(&pp, &paint_green, &stroke_green, Transform::identity(), None);
                }
                let mut tip = PathBuilder::new();
                tip.push_circle(x + 2.5, y + 15.5, 1.0);
                if let Some(tp) = tip.finish() {
                    self.render_buffer.fill_path(&tp, &paint_yellow, FillRule::Winding, Transform::identity(), None);
                }
            }
            HudAction::Ruler => {
                let mut r_pb = PathBuilder::new();
                r_pb.move_to(x + 1.0, y + 14.0);
                r_pb.line_to(x + 14.0, y + 1.0);
                r_pb.line_to(x + 17.0, y + 4.0);
                r_pb.line_to(x + 4.0, y + 17.0);
                r_pb.close();
                // Tick marks
                r_pb.move_to(x + 5.0, y + 10.0);
                r_pb.line_to(x + 7.5, y + 12.5);
                r_pb.move_to(x + 8.0, y + 7.0);
                r_pb.line_to(x + 10.5, y + 9.5);
                r_pb.move_to(x + 11.0, y + 4.0);
                r_pb.line_to(x + 13.5, y + 6.5);
                if let Some(rp) = r_pb.finish() {
                    self.render_buffer.stroke_path(&rp, &paint_green, &stroke_green, Transform::identity(), None);
                }
            }
            HudAction::Scanner => {
                let mut qr_pb = PathBuilder::new();
                let push_box = |pb: &mut PathBuilder, bx: f32, by: f32, bw: f32, bh: f32| {
                    pb.move_to(bx, by);
                    pb.line_to(bx + bw, by);
                    pb.line_to(bx + bw, by + bh);
                    pb.line_to(bx, by + bh);
                    pb.close();
                };
                // Top-left square
                push_box(&mut qr_pb, x + 1.0, y + 1.0, 6.0, 6.0);
                // Top-right square
                push_box(&mut qr_pb, x + 10.0, y + 1.0, 6.0, 6.0);
                // Bottom-left square
                push_box(&mut qr_pb, x + 1.0, y + 10.0, 6.0, 6.0);
                // Center dot
                push_box(&mut qr_pb, x + 11.0, y + 11.0, 4.0, 4.0);
                if let Some(qp) = qr_pb.finish() {
                    self.render_buffer.stroke_path(&qp, &paint_green, &stroke_green, Transform::identity(), None);
                }
            }
            HudAction::Ocr => {
                let mut ocr_pb = PathBuilder::new();
                // Corner brackets
                ocr_pb.move_to(x + 5.0, y + 2.0);
                ocr_pb.line_to(x + 1.5, y + 2.0);
                ocr_pb.line_to(x + 1.5, y + 5.5);

                ocr_pb.move_to(x + 12.5, y + 2.0);
                ocr_pb.line_to(x + 16.0, y + 2.0);
                ocr_pb.line_to(x + 16.0, y + 5.5);

                ocr_pb.move_to(x + 1.5, y + 12.5);
                ocr_pb.line_to(x + 1.5, y + 16.0);
                ocr_pb.line_to(x + 5.0, y + 16.0);

                ocr_pb.move_to(x + 12.5, y + 16.0);
                ocr_pb.line_to(x + 16.0, y + 16.0);
                ocr_pb.line_to(x + 16.0, y + 12.5);
                if let Some(op) = ocr_pb.finish() {
                    self.render_buffer.stroke_path(&op, &paint_green, &stroke_green, Transform::identity(), None);
                }
                // Text 'T'
                let mut t_pb = PathBuilder::new();
                t_pb.move_to(x + 6.0, y + 5.5);
                t_pb.line_to(x + 12.0, y + 5.5);
                t_pb.move_to(x + 9.0, y + 5.5);
                t_pb.line_to(x + 9.0, y + 13.0);
                if let Some(tp) = t_pb.finish() {
                    self.render_buffer.stroke_path(&tp, &paint_yellow, &stroke_green, Transform::identity(), None);
                }
            }
        }
    }

    pub fn update_cursor(&mut self, cx: f32, cy: f32, qh: &QueueHandle<Self>) {
        let margin = 16.0f32;
        let pill_x = margin;
        let pill_y = margin;
        let pad_x = 10.0f32;
        let pad_y = 8.0f32;
        let gap = 6.0f32;
        let btn_h = 40.0f32;
        let btn_y = pill_y + pad_y;

        let mut hit = None;
        let mut cur_x = pill_x + pad_x;

        for (idx, mode) in self.modes.iter().enumerate() {
            let btn_w = mode.width;
            if cx >= cur_x && cx <= cur_x + btn_w && cy >= btn_y && cy <= btn_y + btn_h {
                hit = Some(idx);
                break;
            }
            cur_x += btn_w + gap;
        }

        if hit != self.hovered_idx {
            self.hovered_idx = hit;
            self.render(qh);
        }
    }
}

// Wayland dispatch implementations
impl Dispatch<wl_registry::WlRegistry, ()> for HudSession {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global { name, interface, version } = event {
            match interface.as_str() {
                "wl_compositor" => {
                    state.compositor = Some(registry.bind(name, version.min(6), qh, ()));
                }
                "wl_shm" => {
                    state.shm = Some(registry.bind(name, version.min(1), qh, ()));
                }
                "zwlr_layer_shell_v1" => {
                    state.layer_shell = Some(registry.bind(name, version.min(4), qh, ()));
                }
                "wl_seat" => {
                    state.seat = Some(registry.bind(name, version.min(8), qh, ()));
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<wl_compositor::WlCompositor, ()> for HudSession {
    fn event(
        _: &mut Self,
        _: &wl_compositor::WlCompositor,
        _: wl_compositor::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {}
}

impl Dispatch<wl_shm::WlShm, ()> for HudSession {
    fn event(
        _: &mut Self,
        _: &wl_shm::WlShm,
        _: wl_shm::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {}
}

impl Dispatch<wl_shm_pool::WlShmPool, ()> for HudSession {
    fn event(
        _: &mut Self,
        _: &wl_shm_pool::WlShmPool,
        _: wl_shm_pool::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {}
}

impl Dispatch<wl_buffer::WlBuffer, ()> for HudSession {
    fn event(
        _: &mut Self,
        _: &wl_buffer::WlBuffer,
        _: wl_buffer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {}
}

impl Dispatch<zwlr_layer_shell_v1::ZwlrLayerShellV1, ()> for HudSession {
    fn event(
        _: &mut Self,
        _: &zwlr_layer_shell_v1::ZwlrLayerShellV1,
        _: zwlr_layer_shell_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {}
}

impl Dispatch<wl_seat::WlSeat, ()> for HudSession {
    fn event(
        state: &mut Self,
        seat: &wl_seat::WlSeat,
        event: wl_seat::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_seat::Event::Capabilities { capabilities } = event {
            let caps = match capabilities {
                WEnum::Value(v) => v,
                WEnum::Unknown(_) => return,
            };
            if caps.contains(wl_seat::Capability::Pointer) && state.pointer.is_none() {
                state.pointer = Some(seat.get_pointer(qh, ()));
            }
            if caps.contains(wl_seat::Capability::Keyboard) && state.keyboard.is_none() {
                state.keyboard = Some(seat.get_keyboard(qh, ()));
            }
        }
    }
}

impl Dispatch<wl_pointer::WlPointer, ()> for HudSession {
    fn event(
        state: &mut Self,
        _: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_pointer::Event::Motion { surface_x, surface_y, .. } => {
                state.update_cursor(surface_x as f32, surface_y as f32, qh);
            }
            wl_pointer::Event::Button { button, state: WEnum::Value(btn_state), .. } => {
                if button == 0x110 {
                    if btn_state == wl_pointer::ButtonState::Pressed {
                        state.pressed_idx = state.hovered_idx;
                        state.render(qh);
                    } else {
                        if state.pressed_idx.is_some() && state.pressed_idx == state.hovered_idx {
                            if let Some(idx) = state.hovered_idx {
                                state.selected_action = Some(state.modes[idx].action);
                                state.running.store(false, Ordering::SeqCst);
                            }
                        }
                        state.pressed_idx = None;
                        state.render(qh);
                    }
                } else if button == 0x111 && btn_state == wl_pointer::ButtonState::Released {
                    // Right click cancels
                    state.selected_action = None;
                    state.running.store(false, Ordering::SeqCst);
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for HudSession {
    fn event(
        state: &mut Self,
        _: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_keyboard::Event::Keymap { format, fd, size } => {
                if format == WEnum::Value(wl_keyboard::KeymapFormat::XkbV1) {
                    let ctx = xkbcommon::xkb::Context::new(xkbcommon::xkb::CONTEXT_NO_FLAGS);
                    if let Ok(k) = unsafe {
                        xkbcommon::xkb::Keymap::new_from_fd(
                            &ctx,
                            fd,
                            size as usize,
                            xkbcommon::xkb::KEYMAP_FORMAT_TEXT_V1,
                            xkbcommon::xkb::KEYMAP_COMPILE_NO_FLAGS,
                        )
                    } {
                        if let Some(k) = k {
                            state.xkb_state = Some(xkbcommon::xkb::State::new(&k));
                        }
                    }
                }
            }
            wl_keyboard::Event::Key { key, state: WEnum::Value(key_state), .. } => {
                if key_state == wl_keyboard::KeyState::Released {
                    return;
                }
                // Escape key (1)
                if key == 1 {
                    state.selected_action = None;
                    state.running.store(false, Ordering::SeqCst);
                    return;
                }

                // Check direct keycode match
                for mode in state.modes {
                    if mode.key_code == key {
                        state.selected_action = Some(mode.action);
                        state.running.store(false, Ordering::SeqCst);
                        return;
                    }
                }

                // Fallback: check xkb keysym
                if let Some(ref xkb) = state.xkb_state {
                    let keycode = xkbcommon::xkb::Keycode::new(key + 8);
                    let sym = xkb.key_get_one_sym(keycode);
                    let ch = xkbcommon::xkb::keysym_to_utf8(sym);
                    let ch_lower = ch.to_lowercase();

                    for mode in state.modes {
                        if mode.key_hint.to_lowercase() == ch_lower {
                            state.selected_action = Some(mode.action);
                            state.running.store(false, Ordering::SeqCst);
                            return;
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_surface::WlSurface, ()> for HudSession {
    fn event(
        _: &mut Self,
        _: &wl_surface::WlSurface,
        _: wl_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {}
}

impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, ()> for HudSession {
    fn event(
        state: &mut Self,
        surface: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_layer_surface_v1::Event::Configure { serial, .. } => {
                surface.ack_configure(serial);
                state.configured = true;
                state.render(qh);
            }
            zwlr_layer_surface_v1::Event::Closed => {
                state.running.store(false, Ordering::SeqCst);
            }
            _ => {}
        }
    }
}

pub fn run_hud(pretty: bool) -> Result<Option<HudAction>> {
    let conn = Connection::connect_to_env()
        .map_err(|e| KshotError::WaylandConnection(format!("Cannot connect to Wayland display: {e}")))?;

    let running = Arc::new(AtomicBool::new(true));
    let mut session = HudSession::new(pretty, Arc::clone(&running))?;

    let mut event_queue = conn.new_event_queue();
    let qh = event_queue.handle();

    let display = conn.display();
    display.get_registry(&qh, ());

    event_queue
        .roundtrip(&mut session)
        .map_err(|e| KshotError::WaylandProtocol(format!("Initial roundtrip failed: {e}")))?;

    let compositor = session
        .compositor
        .clone()
        .ok_or_else(|| KshotError::WaylandProtocol("Missing wl_compositor".into()))?;
    let layer_shell = session
        .layer_shell
        .clone()
        .ok_or_else(|| KshotError::WaylandProtocol("Missing zwlr_layer_shell_v1".into()))?;

    let surface = compositor.create_surface(&qh, ());
    let layer_surface = layer_shell.get_layer_surface(
        &surface,
        None,
        zwlr_layer_shell_v1::Layer::Overlay,
        "dev.koresent.kshot.hud".to_string(),
        &qh,
        (),
    );

    layer_surface.set_size(session.width, session.height);
    layer_surface.set_exclusive_zone(-1);
    layer_surface.set_keyboard_interactivity(zwlr_layer_surface_v1::KeyboardInteractivity::Exclusive);
    // Center HUD popover on screen (no anchor means centered in layer shell)
    layer_surface.set_anchor(zwlr_layer_surface_v1::Anchor::empty());

    surface.commit();
    let _ = conn.flush();

    session.surface = Some(surface);
    session.layer_surface = Some(layer_surface);

    while running.load(Ordering::SeqCst) {
        event_queue
            .blocking_dispatch(&mut session)
            .map_err(|e| KshotError::WaylandProtocol(format!("Dispatch error: {e}")))?;
    }

    // Explicitly destroy surface before returning so screen unfreezes and HUD disappears
    drop(session.layer_surface);
    drop(session.surface);
    let _ = conn.flush();
    std::thread::sleep(std::time::Duration::from_millis(45));

    Ok(session.selected_action)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::tools::measure_text_width;

    #[test]
    fn test_hud_button_spacing_no_overlap() {
        for mode in ALL_MODES {
            let text_x = 31.0f32;
            let text_w = measure_text_width(mode.title, 12.0, None);
            let text_end = text_x + text_w;

            let badge_w = 20.0f32;
            let badge_x = mode.width - badge_w - 8.0;

            // Ensure there is at least 8.0px gap between title text and key hint badge
            assert!(
                badge_x >= text_end + 8.0,
                "Mode '{}' badge collides with text: text_end={}, badge_x={}, gap={}",
                mode.title,
                text_end,
                badge_x,
                badge_x - text_end
            );
        }
    }
}
