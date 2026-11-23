use crate::capture::CaptureFrame;
use crate::error::{KshotError, Result};
use crate::render::beautify::build_rounded_rect_path;
use crate::render::tools::render_text;
use crate::wayland::shm::{create_shm_buffer, copy_rgba_to_argb8888, ShmBuffer};
use serde::Deserialize;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tiny_skia::{Color, FillRule, Paint, Pixmap, Stroke, Transform};
use wayland_client::protocol::{
    wl_buffer, wl_compositor, wl_keyboard, wl_output, wl_pointer, wl_registry, wl_seat, wl_shm,
    wl_shm_pool, wl_surface,
};
use wayland_client::{Connection, Dispatch, QueueHandle, WEnum};
use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1, zwlr_layer_surface_v1};

#[derive(Debug, Clone, Deserialize)]
pub struct UmbrielPoint {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UmbrielSize {
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct UmbrielOutput {
    pub name: Option<String>,
    pub focused: Option<bool>,
    pub enabled: Option<bool>,
    pub position: Option<UmbrielPoint>,
    pub logical_size: Option<UmbrielSize>,
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub w: Option<i32>,
    pub h: Option<i32>,
    pub scale: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct UmbrielWindow {
    pub id: Option<String>,
    pub app_id: Option<String>,
    pub title: Option<String>,
    pub floating: Option<bool>,
    pub visible: Option<bool>,
    pub focused: Option<bool>,
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub w: Option<i32>,
    pub h: Option<i32>,
    pub output: Option<String>,
}

#[derive(Debug, Clone)]
pub struct WindowBox {
    pub app_id: String,
    pub title: String,
    pub local_x: f32,
    pub local_y: f32,
    pub local_w: f32,
    pub local_h: f32,
    pub floating: bool,
}

pub fn query_umbriel(cmd: &str) -> Option<serde_json::Value> {
    let uid = unsafe { libc::getuid() };
    let primary = format!("/run/user/{uid}/umbriel-wayland-0.sock");
    let sock_path = if std::path::Path::new(&primary).exists() {
        Some(PathBuf::from(primary))
    } else {
        let user_run = format!("/run/user/{uid}");
        fs::read_dir(user_run).ok()?.flatten().map(|e| e.path()).find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.starts_with("umbriel-") && s.ends_with(".sock"))
                .unwrap_or(false)
        })
    }?;

    let mut stream = UnixStream::connect(sock_path).ok()?;
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_millis(400)));
    let _ = stream.set_write_timeout(Some(std::time::Duration::from_millis(400)));

    stream.write_all(cmd.as_bytes()).ok()?;
    if !cmd.ends_with('\n') {
        stream.write_all(b"\n").ok()?;
    }

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    serde_json::from_str(&line).ok()
}

pub fn filter_windows_for_output(
    windows: &[UmbrielWindow],
    out: &UmbrielOutput,
) -> Vec<WindowBox> {
    let out_x = out.position.as_ref().map(|p| p.x).or(out.x).unwrap_or(0);
    let out_y = out.position.as_ref().map(|p| p.y).or(out.y).unwrap_or(0);
    let out_w = out.logical_size.as_ref().map(|s| s.width).or(out.w).unwrap_or(1920);
    let out_h = out.logical_size.as_ref().map(|s| s.height).or(out.h).unwrap_or(1080);
    let out_name = out.name.as_deref().unwrap_or("");

    let mut result = Vec::new();
    for win in windows {
        if !win.visible.unwrap_or(false) {
            continue;
        }
        let wx = win.x.unwrap_or(0);
        let wy = win.y.unwrap_or(0);
        let ww = win.w.unwrap_or(0);
        let wh = win.h.unwrap_or(0);

        if ww <= 10 || wh <= 10 {
            continue;
        }

        let overlaps = if let Some(ref o) = win.output {
            o == out_name
        } else {
            wx < out_x + out_w && wx + ww > out_x && wy < out_y + out_h && wy + wh > out_y
        };

        if overlaps {
            let local_x = (wx - out_x).max(0) as f32;
            let local_y = (wy - out_y).max(0) as f32;
            let local_w = (ww as f32).min(out_w as f32 - local_x);
            let local_h = (wh as f32).min(out_h as f32 - local_y);

            result.push(WindowBox {
                app_id: win.app_id.clone().unwrap_or_else(|| "Window".to_string()),
                title: win.title.clone().unwrap_or_default(),
                local_x,
                local_y,
                local_w,
                local_h,
                floating: win.floating.unwrap_or(false),
            });
        }
    }

    // Floating windows take priority over tiled
    result.sort_by(|a, b| b.floating.cmp(&a.floating));
    result
}

pub struct WindowSession {
    pub frame: CaptureFrame,
    pub dimmed_pixmap: Pixmap,
    pub windows: Vec<WindowBox>,
    pub hovered_idx: Option<usize>,
    pub selected_window: Option<WindowBox>,

    pub compositor: Option<wl_compositor::WlCompositor>,
    pub shm: Option<wl_shm::WlShm>,
    pub layer_shell: Option<zwlr_layer_shell_v1::ZwlrLayerShellV1>,
    pub seat: Option<wl_seat::WlSeat>,
    pub pointer: Option<wl_pointer::WlPointer>,
    pub keyboard: Option<wl_keyboard::WlKeyboard>,
    pub outputs: Vec<(wl_output::WlOutput, Option<String>)>,
    #[allow(dead_code)]
    pub target_output_name: Option<String>,

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

impl WindowSession {
    pub fn new(
        frame: CaptureFrame,
        target_output_name: Option<String>,
        windows: Vec<WindowBox>,
        running: Arc<AtomicBool>,
    ) -> Result<Self> {
        let w = frame.width;
        let h = frame.height;

        let mut dimmed = Pixmap::new(w, h)
            .ok_or_else(|| KshotError::CaptureFailed("Failed to allocate dimmed pixmap".into()))?;

        // Fast dimming pass (scale RGB by 190/255)
        let src_data = frame.pixmap.data();
        let dst_data = dimmed.data_mut();
        for i in (0..src_data.len()).step_by(4) {
            dst_data[i] = ((src_data[i] as u16 * 190) >> 8) as u8;
            dst_data[i + 1] = ((src_data[i + 1] as u16 * 190) >> 8) as u8;
            dst_data[i + 2] = ((src_data[i + 2] as u16 * 190) >> 8) as u8;
            dst_data[i + 3] = 255;
        }

        let render_buffer = Pixmap::new(w, h)
            .ok_or_else(|| KshotError::CaptureFailed("Failed to allocate render buffer".into()))?;

        Ok(Self {
            frame,
            dimmed_pixmap: dimmed,
            windows,
            hovered_idx: None,
            selected_window: None,
            compositor: None,
            shm: None,
            layer_shell: None,
            seat: None,
            pointer: None,
            keyboard: None,
            outputs: Vec::new(),
            target_output_name,
            surface: None,
            layer_surface: None,
            configured: false,
            width: w,
            height: h,
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
                Err(e) => {
                    eprintln!("Failed to create shm buffer: {e}");
                    return;
                }
            };
            let b1 = match create_shm_buffer(shm, qh, self.width, self.height) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("Failed to create shm buffer: {e}");
                    return;
                }
            };
            self.shm_buffers = Some([b0, b1]);
        }

        // 1. Copy dimmed frame to render buffer
        self.render_buffer
            .data_mut()
            .copy_from_slice(self.dimmed_pixmap.data());

        // 2. If a window is hovered, illuminate it and draw highlight
        if let Some(idx) = self.hovered_idx {
            if let Some(win) = self.windows.get(idx) {
                let wx = win.local_x;
                let wy = win.local_y;
                let ww = win.local_w;
                let wh = win.local_h;

                let r = 8.0f32;
                if let Some(path) = build_rounded_rect_path(wx, wy, ww, wh, r) {
                    // Copy original un-dimmed pixels
                    let x_start = (wx.max(0.0) as u32).min(self.width);
                    let y_start = (wy.max(0.0) as u32).min(self.height);
                    let x_end = ((wx + ww).min(self.width as f32) as u32).max(x_start);
                    let y_end = ((wy + wh).min(self.height as f32) as u32).max(y_start);

                    let src_data = self.frame.pixmap.data();
                    let dst_data = self.render_buffer.data_mut();

                    for y in y_start..y_end {
                        let row_offset = (y * self.width) as usize;
                        let src_start = (row_offset + x_start as usize) * 4;
                        let src_end = (row_offset + x_end as usize) * 4;
                        dst_data[src_start..src_end].copy_from_slice(&src_data[src_start..src_end]);
                    }

                    // Semi-transparent Gruvbox Green overlay
                    let mut tint_paint = Paint::default();
                    tint_paint.set_color(Color::from_rgba8(184, 187, 38, 38)); // 15% #b8bb26
                    tint_paint.anti_alias = true;
                    self.render_buffer.fill_path(
                        &path,
                        &tint_paint,
                        FillRule::Winding,
                        Transform::identity(),
                        None,
                    );

                    // Gruvbox Green border
                    let mut stroke_paint = Paint::default();
                    stroke_paint.set_color(Color::from_rgba8(184, 187, 38, 255));
                    stroke_paint.anti_alias = true;
                    let mut stroke = Stroke::default();
                    stroke.width = 2.0;

                    self.render_buffer.stroke_path(
                        &path,
                        &stroke_paint,
                        &stroke,
                        Transform::identity(),
                        None,
                    );

                    // Draw window info badge
                    let badge_text = if win.title.is_empty() {
                        win.app_id.clone()
                    } else if win.title.chars().count() > 32 {
                        format!("{} — {}…", win.app_id, win.title.chars().take(28).collect::<String>())
                    } else {
                        format!("{} — {}", win.app_id, win.title)
                    };

                    let badge_h = 24.0f32;
                    let badge_w = (badge_text.chars().count() as f32 * 7.5 + 20.0).max(80.0);
                    let badge_x = (wx + 10.0).min(self.width as f32 - badge_w - 10.0).max(10.0);
                    let badge_y = (wy + 10.0).min(self.height as f32 - badge_h - 10.0).max(10.0);

                    if let Some(badge_path) = build_rounded_rect_path(badge_x, badge_y, badge_w, badge_h, 6.0) {
                        let mut bg_paint = Paint::default();
                        bg_paint.set_color(Color::from_rgba8(40, 40, 40, 235)); // Gruvbox Dark #282828
                        bg_paint.anti_alias = true;
                        self.render_buffer.fill_path(
                            &badge_path,
                            &bg_paint,
                            FillRule::Winding,
                            Transform::identity(),
                            None,
                        );

                        let mut border_paint = Paint::default();
                        border_paint.set_color(Color::from_rgba8(80, 73, 69, 255)); // #504945
                        border_paint.anti_alias = true;
                        let mut b_stroke = Stroke::default();
                        b_stroke.width = 1.0;
                        self.render_buffer.stroke_path(
                            &badge_path,
                            &border_paint,
                            &b_stroke,
                            Transform::identity(),
                            None,
                        );

                        render_text(
                            &mut self.render_buffer,
                            &badge_text,
                            badge_x + 8.0,
                            badge_y + 4.0,
                            11.0,
                            (235, 219, 178, 255), // #ebdbb2
                            None,
                        );
                    }
                }
            }
        }

        // Copy render buffer to shm buffer
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

    pub fn update_cursor(&mut self, x: f32, y: f32, qh: &QueueHandle<Self>) {
        let new_idx = self.windows.iter().position(|w| {
            x >= w.local_x && x <= w.local_x + w.local_w && y >= w.local_y && y <= w.local_y + w.local_h
        });

        if new_idx != self.hovered_idx {
            self.hovered_idx = new_idx;
            self.render(qh);
        }
    }
}

// Wayland dispatch implementations
impl Dispatch<wl_registry::WlRegistry, ()> for WindowSession {
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
                "wl_output" => {
                    let out = registry.bind(name, version.min(4), qh, ());
                    state.outputs.push((out, None));
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<wl_compositor::WlCompositor, ()> for WindowSession {
    fn event(
        _: &mut Self,
        _: &wl_compositor::WlCompositor,
        _: wl_compositor::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {}
}

impl Dispatch<wl_shm::WlShm, ()> for WindowSession {
    fn event(
        _: &mut Self,
        _: &wl_shm::WlShm,
        _: wl_shm::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {}
}

impl Dispatch<wl_shm_pool::WlShmPool, ()> for WindowSession {
    fn event(
        _: &mut Self,
        _: &wl_shm_pool::WlShmPool,
        _: wl_shm_pool::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {}
}

impl Dispatch<wl_buffer::WlBuffer, ()> for WindowSession {
    fn event(
        _: &mut Self,
        _: &wl_buffer::WlBuffer,
        _: wl_buffer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {}
}

impl Dispatch<zwlr_layer_shell_v1::ZwlrLayerShellV1, ()> for WindowSession {
    fn event(
        _: &mut Self,
        _: &zwlr_layer_shell_v1::ZwlrLayerShellV1,
        _: zwlr_layer_shell_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {}
}

impl Dispatch<wl_output::WlOutput, ()> for WindowSession {
    fn event(
        state: &mut Self,
        output: &wl_output::WlOutput,
        event: wl_output::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_output::Event::Name { name } = event {
            for (out, out_name) in &mut state.outputs {
                if out == output {
                    *out_name = Some(name);
                    break;
                }
            }
        }
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for WindowSession {
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

impl Dispatch<wl_pointer::WlPointer, ()> for WindowSession {
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
                if btn_state == wl_pointer::ButtonState::Released {
                    if button == 0x110 {
                        // Left click selects hovered window
                        if let Some(idx) = state.hovered_idx {
                            state.selected_window = state.windows.get(idx).cloned();
                        }
                        state.running.store(false, Ordering::SeqCst);
                    } else if button == 0x111 {
                        // Right click cancels
                        state.selected_window = None;
                        state.running.store(false, Ordering::SeqCst);
                    }
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for WindowSession {
    fn event(
        state: &mut Self,
        _: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_keyboard::Event::Key { key, .. } = event {
            // Keycode 1 is Escape
            if key == 1 {
                state.selected_window = None;
                state.running.store(false, Ordering::SeqCst);
            }
        }
    }
}

impl Dispatch<wl_surface::WlSurface, ()> for WindowSession {
    fn event(
        _: &mut Self,
        _: &wl_surface::WlSurface,
        _: wl_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {}
}

impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, ()> for WindowSession {
    fn event(
        state: &mut Self,
        surface: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_layer_surface_v1::Event::Configure { serial, width, height } => {
                surface.ack_configure(serial);
                state.width = width;
                state.height = height;
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

pub fn run_window_capture(target_output_override: Option<&str>) -> Result<Option<CaptureFrame>> {
    let outs_val = query_umbriel("{\"cmd\":\"outputs\"}")
        .ok_or_else(|| KshotError::CaptureFailed("Cannot query outputs from Umbriel IPC".into()))?;
    let wins_val = query_umbriel("{\"cmd\":\"windows\"}")
        .ok_or_else(|| KshotError::CaptureFailed("Cannot query windows from Umbriel IPC".into()))?;

    let outs: Vec<UmbrielOutput> = outs_val
        .get("ok")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();

    let wins: Vec<UmbrielWindow> = wins_val
        .get("ok")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();

    let focused_output = outs
        .iter()
        .find(|o| {
            if let Some(target) = target_output_override {
                o.name.as_deref() == Some(target)
            } else {
                o.focused.unwrap_or(false)
            }
        })
        .or_else(|| outs.iter().find(|o| o.enabled.unwrap_or(true)))
        .cloned()
        .ok_or_else(|| KshotError::CaptureFailed("No active Wayland output found".into()))?;

    let target_output_name = focused_output.name.clone();
    let window_boxes = filter_windows_for_output(&wins, &focused_output);

    // Freeze screen
    let (frame, _) = crate::capture::capture_screen(target_output_name.as_deref())?;

    let conn = Connection::connect_to_env()
        .map_err(|e| KshotError::WaylandConnection(format!("Cannot connect to Wayland display: {e}")))?;

    let running = Arc::new(AtomicBool::new(true));
    let mut session = WindowSession::new(frame.clone(), target_output_name.clone(), window_boxes, Arc::clone(&running))?;

    let mut event_queue = conn.new_event_queue();
    let qh = event_queue.handle();

    let display = conn.display();
    display.get_registry(&qh, ());

    event_queue
        .roundtrip(&mut session)
        .map_err(|e| KshotError::WaylandProtocol(format!("Initial roundtrip failed: {e}")))?;
    event_queue
        .roundtrip(&mut session)
        .map_err(|e| KshotError::WaylandProtocol(format!("Output roundtrip failed: {e}")))?;

    let compositor = session
        .compositor
        .clone()
        .ok_or_else(|| KshotError::WaylandProtocol("Missing wl_compositor".into()))?;
    let layer_shell = session
        .layer_shell
        .clone()
        .ok_or_else(|| KshotError::WaylandProtocol("Missing zwlr_layer_shell_v1".into()))?;

    let target_wl_output = session
        .outputs
        .iter()
        .find(|(_, name)| name.as_ref() == target_output_name.as_ref())
        .map(|(out, _)| out.clone());

    let surface = compositor.create_surface(&qh, ());
    let layer_surface = layer_shell.get_layer_surface(
        &surface,
        target_wl_output.as_ref(),
        zwlr_layer_shell_v1::Layer::Overlay,
        "dev.koresent.kshot.window".to_string(),
        &qh,
        (),
    );

    layer_surface.set_exclusive_zone(-1);
    layer_surface.set_keyboard_interactivity(zwlr_layer_surface_v1::KeyboardInteractivity::Exclusive);
    layer_surface.set_anchor(
        zwlr_layer_surface_v1::Anchor::Top
            | zwlr_layer_surface_v1::Anchor::Bottom
            | zwlr_layer_surface_v1::Anchor::Left
            | zwlr_layer_surface_v1::Anchor::Right,
    );

    surface.commit();

    session.surface = Some(surface);
    session.layer_surface = Some(layer_surface);

    while running.load(Ordering::SeqCst) {
        event_queue
            .blocking_dispatch(&mut session)
            .map_err(|e| KshotError::WaylandProtocol(format!("Dispatch error: {e}")))?;
    }

    if let Some(win) = session.selected_window {
        let cropped = frame.crop(
            win.local_x as i32,
            win.local_y as i32,
            win.local_w as u32,
            win.local_h as u32,
        )?;
        Ok(Some(cropped))
    } else {
        Ok(None)
    }
}
