use crate::capture::CaptureFrame;
use crate::error::{KshotError, Result};
use image::RgbImage;
use std::io::{BufRead, BufReader, Write};
use std::os::fd::{AsFd, AsRawFd, FromRawFd};
use std::os::unix::net::UnixStream;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tiny_skia::Pixmap;
use wayland_client::protocol::{
    wl_buffer, wl_compositor, wl_pointer, wl_registry, wl_seat, wl_shm, wl_shm_pool, wl_surface,
};
use wayland_client::{Connection, Dispatch, QueueHandle, WEnum, delegate_noop};
use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1, zwlr_layer_surface_v1};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

impl Rect {
    pub fn parse(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split_whitespace().collect();
        if parts.len() != 2 {
            return None;
        }
        let xy: Vec<&str> = parts[0].split(',').collect();
        let wh: Vec<&str> = parts[1].split('x').collect();
        if xy.len() != 2 || wh.len() != 2 {
            return None;
        }
        let x = xy[0].parse::<i32>().ok()?;
        let y = xy[1].parse::<i32>().ok()?;
        let w = wh[0].parse::<u32>().ok()?;
        let h = wh[1].parse::<u32>().ok()?;
        if w == 0 || h == 0 {
            return None;
        }
        Some(Self { x, y, w, h })
    }
}

pub fn get_umbriel_window_boxes() -> Vec<String> {
    let uid = unsafe { libc::getuid() };
    let sock = format!("/run/user/{uid}/umbriel-wayland-0.sock");
    let mut stream = match UnixStream::connect(&sock) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let _ = stream.write_all(b"{\"cmd\":\"windows\"}\n");
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        return Vec::new();
    }
    let val: serde_json::Value = match serde_json::from_str(&line) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let wins = match val.get("ok").and_then(|v| v.as_array()) {
        Some(w) => w,
        None => return Vec::new(),
    };

    wins.iter()
        .filter(|w| w.get("visible").and_then(|v| v.as_bool()).unwrap_or(false))
        .filter_map(|w| {
            let x = w.get("x")?.as_i64()?;
            let y = w.get("y")?.as_i64()?;
            let width = w.get("w")?.as_i64()?;
            let height = w.get("h")?.as_i64()?;
            if width > 10 && height > 10 {
                Some(format!("{x},{y} {width}x{height}"))
            } else {
                None
            }
        })
        .collect()
}

pub fn prompt_slurp_geometry() -> Option<Rect> {
    let boxes = get_umbriel_window_boxes();
    let mut slurp_child = Command::new("slurp")
        .args([
            "-B", "#b8bb2630",
            "-b", "#1d202180",
            "-c", "#b8bb26ff",
            "-s", "#b8bb2626",
            "-w", "2",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    if let Some(mut stdin) = slurp_child.stdin.take() {
        for b in &boxes {
            let _ = writeln!(stdin, "{b}");
        }
    }

    let output = slurp_child.wait_with_output().ok()?;
    if !output.status.success() {
        return None;
    }
    let geom_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Rect::parse(&geom_str)
}

pub fn compute_row_luminances(img: &RgbImage) -> Vec<f32> {
    let w = img.width() as usize;
    let h = img.height() as usize;
    let raw = img.as_raw();
    let mut lums = Vec::with_capacity(h);
    let stride = w * 3;

    for y in 0..h {
        let row_start = y * stride;
        let mut sum = 0u64;
        let step = 2usize;
        let mut count = 0u64;
        for x in (0..stride).step_by(3 * step) {
            let r = raw[row_start + x] as u64;
            let g = raw[row_start + x + 1] as u64;
            let b = raw[row_start + x + 2] as u64;
            sum += r * 299 + g * 587 + b * 114;
            count += 1000;
        }
        lums.push(if count > 0 { sum as f32 / count as f32 } else { 0.0 });
    }
    lums
}

pub struct RowStitcher {
    pub stitched: Option<RgbImage>,
    pub prev_frame: Option<RgbImage>,
    pub prev_lums: Vec<f32>,
    pub frame_count: usize,
}

impl RowStitcher {
    pub fn new() -> Self {
        Self {
            stitched: None,
            prev_frame: None,
            prev_lums: Vec::new(),
            frame_count: 0,
        }
    }

    pub fn add_frame(&mut self, frame: RgbImage) -> bool {
        self.frame_count += 1;
        if self.stitched.is_none() {
            self.prev_lums = compute_row_luminances(&frame);
            self.stitched = Some(frame.clone());
            self.prev_frame = Some(frame);
            return true;
        }

        let curr_lums = compute_row_luminances(&frame);
        let h = frame.height() as usize;
        let w = frame.width() as usize;

        if h < 20 {
            return false;
        }

        // Global stillness check: check if overall image didn't change
        let mut diff_zero = 0.0f32;
        for y in 0..h {
            diff_zero += (self.prev_lums[y] - curr_lums[y]).abs();
        }
        if (diff_zero / h as f32) < 0.4 {
            return false;
        }

        // Robust row-by-row voting algorithm
        let max_d = h.saturating_sub(20);
        let mut votes = vec![0u32; max_d + 1];

        for d in 1..=max_d {
            let mut count = 0;
            for y in d..h {
                if (self.prev_lums[y] - curr_lums[y - d]).abs() < 1.8 {
                    count += 1;
                }
            }
            votes[d] = count;
        }

        let mut best_d = 0;
        let mut max_votes = 0;
        for d in 1..=max_d {
            if votes[d] > max_votes {
                max_votes = votes[d];
                best_d = d;
            }
        }

        let overlap = h - best_d;
        let vote_ratio = max_votes as f32 / overlap as f32;
        if max_votes < 20 || vote_ratio < 0.25 {
            return false;
        }

        let mut y_max = 0;
        for y in best_d..h {
            if (self.prev_lums[y] - curr_lums[y - best_d]).abs() < 2.2 {
                if y > y_max {
                    y_max = y;
                }
            }
        }
        if y_max <= best_d {
            y_max = h;
        }

        let footer_h = h.saturating_sub(y_max);
        let curr_stitched = self.stitched.take().unwrap();
        let old_w = curr_stitched.width();
        let old_h = curr_stitched.height() as usize;
        let stride = w * 3;

        let splice_y = old_h.saturating_sub(footer_h);
        let new_h = splice_y + best_d + footer_h;

        let mut new_raw = Vec::with_capacity(new_h * stride);
        let old_raw = curr_stitched.as_raw();

        new_raw.extend_from_slice(&old_raw[..splice_y * stride]);

        let frame_raw = frame.as_raw();
        let append_start = (h - footer_h - best_d) * stride;
        let append_end = h * stride;
        new_raw.extend_from_slice(&frame_raw[append_start..append_end]);

        if let Some(new_img) = RgbImage::from_raw(old_w, new_h as u32, new_raw) {
            self.stitched = Some(new_img);
            self.prev_lums = curr_lums;
            self.prev_frame = Some(frame);
            true
        } else {
            self.stitched = Some(curr_stitched);
            false
        }
    }
}

pub fn grab_region_ppm(rect: &Rect) -> Result<RgbImage> {
    let geom = format!("{},{} {}x{}", rect.x, rect.y, rect.w, rect.h);
    let mut cmd = Command::new("grim");
    cmd.args(["-g", &geom, "-t", "ppm", "-"]);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::null());

    let output = cmd
        .output()
        .map_err(|e| KshotError::CaptureFailed(format!("Failed to invoke grim: {e}")))?;

    if !output.status.success() {
        return Err(KshotError::CaptureFailed("grim exited with error".into()));
    }

    let buf = &output.stdout;
    if buf.len() < 15 || &buf[0..2] != b"P6" {
        return Err(KshotError::CaptureFailed("Invalid PPM output".into()));
    }

    let mut pos = 2;
    let mut header_parts = Vec::new();
    while pos < buf.len() && header_parts.len() < 3 {
        while pos < buf.len() && (buf[pos] == b' ' || buf[pos] == b'\n' || buf[pos] == b'\r' || buf[pos] == b'\t') {
            pos += 1;
        }
        if pos < buf.len() && buf[pos] == b'#' {
            while pos < buf.len() && buf[pos] != b'\n' {
                pos += 1;
            }
            continue;
        }
        let start = pos;
        while pos < buf.len() && !buf[pos].is_ascii_whitespace() {
            pos += 1;
        }
        if start < pos {
            if let Ok(s) = std::str::from_utf8(&buf[start..pos]) {
                header_parts.push(s.to_string());
            }
        }
    }

    if header_parts.len() < 3 {
        return Err(KshotError::CaptureFailed("Truncated PPM header".into()));
    }

    let width: u32 = header_parts[0].parse().map_err(|_| KshotError::CaptureFailed("Invalid width".into()))?;
    let height: u32 = header_parts[1].parse().map_err(|_| KshotError::CaptureFailed("Invalid height".into()))?;
    let max_val: u32 = header_parts[2].parse().map_err(|_| KshotError::CaptureFailed("Invalid max_val".into()))?;

    if max_val != 255 || width == 0 || height == 0 {
        return Err(KshotError::CaptureFailed("Unsupported PPM format".into()));
    }

    if pos < buf.len() && buf[pos].is_ascii_whitespace() {
        pos += 1;
    }

    let expected_len = (width as usize) * (height as usize) * 3;
    if buf.len() < pos + expected_len {
        return Err(KshotError::CaptureFailed("PPM data truncated".into()));
    }

    RgbImage::from_raw(width, height, buf[pos..pos + expected_len].to_vec())
        .ok_or_else(|| KshotError::CaptureFailed("Failed to construct image from raw PPM".into()))
}

// Minimal layer-shell preview app for real-time progress
struct PreviewApp {
    compositor: Option<wl_compositor::WlCompositor>,
    shm: Option<wl_shm::WlShm>,
    layer_shell: Option<zwlr_layer_shell_v1::ZwlrLayerShellV1>,
    seat: Option<wl_seat::WlSeat>,
    pointer: Option<wl_pointer::WlPointer>,
    surface: Option<wl_surface::WlSurface>,
    layer_surface: Option<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1>,
    configured: bool,
    width: u32,
    height: u32,
    done: Arc<AtomicBool>,
    last_click: Instant,
}

impl PreviewApp {
    fn new(done: Arc<AtomicBool>) -> Self {
        Self {
            compositor: None,
            shm: None,
            layer_shell: None,
            seat: None,
            pointer: None,
            surface: None,
            layer_surface: None,
            configured: false,
            width: 140,
            height: 200,
            done,
            last_click: Instant::now(),
        }
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for PreviewApp {
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
                    state.compositor = Some(registry.bind(name, version.min(4), qh, ()));
                }
                "wl_shm" => {
                    state.shm = Some(registry.bind(name, version.min(1), qh, ()));
                }
                "zwlr_layer_shell_v1" => {
                    state.layer_shell = Some(registry.bind(name, version.min(4), qh, ()));
                }
                "wl_seat" => {
                    state.seat = Some(registry.bind(name, version.min(5), qh, ()));
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for PreviewApp {
    fn event(
        state: &mut Self,
        seat: &wl_seat::WlSeat,
        event: wl_seat::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_seat::Event::Capabilities { capabilities } = event {
            let has_pointer = match capabilities {
                WEnum::Value(val) => val.contains(wl_seat::Capability::Pointer),
                _ => false,
            };
            if has_pointer && state.pointer.is_none() {
                state.pointer = Some(seat.get_pointer(qh, ()));
            }
        }
    }
}

impl Dispatch<wl_pointer::WlPointer, ()> for PreviewApp {
    fn event(
        state: &mut Self,
        _: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_pointer::Event::Button { button, state: btn_state, .. } = event {
            if matches!(btn_state, WEnum::Value(wl_pointer::ButtonState::Pressed)) {
                let now = Instant::now();
                // Single right click or double left click finishes capture
                if button == 0x111 || (button == 0x110 && now.duration_since(state.last_click) < Duration::from_millis(450)) {
                    state.done.store(true, Ordering::SeqCst);
                }
                state.last_click = now;
            }
        }
    }
}

impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, ()> for PreviewApp {
    fn event(
        state: &mut Self,
        layer_surface: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_layer_surface_v1::Event::Configure { serial, width, height } => {
                layer_surface.ack_configure(serial);
                if width > 0 { state.width = width; }
                if height > 0 { state.height = height; }
                state.configured = true;
            }
            zwlr_layer_surface_v1::Event::Closed => {
                state.done.store(true, Ordering::SeqCst);
            }
            _ => {}
        }
    }
}

delegate_noop!(PreviewApp: ignore wl_compositor::WlCompositor);
delegate_noop!(PreviewApp: ignore wl_surface::WlSurface);
delegate_noop!(PreviewApp: ignore wl_shm::WlShm);
delegate_noop!(PreviewApp: ignore wl_shm_pool::WlShmPool);
delegate_noop!(PreviewApp: ignore wl_buffer::WlBuffer);
delegate_noop!(PreviewApp: ignore zwlr_layer_shell_v1::ZwlrLayerShellV1);

fn render_preview_pixels(stitched: &RgbImage, pw: u32, ph: u32) -> Vec<u8> {
    let mut pixels = vec![0u8; (pw * ph * 4) as usize];
    let stride = (pw * 4) as usize;

    // Background Gruvbox Dark #282828
    for chunk in pixels.chunks_exact_mut(4) {
        chunk[0] = 0x28; // B
        chunk[1] = 0x28; // G
        chunk[2] = 0x28; // R
        chunk[3] = 0xff; // A
    }

    // Border: #504945
    let border = [0x45, 0x49, 0x50, 0xff];
    for x in 0..pw as usize {
        for y in [0, 1, (ph - 1) as usize, (ph - 2) as usize] {
            let idx = y * stride + x * 4;
            pixels[idx..idx + 4].copy_from_slice(&border);
        }
    }
    for y in 0..ph as usize {
        for x in [0, 1, (pw - 1) as usize, (pw - 2) as usize] {
            let idx = y * stride + x * 4;
            pixels[idx..idx + 4].copy_from_slice(&border);
        }
    }

    let pad = 8usize;
    let thumb_max_w = (pw as usize).saturating_sub(pad * 2);
    let thumb_max_h = (ph as usize).saturating_sub(pad * 2);

    let src_w = stitched.width() as f32;
    let src_h = stitched.height() as f32;

    if src_w > 0.0 && src_h > 0.0 && thumb_max_w > 0 && thumb_max_h > 0 {
        let scale = (thumb_max_w as f32 / src_w).min(thumb_max_h as f32 / src_h);
        let draw_w = (src_w * scale).round().max(1.0) as usize;
        let draw_h = (src_h * scale).round().max(1.0) as usize;

        let offset_x = ((pw as usize).saturating_sub(draw_w)) / 2;
        let offset_y = pad + (thumb_max_h.saturating_sub(draw_h)) / 2;

        let src_raw = stitched.as_raw();
        let src_stride = (stitched.width() as usize) * 3;

        for dy in 0..draw_h {
            let sy = ((dy as f32 / scale).floor() as usize).min(stitched.height() as usize - 1);
            let dst_y = offset_y + dy;
            for dx in 0..draw_w {
                let sx = ((dx as f32 / scale).floor() as usize).min(stitched.width() as usize - 1);
                let s_idx = sy * src_stride + sx * 3;
                let d_idx = dst_y * stride + (offset_x + dx) * 4;

                pixels[d_idx] = src_raw[s_idx + 2];     // B
                pixels[d_idx + 1] = src_raw[s_idx + 1]; // G
                pixels[d_idx + 2] = src_raw[s_idx];     // R
                pixels[d_idx + 3] = 0xff;               // A
            }
        }
    }

    pixels
}

fn create_preview_shm_buffer(
    shm: &wl_shm::WlShm,
    qh: &QueueHandle<PreviewApp>,
    width: u32,
    height: u32,
    pixels: &[u8],
) -> Result<wl_buffer::WlBuffer> {
    let size = (width * height * 4) as usize;
    let fd = unsafe {
        let name = std::ffi::CString::new("kshot-vshot-preview").unwrap();
        let f = libc::memfd_create(name.as_ptr(), libc::MFD_CLOEXEC);
        if f < 0 {
            return Err(KshotError::CaptureFailed("memfd_create failed".into()));
        }
        if libc::ftruncate(f, size as libc::off_t) < 0 {
            libc::close(f);
            return Err(KshotError::CaptureFailed("ftruncate failed".into()));
        }
        f
    };

    let ptr = unsafe {
        let p = libc::mmap(std::ptr::null_mut(), size, libc::PROT_READ | libc::PROT_WRITE, libc::MAP_SHARED, fd, 0);
        if p == libc::MAP_FAILED {
            libc::close(fd);
            return Err(KshotError::CaptureFailed("mmap failed".into()));
        }
        p as *mut u8
    };

    unsafe {
        std::ptr::copy_nonoverlapping(pixels.as_ptr(), ptr, size);
        libc::munmap(ptr as *mut libc::c_void, size);
    }

    let file = unsafe { std::fs::File::from_raw_fd(fd) };
    let pool = shm.create_pool(file.as_fd(), size as i32, qh, ());
    let buffer = pool.create_buffer(0, width as i32, height as i32, (width * 4) as i32, wl_shm::Format::Argb8888, qh, ());
    Ok(buffer)
}

pub fn run_scroll_capture(geometry_opt: Option<&str>) -> Result<Option<CaptureFrame>> {
    let rect = if let Some(geom) = geometry_opt {
        Rect::parse(geom).ok_or_else(|| KshotError::InvalidGeometry("Invalid geometry string".into()))?
    } else {
        match prompt_slurp_geometry() {
            Some(r) => r,
            None => return Ok(None),
        }
    };

    let stitcher = Arc::new(Mutex::new(RowStitcher::new()));
    let stitcher_preview = Arc::clone(&stitcher);
    let done = Arc::new(AtomicBool::new(false));
    let done_preview = Arc::clone(&done);

    // Spawn preview thread
    let _preview_thread = std::thread::spawn(move || {
        let conn = match Connection::connect_to_env() {
            Ok(c) => c,
            Err(_) => return,
        };

        let mut app = PreviewApp::new(Arc::clone(&done_preview));
        let mut event_queue = conn.new_event_queue();
        let qh = event_queue.handle();

        let display = conn.display();
        display.get_registry(&qh, ());
        let _ = event_queue.roundtrip(&mut app);

        let compositor = match app.compositor.as_ref() {
            Some(c) => c.clone(),
            None => return,
        };
        let layer_shell = match app.layer_shell.as_ref() {
            Some(ls) => ls.clone(),
            None => return,
        };

        let surface = compositor.create_surface(&qh, ());
        let layer_surface = layer_shell.get_layer_surface(
            &surface,
            None,
            zwlr_layer_shell_v1::Layer::Overlay,
            "dev.koresent.kshot.scroll-preview".to_string(),
            &qh,
            (),
        );

        layer_surface.set_size(140, 200);
        layer_surface.set_anchor(zwlr_layer_surface_v1::Anchor::Bottom | zwlr_layer_surface_v1::Anchor::Right);
        layer_surface.set_margin(0, 24, 24, 0);
        layer_surface.set_exclusive_zone(-1);
        surface.commit();
        let _ = conn.flush();

        app.surface = Some(surface.clone());
        app.layer_surface = Some(layer_surface);

        while !app.configured && !done_preview.load(Ordering::SeqCst) {
            let _ = event_queue.blocking_dispatch(&mut app);
        }

        let mut last_rendered_h = 0;

        while !done_preview.load(Ordering::SeqCst) {
            let _ = event_queue.dispatch_pending(&mut app);

            let current_image = {
                let st = stitcher_preview.lock().unwrap();
                st.stitched.clone()
            };

            if let Some(img) = current_image {
                if img.height() != last_rendered_h && app.configured {
                    last_rendered_h = img.height();
                    if let Some(shm) = app.shm.as_ref() {
                        let pixels = render_preview_pixels(&img, app.width, app.height);
                        if let Ok(buffer) = create_preview_shm_buffer(shm, &qh, app.width, app.height, &pixels) {
                            surface.attach(Some(&buffer), 0, 0);
                            surface.damage(0, 0, app.width as i32, app.height as i32);
                            surface.commit();
                            let _ = conn.flush();
                        }
                    }
                }
            }

            let _ = conn.flush();

            if let Some(guard) = event_queue.prepare_read() {
                let fd = guard.connection_fd().as_raw_fd();
                let mut pfd = libc::pollfd {
                    fd,
                    events: libc::POLLIN,
                    revents: 0,
                };
                let poll_res = unsafe { libc::poll(&mut pfd, 1, 35) };
                if poll_res > 0 && (pfd.revents & libc::POLLIN) != 0 {
                    let _ = guard.read();
                }
            }

            let _ = event_queue.dispatch_pending(&mut app);
        }
    });

    // Capture loop in main thread
    let capture_interval = Duration::from_millis(55);
    while !done.load(Ordering::SeqCst) {
        if let Ok(frame) = grab_region_ppm(&rect) {
            let mut st = stitcher.lock().unwrap();
            st.add_frame(frame);
        }
        std::thread::sleep(capture_interval);
    }

    let final_image = {
        let st = stitcher.lock().unwrap();
        st.stitched.clone()
    };

    if let Some(img) = final_image {
        let (w, h) = (img.width(), img.height());
        let mut pixmap = Pixmap::new(w, h)
            .ok_or_else(|| KshotError::CaptureFailed("Failed to allocate pixmap for scroll result".into()))?;

        let src_raw = img.as_raw();
        let dst_raw = pixmap.data_mut();
        let total = (w * h) as usize;
        for i in 0..total {
            let src_i = i * 3;
            let dst_i = i * 4;
            dst_raw[dst_i] = src_raw[src_i];
            dst_raw[dst_i + 1] = src_raw[src_i + 1];
            dst_raw[dst_i + 2] = src_raw[src_i + 2];
            dst_raw[dst_i + 3] = 255;
        }

        Ok(Some(CaptureFrame::new(w, h, pixmap)))
    } else {
        Ok(None)
    }
}
