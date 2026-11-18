use crate::capture::CaptureFrame;
use crate::error::{KshotError, Result};
use crate::export::copy_png_to_clipboard;
use crate::wayland::shm::{create_shm_buffer, ShmBuffer};
use std::os::fd::{AsFd, AsRawFd};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tiny_skia::{Paint, PathBuilder, Stroke, Transform};
use wayland_client::protocol::{
    wl_buffer, wl_compositor, wl_keyboard, wl_pointer, wl_registry, wl_seat, wl_shm,
    wl_shm_pool, wl_surface,
};
use wayland_client::{Connection, Dispatch, QueueHandle, WEnum};
use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1, zwlr_layer_surface_v1};

pub struct PinSession {
    pub frame: CaptureFrame,
    pub width: u32,
    pub height: u32,
    pub _scale: f32,
    pub pos_x: i32,
    pub pos_y: i32,
    pub is_dragging: bool,
    pub drag_start: (f32, f32),

    pub compositor: Option<wl_compositor::WlCompositor>,
    pub shm: Option<wl_shm::WlShm>,
    pub layer_shell: Option<zwlr_layer_shell_v1::ZwlrLayerShellV1>,
    pub seat: Option<wl_seat::WlSeat>,
    pub pointer: Option<wl_pointer::WlPointer>,
    pub keyboard: Option<wl_keyboard::WlKeyboard>,

    pub surface: Option<wl_surface::WlSurface>,
    pub layer_surface: Option<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1>,
    pub configured: bool,
    pub shm_buffer: Option<ShmBuffer>,
    pub running: Arc<AtomicBool>,
}

impl PinSession {
    pub fn new(frame: CaptureFrame, running: Arc<AtomicBool>) -> Self {
        let w = frame.width;
        let h = frame.height;
        Self {
            frame,
            width: w,
            height: h,
            _scale: 1.0,
            pos_x: 100,
            pos_y: 100,
            is_dragging: false,
            drag_start: (0.0, 0.0),

            compositor: None,
            shm: None,
            layer_shell: None,
            seat: None,
            pointer: None,
            keyboard: None,

            surface: None,
            layer_surface: None,
            configured: false,
            shm_buffer: None,
            running,
        }
    }

    pub fn render(&mut self, qh: &QueueHandle<Self>) {
        if !self.configured || self.width == 0 || self.height == 0 {
            return;
        }

        let shm = match self.shm.as_ref() {
            Some(s) => s,
            None => return,
        };

        if self.shm_buffer.is_none() {
            let buf = match create_shm_buffer(shm, qh, self.width, self.height) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("Failed to create shm buffer: {e}");
                    return;
                }
            };
            self.shm_buffer = Some(buf);
        }

        let buf = self.shm_buffer.as_mut().unwrap();
        let slice = unsafe { std::slice::from_raw_parts_mut(buf.mmap_ptr, buf.size) };
        let mut target = match tiny_skia::PixmapMut::from_bytes(slice, self.width, self.height) {
            Some(p) => p,
            None => return,
        };

        let mut temp = self.frame.pixmap.clone();
        if let Some(r) = tiny_skia::Rect::from_xywh(0.0, 0.0, self.width as f32, self.height as f32) {
            let path = PathBuilder::from_rect(r);
            let mut paint = Paint::default();
            paint.set_color_rgba8(254, 128, 25, 240); // Gruvbox orange
            let stroke = Stroke {
                width: 2.0,
                ..Default::default()
            };
            temp.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
        }
        let len = (self.width as usize * self.height as usize * 4).min(target.data_mut().len());
        target.data_mut()[..len].copy_from_slice(&temp.data()[..len]);

        if let Some(surf) = &self.surface {
            surf.attach(Some(&buf.buffer), 0, 0);
            surf.damage_buffer(0, 0, self.width as i32, self.height as i32);
            surf.commit();
        }
    }
}

pub fn run_pin_window(frame: CaptureFrame) -> Result<()> {
    let conn = Connection::connect_to_env()
        .map_err(|e| KshotError::WaylandConnection(format!("Cannot connect to Wayland display: {e}")))?;

    let running = Arc::new(AtomicBool::new(true));
    let mut session = PinSession::new(frame, Arc::clone(&running));

    let mut event_queue = conn.new_event_queue();
    let qh = event_queue.handle();

    let display = conn.display();
    display.get_registry(&qh, ());

    let _ = conn.flush();
    event_queue
        .roundtrip(&mut session)
        .map_err(|e| KshotError::WaylandProtocol(format!("Initial roundtrip failed: {e}")))?;

    let compositor = session
        .compositor
        .clone()
        .ok_or_else(|| KshotError::WaylandProtocol("Missing wl_compositor in Wayland registry".into()))?;

    let layer_shell = session
        .layer_shell
        .clone()
        .ok_or_else(|| KshotError::WaylandProtocol("Compositor does not support zwlr_layer_shell_v1".into()))?;

    let surface = compositor.create_surface(&qh, ());

    let layer_surface = layer_shell.get_layer_surface(
        &surface,
        None,
        zwlr_layer_shell_v1::Layer::Overlay,
        "kshot-pin".to_string(),
        &qh,
        (),
    );

    layer_surface.set_size(session.width, session.height);
    layer_surface.set_anchor(
        zwlr_layer_surface_v1::Anchor::Top | zwlr_layer_surface_v1::Anchor::Left,
    );
    layer_surface.set_margin(session.pos_y, 0, 0, session.pos_x);
    layer_surface.set_exclusive_zone(0);
    layer_surface.set_keyboard_interactivity(zwlr_layer_surface_v1::KeyboardInteractivity::OnDemand);

    surface.commit();

    session.surface = Some(surface);
    session.layer_surface = Some(layer_surface);

    let _ = conn.flush();
    let conn_fd = conn.as_fd();

    while running.load(Ordering::SeqCst) {
        let _ = event_queue.dispatch_pending(&mut session);

        let mut pollfd = libc::pollfd {
            fd: conn_fd.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };

        unsafe {
            libc::poll(&mut pollfd, 1, 16);
        }

        if let Some(guard) = event_queue.prepare_read() {
            let _ = guard.read();
        }

        let _ = conn.flush();
    }

    Ok(())
}

impl Dispatch<wl_registry::WlRegistry, ()> for PinSession {
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
                    state.compositor = Some(registry.bind::<wl_compositor::WlCompositor, _, _>(name, version.min(4), qh, ()));
                }
                "wl_shm" => {
                    state.shm = Some(registry.bind::<wl_shm::WlShm, _, _>(name, 1, qh, ()));
                }
                "zwlr_layer_shell_v1" => {
                    state.layer_shell = Some(registry.bind::<zwlr_layer_shell_v1::ZwlrLayerShellV1, _, _>(name, version.min(4), qh, ()));
                }
                "wl_seat" => {
                    state.seat = Some(registry.bind::<wl_seat::WlSeat, _, _>(name, version.min(5), qh, ()));
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, ()> for PinSession {
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
                state.configured = true;
                if width > 0 && height > 0 {
                    state.width = width;
                    state.height = height;
                }
                state.render(qh);
            }
            zwlr_layer_surface_v1::Event::Closed => {
                state.running.store(false, Ordering::SeqCst);
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_pointer::WlPointer, ()> for PinSession {
    fn event(
        state: &mut Self,
        _: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        _: &(),
        _: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_pointer::Event::Button { button, state: btn_state, .. } => {
                let pressed = btn_state == WEnum::Value(wl_pointer::ButtonState::Pressed);
                if button == 0x110 {
                    // Left click: drag or release
                    state.is_dragging = pressed;
                } else if button == 0x111 && pressed {
                    // Right click: copy to clipboard
                    if let Ok(bytes) = state.frame.to_png_bytes() {
                        let _ = copy_png_to_clipboard(&bytes);
                    }
                } else if button == 0x112 && pressed {
                    // Middle click: close
                    state.running.store(false, Ordering::SeqCst);
                }
            }
            wl_pointer::Event::Motion { surface_x, surface_y, .. } => {
                if state.is_dragging {
                    if let Some(ls) = &state.layer_surface {
                        let dx = (surface_x as f32 - state.drag_start.0) as i32;
                        let dy = (surface_y as f32 - state.drag_start.1) as i32;
                        state.pos_x += dx;
                        state.pos_y += dy;
                        ls.set_margin(state.pos_y, 0, 0, state.pos_x);
                        if let Some(s) = &state.surface {
                            s.commit();
                        }
                    }
                } else {
                    state.drag_start = (surface_x as f32, surface_y as f32);
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for PinSession {
    fn event(
        state: &mut Self,
        _: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_keyboard::Event::Key { key, state: key_state, .. } = event {
            if key_state == WEnum::Value(wl_keyboard::KeyState::Pressed) {
                if key == 1 || key == 16 {
                    // Esc or Q closes pinned window
                    state.running.store(false, Ordering::SeqCst);
                }
            }
        }
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for PinSession {
    fn event(
        state: &mut Self,
        seat: &wl_seat::WlSeat,
        event: wl_seat::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_seat::Event::Capabilities { capabilities } = event {
            if let WEnum::Value(caps) = capabilities {
                if caps.contains(wl_seat::Capability::Pointer) {
                    state.pointer = Some(seat.get_pointer(qh, ()));
                }
                if caps.contains(wl_seat::Capability::Keyboard) {
                    state.keyboard = Some(seat.get_keyboard(qh, ()));
                }
            }
        }
    }
}

wayland_client::delegate_noop!(PinSession: wl_compositor::WlCompositor);
wayland_client::delegate_noop!(PinSession: wl_surface::WlSurface);
wayland_client::delegate_noop!(PinSession: wl_shm::WlShm);
wayland_client::delegate_noop!(PinSession: wl_shm_pool::WlShmPool);
wayland_client::delegate_noop!(PinSession: wl_buffer::WlBuffer);
wayland_client::delegate_noop!(PinSession: zwlr_layer_shell_v1::ZwlrLayerShellV1);
