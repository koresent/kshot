mod capture;
mod cli;
mod config;
mod error;
mod export;
mod pin;
mod render;
mod ui;
mod wayland;

use clap::Parser;
use cli::{Cli, Commands, GuiArgs};
use config::Config;
use error::Result;
use export::{copy_png_to_clipboard, save_image_to_file, send_clipboard_notification};
use std::process::Command;
use std::time::Duration;
use ui::hud::HudAction;

fn export_capture_frame(
    mut frame: capture::CaptureFrame,
    pretty: bool,
    clipboard: bool,
    path: Option<&std::path::Path>,
    config: &Config,
) -> Result<()> {
    if pretty {
        frame.beautify(12.0, 40)?;
    }
    let png_bytes = frame.to_png_bytes()?;
    if clipboard {
        copy_png_to_clipboard(&png_bytes)?;
        if config.general.show_notifications {
            send_clipboard_notification();
        }
    }
    if path.is_some() || !clipboard {
        save_image_to_file(&png_bytes, path, config)?;
    }
    Ok(())
}

fn execute_hud_action(action: HudAction, pretty: bool, config: &Config) -> Result<()> {
    match action {
        HudAction::Area => {
            let (mut frame, target_output) = capture::capture_screen(None)?;
            if pretty {
                frame.beautify(12.0, 40)?;
            }
            let gui_args = GuiArgs {
                pretty,
                ..Default::default()
            };
            wayland::run_gui(frame, target_output, config.clone(), gui_args)?;
        }
        HudAction::Window => {
            if let Some(frame) = capture::run_window_capture(None)? {
                export_capture_frame(frame, pretty, true, None, config)?;
            }
        }
        HudAction::Screen => {
            let (frame, _) = capture::capture_screen(None)?;
            export_capture_frame(frame, pretty, true, None, config)?;
        }
        HudAction::Scroll => {
            if let Some(frame) = capture::run_scroll_capture(None)? {
                export_capture_frame(frame, pretty, true, None, config)?;
            }
        }
        HudAction::Picker => {
            let _ = Command::new("cpick").spawn();
        }
        HudAction::Ruler => {
            let _ = Command::new("ruler").spawn();
        }
        HudAction::Scanner => {
            let _ = Command::new("qscan").spawn();
        }
        HudAction::Ocr => {
            let _ = Command::new("ocr-screen").spawn();
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = Config::load_or_create();

    match cli.command.unwrap_or(Commands::Gui(GuiArgs::default())) {
        Commands::Gui(gui_args) => {
            if gui_args.delay > 0 {
                std::thread::sleep(Duration::from_millis(gui_args.delay));
            }
            let (mut frame, target_output) = capture::capture_screen(None)?;
            if gui_args.pretty {
                frame.beautify(12.0, 40)?;
            }
            wayland::run_gui(frame, target_output, config, gui_args)?;
        }

        Commands::Hud(hud_args) => {
            if let Some(action) = ui::run_hud(hud_args.pretty)? {
                execute_hud_action(action, hud_args.pretty, &config)?;
            }
        }

        Commands::Window(win_args) => {
            if let Some(frame) = capture::run_window_capture(None)? {
                if win_args.edit {
                    let gui_args = GuiArgs {
                        path: win_args.path,
                        clipboard: win_args.clipboard,
                        pretty: win_args.pretty,
                        ..Default::default()
                    };
                    wayland::run_gui(frame, None, config, gui_args)?;
                } else {
                    export_capture_frame(
                        frame,
                        win_args.pretty,
                        win_args.clipboard,
                        win_args.path.as_deref(),
                        &config,
                    )?;
                }
            }
        }

        Commands::Scroll(scroll_args) => {
            if let Some(frame) = capture::run_scroll_capture(scroll_args.geometry.as_deref())? {
                if scroll_args.edit {
                    let gui_args = GuiArgs {
                        path: scroll_args.path,
                        clipboard: !scroll_args.no_copy,
                        pretty: scroll_args.pretty,
                        ..Default::default()
                    };
                    wayland::run_gui(frame, None, config, gui_args)?;
                } else {
                    export_capture_frame(
                        frame,
                        scroll_args.pretty,
                        !scroll_args.no_copy,
                        scroll_args.path.as_deref(),
                        &config,
                    )?;
                }
            }
        }

        Commands::Screen(screen_args) => {
            if screen_args.delay > 0 {
                std::thread::sleep(Duration::from_millis(screen_args.delay));
            }
            let (frame, target_output) = capture::capture_screen(screen_args.output.as_deref())?;

            if screen_args.edit {
                let gui_args = GuiArgs {
                    path: screen_args.path,
                    clipboard: screen_args.clipboard,
                    delay: 0,
                    region: None,
                    raw: false,
                    pin: false,
                    accept_on_select: false,
                    pretty: screen_args.pretty,
                };
                wayland::run_gui(frame, target_output, config, gui_args)?;
            } else {
                export_capture_frame(
                    frame,
                    screen_args.pretty,
                    screen_args.clipboard,
                    screen_args.path.as_deref(),
                    &config,
                )?;
            }
        }

        Commands::Area(area_args) => {
            let frame = capture::capture_geometry(&area_args.geometry)?;
            export_capture_frame(
                frame,
                area_args.pretty,
                area_args.clipboard,
                area_args.path.as_deref(),
                &config,
            )?;
        }

        Commands::Pin(pin_args) => {
            let img = image::open(&pin_args.file)?;
            let rgba = img.to_rgba8();
            let (w, h) = rgba.dimensions();
            let mut pixmap = tiny_skia::Pixmap::new(w, h).ok_or_else(|| {
                error::KshotError::CaptureFailed("Failed to allocate pixmap for pinned image".into())
            })?;
            pixmap.data_mut().copy_from_slice(rgba.as_raw());

            let frame = capture::CaptureFrame::new(w, h, pixmap);
            pin::run_pin_window(frame)?;
        }

        Commands::Config(config_args) => {
            let path = Config::config_path();
            if config_args.path {
                println!("{}", path.display());
            } else if path.exists() {
                let content = std::fs::read_to_string(&path)?;
                println!("{content}");
            } else {
                let yaml = serde_yaml::to_string(&config)?;
                println!("{yaml}");
            }
        }
    }

    Ok(())
}
