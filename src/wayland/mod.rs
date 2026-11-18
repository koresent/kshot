pub mod gui_session;
pub mod shm;

pub use gui_session::GuiSession;

use crate::capture::CaptureFrame;
use crate::cli::GuiArgs;
use crate::config::Config;
use crate::error::{KshotError, Result};
use std::os::fd::{AsFd, AsRawFd};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use wayland_client::Connection;
use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1, zwlr_layer_surface_v1};

pub fn run_gui(frame: CaptureFrame, config: Config, gui_args: GuiArgs) -> Result<()> {
    let conn = Connection::connect_to_env()
        .map_err(|e| KshotError::WaylandConnection(format!("Cannot connect to Wayland display: {e}")))?;

    let running = Arc::new(AtomicBool::new(true));
    let mut session = GuiSession::new(frame, config, gui_args, Arc::clone(&running));

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
        None, // Fullscreen across active output / compositor canvas
        zwlr_layer_shell_v1::Layer::Overlay,
        "kshot".to_string(),
        &qh,
        (),
    );

    layer_surface.set_size(session.width, session.height);
    layer_surface.set_anchor(
        zwlr_layer_surface_v1::Anchor::Top
            | zwlr_layer_surface_v1::Anchor::Bottom
            | zwlr_layer_surface_v1::Anchor::Left
            | zwlr_layer_surface_v1::Anchor::Right,
    );
    layer_surface.set_exclusive_zone(-1);
    layer_surface.set_keyboard_interactivity(zwlr_layer_surface_v1::KeyboardInteractivity::Exclusive);

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
