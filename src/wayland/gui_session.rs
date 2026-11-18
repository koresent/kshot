use crate::capture::CaptureFrame;
use crate::cli::GuiArgs;
use crate::config::Config;
use crate::export::{copy_png_to_clipboard, save_image_to_file, send_clipboard_notification};
use crate::render::selection::{Handle, SelectionRect};
use crate::render::tools::{Annotation, ArrowStyle, ToolType};
use crate::render::Canvas;
use crate::ui::button::ButtonId;
use crate::ui::ToolbarManager;
use crate::wayland::shm::{create_shm_buffer, ShmBuffer};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tiny_skia::Pixmap;
use wayland_client::protocol::{
    wl_buffer, wl_compositor, wl_keyboard, wl_output, wl_pointer, wl_registry, wl_seat, wl_shm,
    wl_shm_pool, wl_surface,
};
use wayland_client::{Connection, Dispatch, QueueHandle, WEnum};
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
    pub outputs: Vec<(wl_output::WlOutput, Option<String>)>,

    pub surface: Option<wl_surface::WlSurface>,
    pub layer_surface: Option<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1>,
    pub configured: bool,

    pub width: u32,
    pub height: u32,
    pub shm_buffers: Option<[ShmBuffer; 2]>,
    pub current_buf_idx: usize,
    pub running: Arc<AtomicBool>,
}

impl GuiSession {
    pub fn new(
        frame: CaptureFrame,
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
            outputs: Vec::new(),

            surface: None,
            layer_surface: None,
            configured: false,

            width: w,
            height: h,
            shm_buffers: None,
            current_buf_idx: 0,
            running,
        }
    }

    pub fn render_frame(&mut self, qh: &QueueHandle<Self>) {
        if !self.configured || self.width == 0 || self.height == 0 {
            return;
        }

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

        // Render everything from canvas onto the Wayland buffer
        let mut temp_pixmap = Pixmap::new(self.width, self.height).unwrap();
        self.canvas.render_to(&mut temp_pixmap, &self.toolbar, &self.config);

        // Copy rendered pixels into shm target
        target_pixmap.data_mut().copy_from_slice(temp_pixmap.data());

        // Attach & commit to Wayland surface
        if let Some(surf) = &self.surface {
            surf.attach(Some(&buf.buffer), 0, 0);
            surf.damage_buffer(0, 0, self.width as i32, self.height as i32);
            surf.commit();
        }

        self.current_buf_idx = back_idx;
    }

    pub fn finish_and_export(&mut self, copy: bool, save: bool) {
        let frame = match self.canvas.get_cropped_frame() {
            Some(f) => f,
            None => {
                // If no selection, export full frame
                self.canvas.base_frame.clone()
            }
        };

        let png_bytes = match frame.to_png_bytes() {
            Ok(b) => b,
            Err(e) => {
                eprintln!("Error encoding PNG: {e}");
                self.running.store(false, Ordering::SeqCst);
                return;
            }
        };

        if self.gui_args.raw {
            use std::io::Write;
            let _ = std::io::stdout().write_all(&png_bytes);
            self.running.store(false, Ordering::SeqCst);
            return;
        }

        if copy || self.gui_args.clipboard {
            if let Err(e) = copy_png_to_clipboard(&png_bytes) {
                eprintln!("Clipboard error: {e}");
            } else if self.config.general.show_notifications {
                send_clipboard_notification();
            }
        }

        if save || self.gui_args.path.is_some() {
            if let Err(e) = save_image_to_file(&png_bytes, self.gui_args.path.as_deref(), &self.config) {
                eprintln!("Save error: {e}");
            }
        }

        self.running.store(false, Ordering::SeqCst);
    }
}

pub fn parse_region_geometry(geom: &str) -> Option<SelectionRect> {
    // Format: WxH+X+Y
    let parts: Vec<&str> = geom.split('+').collect();
    if parts.len() != 3 {
        return None;
    }
    let wh: Vec<&str> = parts[0].split('x').collect();
    if wh.len() != 2 {
        return None;
    }

    let w: f32 = wh[0].parse().ok()?;
    let h: f32 = wh[1].parse().ok()?;
    let x: f32 = parts[1].parse().ok()?;
    let y: f32 = parts[2].parse().ok()?;

    Some(SelectionRect::new(x, y, w, h))
}

// -----------------------------------------------------------------------------
// Wayland Protocol Dispatch Implementations
// -----------------------------------------------------------------------------

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
                if width > 0 && height > 0 {
                    state.width = width;
                    state.height = height;
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
            wl_pointer::Event::Motion { surface_x, surface_y, .. } => {
                let px = surface_x as f32;
                let py = surface_y as f32;
                state.canvas.cursor_pos = Some((px as i32, py as i32));

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
                                let style = if state.config.tools.arrow_style == "classic" {
                                    ArrowStyle::Classic
                                } else {
                                    ArrowStyle::Curved
                                };
                                state.canvas.in_progress_annotation = Some(Annotation::Arrow {
                                    start: (start_x, start_y),
                                    end: (px, py),
                                    color,
                                    thickness: thick,
                                    style,
                                });
                            }
                            ToolType::Line => {
                                let mut end_x = px;
                                let mut end_y = py;
                                if state.shift_held {
                                    // Snap to 0, 45, 90 degrees
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
                                    filled: state.shift_held,
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

                state.render_frame(qh);
            }

            wl_pointer::Event::Button { button, state: btn_state, .. } => {
                let pressed = btn_state == WEnum::Value(wl_pointer::ButtonState::Pressed);
                let (cx, cy) = state.canvas.cursor_pos.unwrap_or((0, 0));
                let px = cx as f32;
                let py = cy as f32;

                if button == 0x110 {
                    // Left mouse button
                    if pressed {
                        // 1. Check Toolbar click
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
                                ButtonId::ToolSelection => state.active_tool = ToolType::Selection,
                                ButtonId::ToolArrow => state.active_tool = ToolType::Arrow,
                                ButtonId::ToolRectangle => state.active_tool = ToolType::Rectangle,
                                ButtonId::ToolCircle => state.active_tool = ToolType::Circle,
                                ButtonId::ToolPencil => state.active_tool = ToolType::Pencil,
                                ButtonId::ToolMarker => state.active_tool = ToolType::Marker,
                                ButtonId::ToolText => state.active_tool = ToolType::Text,
                                ButtonId::ToolCounter => state.active_tool = ToolType::Counter,
                                ButtonId::ToolPixelate => state.active_tool = ToolType::Pixelate,
                                ButtonId::ToolInvert => state.active_tool = ToolType::Invert,
                                _ => {}
                            }
                            state.toolbar.set_active_tool(btn_id);
                            state.render_frame(qh);
                            return;
                        }

                        // 2. Check Selection handle hit
                        if let Some(ref sel) = state.canvas.selection {
                            if let Some(h) = sel.hit_handle(px, py, 10.0) {
                                state.drag_mode = DragMode::ResizingSelection { handle: h };
                                return;
                            }
                        }

                        // 3. Check Inside Selection
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
                        } else {
                            // Active drawing tool
                            if state.active_tool == ToolType::Counter {
                                // Place counter badge immediately
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
                    state.render_frame(qh);
                } else if button == 0x111 && pressed {
                    // Right mouse button: decrement counter or reset selection
                    if state.active_tool == ToolType::Counter && state.counter_value > 1 {
                        state.counter_value -= 1;
                    } else {
                        state.active_tool = ToolType::Selection;
                        state.toolbar.set_active_tool(ButtonId::ToolSelection);
                    }
                    state.render_frame(qh);
                }
            }

            wl_pointer::Event::Axis { value, .. } => {
                // Mouse wheel: scroll up increases thickness, scroll down decreases
                if value < 0.0 {
                    state.toolbar.thickness = (state.toolbar.thickness + 1.0).min(40.0);
                } else if value > 0.0 {
                    state.toolbar.thickness = (state.toolbar.thickness - 1.0).max(1.0);
                }
                state.render_frame(qh);
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
            wl_keyboard::Event::Key { key, state: key_state, .. } => {
                let pressed = key_state == WEnum::Value(wl_keyboard::KeyState::Pressed);

                // Evdev key codes:
                // Esc = 1, Enter = 28, Space = 57, A = 30, S = 31, R = 19, C = 46,
                // P = 25, M = 50, T = 20, N = 49, B = 48, I = 23, Z = 44, Y = 21
                match key {
                    42 | 54 => state.shift_held = pressed, // Shift
                    29 | 97 => state.ctrl_held = pressed,  // Ctrl
                    _ => {}
                }

                if !pressed {
                    return;
                }

                match key {
                    1 => {
                        // Escape: clear selection or exit
                        if state.canvas.selection.is_some() {
                            state.canvas.selection = None;
                        } else {
                            state.running.store(false, Ordering::SeqCst);
                            return;
                        }
                    }
                    28 => {
                        // Enter: confirm & export
                        state.finish_and_export(true, false);
                        return;
                    }
                    46 if state.ctrl_held => {
                        // Ctrl+C: copy & exit
                        state.finish_and_export(true, false);
                        return;
                    }
                    31 if state.ctrl_held => {
                        // Ctrl+S: save & exit
                        state.finish_and_export(false, true);
                        return;
                    }
                    44 if state.ctrl_held => {
                        // Ctrl+Z: undo / Ctrl+Shift+Z: redo
                        if state.shift_held {
                            state.canvas.annotations.redo();
                        } else {
                            state.canvas.annotations.undo();
                        }
                    }
                    21 if state.ctrl_held => {
                        // Ctrl+Y: redo
                        state.canvas.annotations.redo();
                    }
                    30 if state.ctrl_held => {
                        // Ctrl+A: select all
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
                    }
                    // Tool shortcuts (without Ctrl)
                    30 if !state.ctrl_held => {
                        state.active_tool = ToolType::Arrow;
                        state.toolbar.set_active_tool(ButtonId::ToolArrow);
                    }
                    31 if !state.ctrl_held => {
                        state.active_tool = ToolType::Selection;
                        state.toolbar.set_active_tool(ButtonId::ToolSelection);
                    }
                    19 if !state.ctrl_held => {
                        state.active_tool = ToolType::Rectangle;
                        state.toolbar.set_active_tool(ButtonId::ToolRectangle);
                    }
                    46 if !state.ctrl_held => {
                        state.active_tool = ToolType::Circle;
                        state.toolbar.set_active_tool(ButtonId::ToolCircle);
                    }
                    25 => {
                        state.active_tool = ToolType::Pencil;
                        state.toolbar.set_active_tool(ButtonId::ToolPencil);
                    }
                    50 => {
                        state.active_tool = ToolType::Marker;
                        state.toolbar.set_active_tool(ButtonId::ToolMarker);
                    }
                    20 => {
                        state.active_tool = ToolType::Text;
                        state.toolbar.set_active_tool(ButtonId::ToolText);
                    }
                    49 => {
                        state.active_tool = ToolType::Counter;
                        state.toolbar.set_active_tool(ButtonId::ToolCounter);
                    }
                    48 => {
                        state.active_tool = ToolType::Pixelate;
                        state.toolbar.set_active_tool(ButtonId::ToolPixelate);
                    }
                    23 => {
                        state.active_tool = ToolType::Invert;
                        state.toolbar.set_active_tool(ButtonId::ToolInvert);
                    }
                    // Arrow nudging
                    103 => {
                        // Up
                        if let Some(ref mut sel) = state.canvas.selection {
                            let step = if state.shift_held { 10.0 } else { 1.0 };
                            sel.nudge(0.0, -step, state.width as f32, state.height as f32);
                        }
                    }
                    108 => {
                        // Down
                        if let Some(ref mut sel) = state.canvas.selection {
                            let step = if state.shift_held { 10.0 } else { 1.0 };
                            sel.nudge(0.0, step, state.width as f32, state.height as f32);
                        }
                    }
                    105 => {
                        // Left
                        if let Some(ref mut sel) = state.canvas.selection {
                            let step = if state.shift_held { 10.0 } else { 1.0 };
                            sel.nudge(-step, 0.0, state.width as f32, state.height as f32);
                        }
                    }
                    106 => {
                        // Right
                        if let Some(ref mut sel) = state.canvas.selection {
                            let step = if state.shift_held { 10.0 } else { 1.0 };
                            sel.nudge(step, 0.0, state.width as f32, state.height as f32);
                        }
                    }
                    _ => {}
                }

                state.render_frame(qh);
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
                    state.pointer = Some(seat.get_pointer(qh, ()));
                }
                if caps.contains(wl_seat::Capability::Keyboard) {
                    state.keyboard = Some(seat.get_keyboard(qh, ()));
                }
            }
        }
    }
}

wayland_client::delegate_noop!(GuiSession: wl_compositor::WlCompositor);
wayland_client::delegate_noop!(GuiSession: wl_surface::WlSurface);
wayland_client::delegate_noop!(GuiSession: wl_shm::WlShm);
wayland_client::delegate_noop!(GuiSession: wl_shm_pool::WlShmPool);
wayland_client::delegate_noop!(GuiSession: wl_buffer::WlBuffer);
wayland_client::delegate_noop!(GuiSession: wl_output::WlOutput);
wayland_client::delegate_noop!(GuiSession: zwlr_layer_shell_v1::ZwlrLayerShellV1);
