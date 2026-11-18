use crate::capture::CaptureFrame;
use crate::cli::GuiArgs;
use crate::config::Config;
use crate::export::{copy_png_to_clipboard, save_image_to_file, send_clipboard_notification};
use crate::render::canvas::ActiveTextInput;
use crate::render::selection::{Handle, SelectionRect};
use crate::render::tools::{Annotation, ArrowStyle, ToolType};
use crate::render::Canvas;
use crate::ui::button::ButtonId;
use crate::ui::{ColorWheel, ToolbarManager};
use crate::wayland::shm::{create_shm_buffer, ShmBuffer};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tiny_skia::Pixmap;
use wayland_client::protocol::{
    wl_buffer, wl_callback, wl_compositor, wl_keyboard, wl_output, wl_pointer, wl_registry, wl_seat, wl_shm,
    wl_shm_pool, wl_surface,
};
use wayland_client::{Connection, Dispatch, QueueHandle, WEnum};
use wayland_protocols::wp::cursor_shape::v1::client::{wp_cursor_shape_device_v1, wp_cursor_shape_manager_v1};
use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1, zwlr_layer_surface_v1};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DragMode {
    None,
    CreatingSelection { start_x: f32, start_y: f32 },
    ResizingSelection { handle: Handle },
    MovingSelection { start_x: f32, start_y: f32, orig_x: f32, orig_y: f32 },
    DrawingTool { start_x: f32, start_y: f32 },
}

pub struct GuiSession {
    pub canvas: Canvas,
    pub toolbar: ToolbarManager,
    pub color_wheel: ColorWheel,
    pub config: Config,
    pub gui_args: GuiArgs,
    pub active_tool: ToolType,
    pub drag_mode: DragMode,
    pub counter_value: u32,
    pub shift_held: bool,
    pub ctrl_held: bool,

    // Wayland objects
    pub compositor: Option<wl_compositor::WlCompositor>,
    pub shm: Option<wl_shm::WlShm>,
    pub layer_shell: Option<zwlr_layer_shell_v1::ZwlrLayerShellV1>,
    pub seat: Option<wl_seat::WlSeat>,
    pub pointer: Option<wl_pointer::WlPointer>,
    pub keyboard: Option<wl_keyboard::WlKeyboard>,
    pub cursor_shape_manager: Option<wp_cursor_shape_manager_v1::WpCursorShapeManagerV1>,
    pub cursor_shape_device: Option<wp_cursor_shape_device_v1::WpCursorShapeDeviceV1>,
    pub last_enter_serial: u32,
    pub xkb_state: Option<xkbcommon::xkb::State>,

    pub outputs: Vec<(wl_output::WlOutput, Option<String>)>,
    pub target_output_name: Option<String>,

    pub surface: Option<wl_surface::WlSurface>,
    pub layer_surface: Option<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1>,
    pub configured: bool,

    pub width: u32,
    pub height: u32,
    pub shm_buffers: Option<[ShmBuffer; 2]>,
    pub current_buf_idx: usize,
    pub render_buffer: Option<Pixmap>,
    pub last_click: Option<(std::time::Instant, f32, f32)>,
    pub needs_render: bool,
    pub frame_callback_pending: bool,
    pub running: Arc<AtomicBool>,
}

impl GuiSession {
    pub fn new(
        frame: CaptureFrame,
        target_output_name: Option<String>,
        config: Config,
        gui_args: GuiArgs,
        running: Arc<AtomicBool>,
    ) -> Self {
        let w = frame.width;
        let h = frame.height;
        let counter_start = config.tools.counter_start;
        let default_thickness = config.theme.default_thickness as f32;
        let swatches = config.theme.swatches.clone();

        let initial_tool = match config.tools.initial_tool.as_str() {
            "arrow" => ToolType::Arrow,
            "rectangle" => ToolType::Rectangle,
            "circle" => ToolType::Circle,
            "pencil" => ToolType::Pencil,
            "marker" => ToolType::Marker,
            "text" => ToolType::Text,
            "counter" => ToolType::Counter,
            "pixelate" => ToolType::Pixelate,
            "invert" => ToolType::Invert,
            _ => ToolType::Selection,
        };

        let mut canvas = Canvas::new(frame, &config);

        // Pre-select region if provided in args
        if let Some(ref reg) = gui_args.region {
            if let Some(s) = parse_region_geometry(reg) {
                canvas.selection = Some(s);
            }
        }

        let mut toolbar = ToolbarManager::new(&swatches, default_thickness);
        toolbar.update_layout(canvas.selection.as_ref(), w as f32, h as f32);

        Self {
            canvas,
            toolbar,
            color_wheel: ColorWheel::new(),
            config,
            gui_args,
            active_tool: initial_tool,
            drag_mode: DragMode::None,
            counter_value: counter_start,
            shift_held: false,
            ctrl_held: false,

            compositor: None,
            shm: None,
            layer_shell: None,
            seat: None,
            pointer: None,
            keyboard: None,
            cursor_shape_manager: None,
            cursor_shape_device: None,
            last_enter_serial: 0,
            xkb_state: None,
            outputs: Vec::new(),
            target_output_name,

            surface: None,
            layer_surface: None,
            configured: false,

            width: w,
            height: h,
            shm_buffers: None,
            current_buf_idx: 0,
            render_buffer: Pixmap::new(w, h),
            last_click: None,
            needs_render: false,
            frame_callback_pending: false,
            running,
        }
    }

    pub fn set_cursor_shape(&mut self, shape: wp_cursor_shape_device_v1::Shape) {
        if let Some(ref device) = self.cursor_shape_device {
            device.set_shape(self.last_enter_serial, shape);
        }
    }

    pub fn update_cursor(&mut self) {
        let (px, py) = match self.canvas.cursor_pos {
            Some((x, y)) => (x as f32, y as f32),
            None => return,
        };

        let shape = if self.color_wheel.visible || self.toolbar.hovered_button.is_some() {
            wp_cursor_shape_device_v1::Shape::Pointer
        } else if let Some(ref sel) = self.canvas.selection {
            if let Some(handle) = sel.hit_handle(px, py, 10.0) {
                match handle {
                    Handle::TopLeft | Handle::BottomRight => wp_cursor_shape_device_v1::Shape::NwseResize,
                    Handle::TopRight | Handle::BottomLeft => wp_cursor_shape_device_v1::Shape::NeswResize,
                    Handle::TopCenter | Handle::BottomCenter => wp_cursor_shape_device_v1::Shape::NsResize,
                    Handle::MiddleLeft | Handle::MiddleRight => wp_cursor_shape_device_v1::Shape::EwResize,
                }
            } else if sel.contains(px, py) && self.active_tool == ToolType::Selection {
                wp_cursor_shape_device_v1::Shape::Move
            } else if self.active_tool == ToolType::Text {
                wp_cursor_shape_device_v1::Shape::Text
            } else {
                wp_cursor_shape_device_v1::Shape::Crosshair
            }
        } else if self.active_tool == ToolType::Text {
            wp_cursor_shape_device_v1::Shape::Text
        } else {
            wp_cursor_shape_device_v1::Shape::Crosshair
        };

        self.set_cursor_shape(shape);
    }

    pub fn queue_render(&mut self, qh: &QueueHandle<Self>) {
        self.needs_render = true;
        if !self.frame_callback_pending {
            self.render_frame(qh);
        }
    }

    pub fn render_frame(&mut self, qh: &QueueHandle<Self>) {
        if !self.configured || self.width == 0 || self.height == 0 {
            return;
        }

        self.needs_render = false;
        self.frame_callback_pending = true;

        let shm = match self.shm.as_ref() {
            Some(s) => s,
            None => return,
        };

        if self.shm_buffers.is_none() {
            let b1 = match create_shm_buffer(shm, qh, self.width, self.height) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("Failed to create shm buffer 1: {e}");
                    return;
                }
            };
            let b2 = match create_shm_buffer(shm, qh, self.width, self.height) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("Failed to create shm buffer 2: {e}");
                    return;
                }
            };
            self.shm_buffers = Some([b1, b2]);
        }

        let back_idx = 1 - self.current_buf_idx;
        let buf = &mut self.shm_buffers.as_mut().unwrap()[back_idx];

        // Wrap shm mmap pointer into tiny_skia PixmapMut
        let slice = unsafe { std::slice::from_raw_parts_mut(buf.mmap_ptr, buf.size) };
        let mut target_pixmap = match tiny_skia::PixmapMut::from_bytes(slice, self.width, self.height) {
            Some(p) => p,
            None => return,
        };

        // Ensure render buffer exists and matches dimensions
        if self.render_buffer.is_none()
            || self.render_buffer.as_ref().map(|p| (p.width(), p.height())) != Some((self.width, self.height))
        {
            self.render_buffer = Pixmap::new(self.width, self.height);
        }
        let render_buf = match self.render_buffer.as_mut() {
            Some(rb) => rb,
            None => return,
        };

        // Render everything from canvas onto the internal RGBA render buffer
        self.canvas.render_to(render_buf, &self.toolbar, &self.color_wheel, &self.config);

        // Convert RGBA (tiny-skia) to ARGB8888 (Wayland wl_shm little-endian: [B, G, R, A])
        let src = render_buf.data();
        let dst = target_pixmap.data_mut();
        let num_pixels = src.len() / 4;
        let src_u32 = unsafe { std::slice::from_raw_parts(src.as_ptr() as *const u32, num_pixels) };
        let dst_u32 = unsafe { std::slice::from_raw_parts_mut(dst.as_mut_ptr() as *mut u32, num_pixels) };
        for (s, d) in src_u32.iter().zip(dst_u32.iter_mut()) {
            let px = *s;
            let r = (px & 0xFF) as u32;
            let g = ((px >> 8) & 0xFF) as u32;
            let b = ((px >> 16) & 0xFF) as u32;
            let a = ((px >> 24) & 0xFF) as u32;
            *d = (a << 24) | (r << 16) | (g << 8) | b;
        }

        self.current_buf_idx = back_idx;

        if let Some(ref surface) = self.surface {
            surface.attach(Some(&buf.buffer), 0, 0);
            surface.damage_buffer(0, 0, self.width as i32, self.height as i32);

            let frame_cb = surface.frame(qh, ());
            let _ = frame_cb;

            surface.commit();
        }
    }

    pub fn commit_active_text(&mut self) {
        if let Some(input) = self.canvas.active_text_input.take() {
            if !input.text.trim().is_empty() {
                self.canvas.annotations.push(Annotation::Text {
                    x: input.x,
                    y: input.y,
                    text: input.text,
                    color: input.color,
                    font_size: input.font_size,
                });
            }
        }
    }

    pub fn finish_and_export(&mut self, copy_clip: bool, save_file: bool) {
        self.commit_active_text();

        // If no region selected, full frame is exported
        let frame = self.canvas.get_cropped_frame().unwrap_or_else(|| {
            let mut full_rendered = self.canvas.base_frame.pixmap.clone();
            for item in self.canvas.annotations.items() {
                item.render(&mut full_rendered, None);
            }
            CaptureFrame::new(self.canvas.width, self.canvas.height, full_rendered)
        });

        if copy_clip {
            if let Ok(bytes) = frame.to_png_bytes() {
                if let Err(e) = copy_png_to_clipboard(&bytes) {
                    eprintln!("Clipboard copy failed: {e}");
                } else if self.config.general.show_notifications {
                    send_clipboard_notification();
                }
            }
        }

        if save_file {
            let custom_path = self.gui_args.path.as_deref().map(std::path::Path::new);
            if let Ok(bytes) = frame.to_png_bytes() {
                if let Err(e) = save_image_to_file(&bytes, custom_path, &self.config) {
                    eprintln!("Save failed: {e}");
                }
            }
        }

        self.running.store(false, Ordering::SeqCst);
    }
}

fn parse_region_geometry(reg: &str) -> Option<SelectionRect> {
    let parts: Vec<&str> = reg.split(|c| c == ',' || c == 'x' || c == '+').collect();
    if parts.len() == 4 {
        let x: f32 = parts[0].parse().ok()?;
        let y: f32 = parts[1].parse().ok()?;
        let w: f32 = parts[2].parse().ok()?;
        let h: f32 = parts[3].parse().ok()?;
        Some(SelectionRect::new(x, y, w, h))
    } else {
        None
    }
}

fn get_key_name(key: u32, xkb_state: Option<&xkbcommon::xkb::State>) -> Option<String> {
    if let Some(xkb) = xkb_state {
        let keycode = xkbcommon::xkb::Keycode::new(key + 8);
        let sym = xkb.key_get_one_sym(keycode);
        let name = xkbcommon::xkb::keysym_get_name(sym).to_lowercase();
        let normalized = match name.as_str() {
            "return" => "enter",
            "kp_enter" => "enter",
            "escape" => "escape",
            "space" => "space",
            "backspace" => "backspace",
            _ => &name,
        };
        return Some(normalized.to_string());
    }

    let name = match key {
        1 => "escape",
        16 => "q",
        28 => "enter",
        57 => "space",
        44 => "z",
        21 => "y",
        46 => "c",
        31 => "s",
        30 => "a",
        19 => "r",
        33 => "f",
        25 => "p",
        50 => "m",
        20 => "t",
        49 => "n",
        48 => "b",
        22 => "u",
        23 => "i",
        _ => return None,
    };
    Some(name.to_string())
}

impl Dispatch<wl_registry::WlRegistry, ()> for GuiSession {
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
                    state.compositor = Some(registry.bind::<wl_compositor::WlCompositor, _, _>(
                        name,
                        version.min(4),
                        qh,
                        (),
                    ));
                }
                "wl_shm" => {
                    state.shm = Some(registry.bind::<wl_shm::WlShm, _, _>(name, 1, qh, ()));
                }
                "zwlr_layer_shell_v1" => {
                    state.layer_shell = Some(registry.bind::<zwlr_layer_shell_v1::ZwlrLayerShellV1, _, _>(
                        name,
                        version.min(4),
                        qh,
                        (),
                    ));
                }
                "wl_seat" => {
                    state.seat = Some(registry.bind::<wl_seat::WlSeat, _, _>(name, version.min(5), qh, ()));
                }
                "wp_cursor_shape_manager_v1" => {
                    let mgr = registry.bind::<wp_cursor_shape_manager_v1::WpCursorShapeManagerV1, _, _>(
                        name,
                        1,
                        qh,
                        (),
                    );
                    if let Some(ref ptr) = state.pointer {
                        state.cursor_shape_device = Some(mgr.get_pointer(ptr, qh, ()));
                    }
                    state.cursor_shape_manager = Some(mgr);
                }
                "wl_output" => {
                    let out = registry.bind::<wl_output::WlOutput, _, _>(name, version.min(4), qh, ());
                    state.outputs.push((out, None));
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, ()> for GuiSession {
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
                if width > 0 && height > 0 && (width != state.width || height != state.height) {
                    state.width = width;
                    state.height = height;
                    state.shm_buffers = None;
                    state.render_buffer = None;
                }
                state.render_frame(qh);
            }
            zwlr_layer_surface_v1::Event::Closed => {
                state.running.store(false, Ordering::SeqCst);
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_pointer::WlPointer, ()> for GuiSession {
    fn event(
        state: &mut Self,
        _: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_pointer::Event::Enter { serial, surface_x, surface_y, .. } => {
                state.last_enter_serial = serial;
                let px = surface_x as f32;
                let py = surface_y as f32;
                state.canvas.cursor_pos = Some((px as i32, py as i32));
                state.toolbar.handle_mouse_move(px, py);
                state.update_cursor();
                state.queue_render(qh);
            }
            wl_pointer::Event::Leave { .. } => {
                state.canvas.cursor_pos = None;
                state.queue_render(qh);
            }
            wl_pointer::Event::Motion { surface_x, surface_y, .. } => {
                let px = surface_x as f32;
                let py = surface_y as f32;
                state.canvas.cursor_pos = Some((px as i32, py as i32));

                if state.color_wheel.visible {
                    state.color_wheel.handle_mouse_move(px, py);
                    state.update_cursor();
                    state.queue_render(qh);
                    return;
                }

                // Handle drag interactions
                match state.drag_mode {
                    DragMode::CreatingSelection { start_x, start_y } => {
                        state.canvas.selection = Some(SelectionRect::from_points(start_x, start_y, px, py));
                        state.toolbar.update_layout(
                            state.canvas.selection.as_ref(),
                            state.width as f32,
                            state.height as f32,
                        );
                    }
                    DragMode::ResizingSelection { handle } => {
                        if let Some(ref sel) = state.canvas.selection {
                            state.canvas.selection = Some(sel.resize_handle(handle, px, py));
                            state.toolbar.update_layout(
                                state.canvas.selection.as_ref(),
                                state.width as f32,
                                state.height as f32,
                            );
                        }
                    }
                    DragMode::MovingSelection { start_x, start_y, orig_x, orig_y } => {
                        if let Some(ref mut sel) = state.canvas.selection {
                            let dx = px - start_x;
                            let dy = py - start_y;
                            sel.x = (orig_x + dx).clamp(0.0, (state.width as f32 - sel.w).max(0.0));
                            sel.y = (orig_y + dy).clamp(0.0, (state.height as f32 - sel.h).max(0.0));
                            state.toolbar.update_layout(
                                state.canvas.selection.as_ref(),
                                state.width as f32,
                                state.height as f32,
                            );
                        }
                    }
                    DragMode::DrawingTool { start_x, start_y } => {
                        let color = state.toolbar.active_color();
                        let thick = state.toolbar.thickness;

                        match state.active_tool {
                            ToolType::Arrow => {
                                let mut end_x = px;
                                let mut end_y = py;
                                if state.shift_held {
                                    let dx = px - start_x;
                                    let dy = py - start_y;
                                    if dx.abs() > dy.abs() * 2.0 {
                                        end_y = start_y;
                                    } else if dy.abs() > dx.abs() * 2.0 {
                                        end_x = start_x;
                                    } else {
                                        let dist = (dx.abs() + dy.abs()) * 0.5;
                                        end_x = start_x + dist * dx.signum();
                                        end_y = start_y + dist * dy.signum();
                                    }
                                }
                                let style = match state.config.tools.arrow_style.as_str() {
                                    "curved" => ArrowStyle::Curved,
                                    _ => ArrowStyle::Classic,
                                };
                                state.canvas.in_progress_annotation = Some(Annotation::Arrow {
                                    start: (start_x, start_y),
                                    end: (end_x, end_y),
                                    color,
                                    thickness: thick,
                                    style,
                                });
                            }
                            ToolType::Line => {
                                let mut end_x = px;
                                let mut end_y = py;
                                if state.shift_held {
                                    let dx = px - start_x;
                                    let dy = py - start_y;
                                    if dx.abs() > dy.abs() * 2.0 {
                                        end_y = start_y;
                                    } else if dy.abs() > dx.abs() * 2.0 {
                                        end_x = start_x;
                                    } else {
                                        let dist = (dx.abs() + dy.abs()) * 0.5;
                                        end_x = start_x + dist * dx.signum();
                                        end_y = start_y + dist * dy.signum();
                                    }
                                }
                                state.canvas.in_progress_annotation = Some(Annotation::Line {
                                    start: (start_x, start_y),
                                    end: (end_x, end_y),
                                    color,
                                    thickness: thick,
                                });
                            }
                            ToolType::Rectangle => {
                                let s = SelectionRect::from_points(start_x, start_y, px, py);
                                state.canvas.in_progress_annotation = Some(Annotation::Rectangle {
                                    x: s.x,
                                    y: s.y,
                                    w: s.w,
                                    h: s.h,
                                    color,
                                    thickness: thick,
                                    filled: false,
                                });
                            }
                            ToolType::RectFilled => {
                                let s = SelectionRect::from_points(start_x, start_y, px, py);
                                state.canvas.in_progress_annotation = Some(Annotation::Rectangle {
                                    x: s.x,
                                    y: s.y,
                                    w: s.w,
                                    h: s.h,
                                    color,
                                    thickness: thick,
                                    filled: true,
                                });
                            }
                            ToolType::Circle => {
                                let s = SelectionRect::from_points(start_x, start_y, px, py);
                                state.canvas.in_progress_annotation = Some(Annotation::Circle {
                                    x: s.x,
                                    y: s.y,
                                    w: s.w,
                                    h: s.h,
                                    color,
                                    thickness: thick,
                                    filled: state.shift_held,
                                });
                            }
                            ToolType::Pencil | ToolType::Marker => {
                                if let Some(Annotation::Pencil { ref mut points, .. }) = state.canvas.in_progress_annotation {
                                    points.push((px, py));
                                } else if let Some(Annotation::Marker { ref mut points, .. }) = state.canvas.in_progress_annotation {
                                    points.push((px, py));
                                }
                            }
                            ToolType::Pixelate => {
                                let s = SelectionRect::from_points(start_x, start_y, px, py);
                                state.canvas.in_progress_annotation = Some(Annotation::Pixelate {
                                    x: s.x as i32,
                                    y: s.y as i32,
                                    w: s.w as u32,
                                    h: s.h as u32,
                                    block_size: state.config.tools.pixelate_block_size,
                                });
                            }
                            ToolType::Blur => {
                                let s = SelectionRect::from_points(start_x, start_y, px, py);
                                state.canvas.in_progress_annotation = Some(Annotation::Blur {
                                    x: s.x as i32,
                                    y: s.y as i32,
                                    w: s.w as u32,
                                    h: s.h as u32,
                                    radius: state.config.tools.blur_radius as usize,
                                });
                            }
                            ToolType::Invert => {
                                let s = SelectionRect::from_points(start_x, start_y, px, py);
                                state.canvas.in_progress_annotation = Some(Annotation::Invert {
                                    x: s.x as i32,
                                    y: s.y as i32,
                                    w: s.w as u32,
                                    h: s.h as u32,
                                });
                            }
                            _ => {}
                        }
                    }
                    DragMode::None => {
                        state.toolbar.handle_mouse_move(px, py);
                    }
                }

                // Update magnifier visibility (Flameshot logic: active in selection mode, hidden when drawing)
                let is_selection_tool = state.active_tool == ToolType::Selection;
                let is_creating_sel = matches!(state.drag_mode, DragMode::CreatingSelection { .. });
                let no_hover = state.toolbar.hovered_button.is_none() && !state.color_wheel.visible;
                state.canvas.show_magnifier = (state.canvas.selection.is_none() || is_creating_sel || is_selection_tool) && no_hover;

                state.update_cursor();
                state.queue_render(qh);
            }

            wl_pointer::Event::Button { button, state: btn_state, .. } => {
                let pressed = btn_state == WEnum::Value(wl_pointer::ButtonState::Pressed);
                let (cx, cy) = state.canvas.cursor_pos.unwrap_or((0, 0));
                let px = cx as f32;
                let py = cy as f32;

                if button == 0x110 {
                    // Left mouse button
                    if pressed {
                        // 1. Color Wheel Interaction (Flameshot circular palette)
                        if state.color_wheel.visible {
                            if let Some(chosen) = state.color_wheel.handle_click(px, py) {
                                state.toolbar.set_active_color(chosen);
                            }
                            state.color_wheel.hide();
                            state.update_cursor();
                            state.queue_render(qh);
                            return;
                        }

                        // 2. Active Text Input Commit on Click
                        if state.canvas.active_text_input.is_some() {
                            state.commit_active_text();
                            if state.active_tool != ToolType::Text {
                                state.queue_render(qh);
                                return;
                            }
                        }

                        // 3. Check Toolbar click
                        if let Some(btn_id) = state.toolbar.handle_click(px, py) {
                            match btn_id {
                                ButtonId::ActionConfirm | ButtonId::ActionCopy => {
                                    state.finish_and_export(true, false);
                                    return;
                                }
                                ButtonId::ActionSave => {
                                    state.finish_and_export(false, true);
                                    return;
                                }
                                ButtonId::ActionCancel => {
                                    state.running.store(false, Ordering::SeqCst);
                                    return;
                                }
                                ButtonId::ActionUndo => {
                                    state.canvas.annotations.undo();
                                }
                                ButtonId::ActionRedo => {
                                    state.canvas.annotations.redo();
                                }
                                ButtonId::ActionPin => {
                                    if let Some(cropped) = state.canvas.get_cropped_frame() {
                                        if let Ok(bytes) = cropped.to_png_bytes() {
                                            let tmp_path = format!("/tmp/kshot_pin_{}.png", std::process::id());
                                            if std::fs::write(&tmp_path, &bytes).is_ok() {
                                                let _ = std::process::Command::new("kshot")
                                                    .args(["pin", &tmp_path])
                                                    .spawn();
                                                state.running.store(false, Ordering::SeqCst);
                                                return;
                                            }
                                        }
                                    }
                                }
                                ButtonId::ToolSelection => state.active_tool = ToolType::Selection,
                                ButtonId::ToolPencil => state.active_tool = ToolType::Pencil,
                                ButtonId::ToolLine => state.active_tool = ToolType::Line,
                                ButtonId::ToolArrow => state.active_tool = ToolType::Arrow,
                                ButtonId::ToolRectangle => state.active_tool = ToolType::Rectangle,
                                ButtonId::ToolRectFilled => state.active_tool = ToolType::RectFilled,
                                ButtonId::ToolCircle => state.active_tool = ToolType::Circle,
                                ButtonId::ToolMarker => state.active_tool = ToolType::Marker,
                                ButtonId::ToolText => state.active_tool = ToolType::Text,
                                ButtonId::ToolCounter => state.active_tool = ToolType::Counter,
                                ButtonId::ToolBlur => state.active_tool = ToolType::Blur,
                                ButtonId::ToolPixelate => state.active_tool = ToolType::Pixelate,
                                ButtonId::ToolInvert => state.active_tool = ToolType::Invert,
                            }
                            state.toolbar.set_active_tool(btn_id);
                            state.update_cursor();
                            state.queue_render(qh);
                            return;
                        }

                        // 4. Check Selection handle hit
                        if let Some(ref sel) = state.canvas.selection {
                            if let Some(h) = sel.hit_handle(px, py, 10.0) {
                                state.drag_mode = DragMode::ResizingSelection { handle: h };
                                state.update_cursor();
                                return;
                            }
                        }

                        // 5. Check Double Click for Full Screen
                        let now = std::time::Instant::now();
                        let is_double_click = if let Some((prev_time, prev_x, prev_y)) = state.last_click {
                            now.duration_since(prev_time).as_millis() < 350
                                && (px - prev_x).abs() < 5.0
                                && (py - prev_y).abs() < 5.0
                        } else {
                            false
                        };

                        if is_double_click {
                            state.canvas.selection = Some(SelectionRect::new(
                                0.0,
                                0.0,
                                state.width as f32,
                                state.height as f32,
                            ));
                            state.toolbar.update_layout(
                                state.canvas.selection.as_ref(),
                                state.width as f32,
                                state.height as f32,
                            );
                            state.last_click = None;
                            state.update_cursor();
                            state.queue_render(qh);
                            return;
                        }
                        state.last_click = Some((now, px, py));

                        // 6. Check Inside Selection or Create New Selection / Tools
                        let inside_sel = state.canvas.selection.as_ref().map(|s| s.contains(px, py)).unwrap_or(false);

                        if state.active_tool == ToolType::Selection {
                            if inside_sel {
                                let sel = state.canvas.selection.as_ref().unwrap();
                                state.drag_mode = DragMode::MovingSelection {
                                    start_x: px,
                                    start_y: py,
                                    orig_x: sel.x,
                                    orig_y: sel.y,
                                };
                            } else {
                                state.drag_mode = DragMode::CreatingSelection { start_x: px, start_y: py };
                            }
                        } else if state.active_tool == ToolType::Text {
                            // Text tool: start interactive text entry at clicked coordinates
                            let color = state.toolbar.active_color();
                            let font_size = 16.0 + state.toolbar.thickness * 2.0;
                            state.canvas.active_text_input = Some(ActiveTextInput {
                                x: px,
                                y: py,
                                text: String::new(),
                                color,
                                font_size,
                                cursor_visible: true,
                            });
                        } else if state.active_tool == ToolType::Counter {
                            let color = state.toolbar.active_color();
                            state.canvas.annotations.push(Annotation::Counter {
                                number: state.counter_value,
                                center: (px, py),
                                pointer: None,
                                color,
                                radius: 14.0,
                            });
                            state.counter_value += 1;
                        } else if state.active_tool == ToolType::Pencil {
                            let color = state.toolbar.active_color();
                            state.canvas.in_progress_annotation = Some(Annotation::Pencil {
                                points: vec![(px, py)],
                                color,
                                thickness: state.toolbar.thickness,
                            });
                            state.drag_mode = DragMode::DrawingTool { start_x: px, start_y: py };
                        } else if state.active_tool == ToolType::Marker {
                            let color = state.toolbar.active_color();
                            state.canvas.in_progress_annotation = Some(Annotation::Marker {
                                points: vec![(px, py)],
                                color,
                                thickness: state.toolbar.thickness,
                            });
                            state.drag_mode = DragMode::DrawingTool { start_x: px, start_y: py };
                        } else {
                            state.drag_mode = DragMode::DrawingTool { start_x: px, start_y: py };
                        }
                    } else {
                        // Left button released
                        if let Some(ann) = state.canvas.in_progress_annotation.take() {
                            state.canvas.annotations.push(ann);
                        }

                        let was_creating = matches!(state.drag_mode, DragMode::CreatingSelection { .. });
                        state.drag_mode = DragMode::None;

                        if was_creating && state.gui_args.accept_on_select {
                            state.finish_and_export(true, false);
                            return;
                        }
                    }
                    state.update_cursor();
                    state.queue_render(qh);
                } else if button == 0x111 && pressed {
                    // Right mouse button (Flameshot right-click circular palette)
                    if state.color_wheel.visible {
                        state.color_wheel.hide();
                    } else if state.canvas.active_text_input.is_some() {
                        state.canvas.active_text_input = None;
                    } else {
                        state.color_wheel.show_at(px, py, state.width as f32, state.height as f32);
                    }
                    state.update_cursor();
                    state.queue_render(qh);
                }
            }

            wl_pointer::Event::Axis { value, .. } => {
                // Mouse wheel: scroll up increases thickness, scroll down decreases (Flameshot toolSize)
                let delta = if value < 0.0 { 1.0 } else { -1.0 };
                state.toolbar.thickness = (state.toolbar.thickness + delta).clamp(1.0, 50.0);
                state.canvas.size_notification = Some((
                    format!("{} px", state.toolbar.thickness as u32),
                    std::time::Instant::now(),
                ));
                state.queue_render(qh);
            }

            _ => {}
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for GuiSession {
    fn event(
        state: &mut Self,
        _: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_keyboard::Event::Keymap { format, fd, size } => {
                if format == WEnum::Value(wl_keyboard::KeymapFormat::XkbV1) {
                    let ctx = xkbcommon::xkb::Context::new(xkbcommon::xkb::CONTEXT_NO_FLAGS);
                    let keymap = unsafe {
                        xkbcommon::xkb::Keymap::new_from_fd(
                            &ctx,
                            fd,
                            size as usize,
                            xkbcommon::xkb::KEYMAP_FORMAT_TEXT_V1,
                            xkbcommon::xkb::KEYMAP_COMPILE_NO_FLAGS,
                        )
                    };
                    if let Ok(Some(k)) = keymap {
                        state.xkb_state = Some(xkbcommon::xkb::State::new(&k));
                    }
                }
            }
            wl_keyboard::Event::Modifiers {
                mods_depressed,
                mods_latched,
                mods_locked,
                group,
                ..
            } => {
                if let Some(ref mut xkb) = state.xkb_state {
                    xkb.update_mask(mods_depressed, mods_latched, mods_locked, 0, 0, group);
                }
            }
            wl_keyboard::Event::Key { key, state: key_state, .. } => {
                let pressed = key_state == WEnum::Value(wl_keyboard::KeyState::Pressed);

                // Track modifier keys exclusively without triggering actions
                // Evdev: LeftCtrl = 29, RightCtrl = 97, LeftShift = 42, RightShift = 54, LeftAlt = 56, RightAlt = 100
                match key {
                    29 | 97 => {
                        state.ctrl_held = pressed;
                        return;
                    }
                    42 | 54 => {
                        state.shift_held = pressed;
                        return;
                    }
                    56 | 100 | 125 | 126 => {
                        return;
                    }
                    _ => {}
                }

                if !pressed {
                    return;
                }

                // If active text input is open, route typing to it
                if state.canvas.active_text_input.is_some() {
                    let keycode = xkbcommon::xkb::Keycode::new(key + 8);
                    let keysym_val = state
                        .xkb_state
                        .as_ref()
                        .map(|s| s.key_get_one_sym(keycode).raw())
                        .unwrap_or(0);

                    if keysym_val == xkbcommon::xkb::keysyms::KEY_Escape || key == 1 {
                        state.canvas.active_text_input = None;
                        state.queue_render(qh);
                        return;
                    }

                    if keysym_val == xkbcommon::xkb::keysyms::KEY_Return || key == 28 {
                        state.commit_active_text();
                        state.queue_render(qh);
                        return;
                    }

                    if keysym_val == xkbcommon::xkb::keysyms::KEY_BackSpace || key == 14 {
                        if let Some(ref mut input) = state.canvas.active_text_input {
                            input.text.pop();
                        }
                        state.queue_render(qh);
                        return;
                    }

                    if let Some(ref xkb) = state.xkb_state {
                        let text_chunk = xkb.key_get_utf8(keycode);
                        if !text_chunk.is_empty() && text_chunk.chars().all(|c| !c.is_control()) {
                            if let Some(ref mut input) = state.canvas.active_text_input {
                                input.text.push_str(&text_chunk);
                            }
                            state.queue_render(qh);
                            return;
                        }
                    }
                }

                // Resolve key name for configurable shortcut matching
                let key_name = get_key_name(key, state.xkb_state.as_ref()).unwrap_or_default();
                let mut current_shortcut = String::new();
                if state.ctrl_held {
                    current_shortcut.push_str("ctrl+");
                }
                if state.shift_held {
                    current_shortcut.push_str("shift+");
                }
                current_shortcut.push_str(&key_name);

                // 1. Actions (Undo, Redo, Copy, Save, Cancel, Confirm, Pin, Select All)
                let is_undo = state.config.matches_shortcut("undo", &current_shortcut)
                    || (state.ctrl_held && !state.shift_held && (key == 44 || key_name == "z" || key_name == "я"));

                let is_redo = state.config.matches_shortcut("redo", &current_shortcut)
                    || (state.ctrl_held && (key == 21 || key_name == "y" || key_name == "н" || (state.shift_held && (key == 44 || key_name == "z" || key_name == "я"))));

                let is_copy = state.config.matches_shortcut("copy", &current_shortcut)
                    || (state.ctrl_held && (key == 46 || key_name == "c" || key_name == "с"));

                let is_save = state.config.matches_shortcut("save", &current_shortcut)
                    || (state.ctrl_held && (key == 31 || key_name == "s" || key_name == "ы"));

                let is_select_all = state.config.matches_shortcut("select_all", &current_shortcut)
                    || (state.ctrl_held && (key == 30 || key_name == "a" || key_name == "ф"));

                let is_cancel = state.config.matches_shortcut("cancel", &current_shortcut)
                    || key == 1
                    || key_name == "escape"
                    || (!state.ctrl_held && (key == 16 || key_name == "q"));

                let is_confirm = state.config.matches_shortcut("confirm", &current_shortcut)
                    || key == 28
                    || key_name == "enter";

                let is_pin = state.config.matches_shortcut("pin", &current_shortcut)
                    || (!state.ctrl_held && (key == 57 || key_name == "space"));

                if is_cancel {
                    if state.color_wheel.visible {
                        state.color_wheel.hide();
                    } else if state.canvas.selection.is_some() {
                        state.canvas.selection = None;
                    } else {
                        state.running.store(false, Ordering::SeqCst);
                        return;
                    }
                } else if is_confirm {
                    state.finish_and_export(true, false);
                    return;
                } else if is_copy {
                    if state.canvas.selection.is_some() {
                        state.finish_and_export(true, false);
                    } else {
                        state.running.store(false, Ordering::SeqCst);
                    }
                    return;
                } else if is_save {
                    state.finish_and_export(false, true);
                    return;
                } else if is_undo {
                    state.canvas.annotations.undo();
                } else if is_redo {
                    state.canvas.annotations.redo();
                } else if is_pin {
                    if let Some(cropped) = state.canvas.get_cropped_frame() {
                        if let Ok(bytes) = cropped.to_png_bytes() {
                            let tmp_path = format!("/tmp/kshot_pin_{}.png", std::process::id());
                            if std::fs::write(&tmp_path, &bytes).is_ok() {
                                let _ = std::process::Command::new("kshot")
                                    .args(["pin", &tmp_path])
                                    .spawn();
                                state.running.store(false, Ordering::SeqCst);
                                return;
                            }
                        }
                    }
                } else if is_select_all {
                    state.canvas.selection = Some(SelectionRect::new(
                        0.0,
                        0.0,
                        state.width as f32,
                        state.height as f32,
                    ));
                    state.toolbar.update_layout(
                        state.canvas.selection.as_ref(),
                        state.width as f32,
                        state.height as f32,
                    );
                } else if !state.ctrl_held {
                    // 2. Tool switches (configurable via YAML shortcuts.bindings)
                    if state.config.matches_shortcut("selection", &current_shortcut) || key == 31 || key_name == "s" {
                        state.active_tool = ToolType::Selection;
                        state.toolbar.set_active_tool(ButtonId::ToolSelection);
                    } else if state.config.matches_shortcut("arrow", &current_shortcut) || key == 30 || key_name == "a" {
                        state.active_tool = ToolType::Arrow;
                        state.toolbar.set_active_tool(ButtonId::ToolArrow);
                    } else if state.config.matches_shortcut("rectangle", &current_shortcut) || key == 19 || key_name == "r" {
                        state.active_tool = ToolType::Rectangle;
                        state.toolbar.set_active_tool(ButtonId::ToolRectangle);
                    } else if state.config.matches_shortcut("rect_filled", &current_shortcut) || key == 33 || key_name == "f" {
                        state.active_tool = ToolType::RectFilled;
                        state.toolbar.set_active_tool(ButtonId::ToolRectFilled);
                    } else if state.config.matches_shortcut("circle", &current_shortcut) || key == 46 || key_name == "c" {
                        state.active_tool = ToolType::Circle;
                        state.toolbar.set_active_tool(ButtonId::ToolCircle);
                    } else if state.config.matches_shortcut("pencil", &current_shortcut) || key == 25 || key_name == "p" {
                        state.active_tool = ToolType::Pencil;
                        state.toolbar.set_active_tool(ButtonId::ToolPencil);
                    } else if state.config.matches_shortcut("marker", &current_shortcut) || key == 50 || key_name == "m" {
                        state.active_tool = ToolType::Marker;
                        state.toolbar.set_active_tool(ButtonId::ToolMarker);
                    } else if state.config.matches_shortcut("text", &current_shortcut) || key == 20 || key_name == "t" {
                        state.active_tool = ToolType::Text;
                        state.toolbar.set_active_tool(ButtonId::ToolText);
                    } else if state.config.matches_shortcut("counter", &current_shortcut) || key == 49 || key_name == "n" {
                        state.active_tool = ToolType::Counter;
                        state.toolbar.set_active_tool(ButtonId::ToolCounter);
                    } else if state.config.matches_shortcut("pixelate", &current_shortcut) || key == 48 || key_name == "b" {
                        state.active_tool = ToolType::Pixelate;
                        state.toolbar.set_active_tool(ButtonId::ToolPixelate);
                    } else if state.config.matches_shortcut("blur", &current_shortcut) || key == 22 || key_name == "u" {
                        state.active_tool = ToolType::Blur;
                        state.toolbar.set_active_tool(ButtonId::ToolBlur);
                    }
                }

                // 3. Arrow Keys: Nudge selection
                match key {
                    103 => {
                        if let Some(ref mut sel) = state.canvas.selection {
                            let step = if state.shift_held { 10.0 } else { 1.0 };
                            sel.nudge(0.0, -step, state.width as f32, state.height as f32);
                        }
                    }
                    108 => {
                        if let Some(ref mut sel) = state.canvas.selection {
                            let step = if state.shift_held { 10.0 } else { 1.0 };
                            sel.nudge(0.0, step, state.width as f32, state.height as f32);
                        }
                    }
                    105 => {
                        if let Some(ref mut sel) = state.canvas.selection {
                            let step = if state.shift_held { 10.0 } else { 1.0 };
                            sel.nudge(-step, 0.0, state.width as f32, state.height as f32);
                        }
                    }
                    106 => {
                        if let Some(ref mut sel) = state.canvas.selection {
                            let step = if state.shift_held { 10.0 } else { 1.0 };
                            sel.nudge(step, 0.0, state.width as f32, state.height as f32);
                        }
                    }
                    _ => {}
                }

                state.update_cursor();
                state.queue_render(qh);
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for GuiSession {
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
                    let ptr = seat.get_pointer(qh, ());
                    if let Some(ref mgr) = state.cursor_shape_manager {
                        state.cursor_shape_device = Some(mgr.get_pointer(&ptr, qh, ()));
                    }
                    state.pointer = Some(ptr);
                }
                if caps.contains(wl_seat::Capability::Keyboard) {
                    state.keyboard = Some(seat.get_keyboard(qh, ()));
                }
            }
        }
    }
}

impl Dispatch<wl_output::WlOutput, ()> for GuiSession {
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
                    *out_name = Some(name.clone());
                }
            }
        }
    }
}

impl Dispatch<wl_callback::WlCallback, ()> for GuiSession {
    fn event(
        state: &mut Self,
        _: &wl_callback::WlCallback,
        event: wl_callback::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_callback::Event::Done { .. } = event {
            state.frame_callback_pending = false;
            if state.needs_render {
                state.render_frame(qh);
            }
        }
    }
}

wayland_client::delegate_noop!(GuiSession: ignore wl_compositor::WlCompositor);
wayland_client::delegate_noop!(GuiSession: ignore wl_surface::WlSurface);
wayland_client::delegate_noop!(GuiSession: ignore wl_shm::WlShm);
wayland_client::delegate_noop!(GuiSession: ignore wl_shm_pool::WlShmPool);
wayland_client::delegate_noop!(GuiSession: ignore wl_buffer::WlBuffer);
wayland_client::delegate_noop!(GuiSession: ignore zwlr_layer_shell_v1::ZwlrLayerShellV1);
wayland_client::delegate_noop!(GuiSession: ignore wayland_protocols::wp::cursor_shape::v1::client::wp_cursor_shape_manager_v1::WpCursorShapeManagerV1);
wayland_client::delegate_noop!(GuiSession: ignore wayland_protocols::wp::cursor_shape::v1::client::wp_cursor_shape_device_v1::WpCursorShapeDeviceV1);
