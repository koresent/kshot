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
use std::time::Duration;

fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = Config::load_or_create();

    match cli.command.unwrap_or(Commands::Gui(GuiArgs::default())) {
        Commands::Gui(gui_args) => {
            if gui_args.delay > 0 {
                std::thread::sleep(Duration::from_millis(gui_args.delay));
            }
            let frame = capture::capture_screen(None)?;
            wayland::run_gui(frame, config, gui_args)?;
        }

        Commands::Screen(screen_args) => {
            if screen_args.delay > 0 {
                std::thread::sleep(Duration::from_millis(screen_args.delay));
            }
            let frame = capture::capture_screen(screen_args.output.as_deref())?;

            if screen_args.edit {
                let gui_args = GuiArgs {
                    path: screen_args.path,
                    clipboard: screen_args.clipboard,
                    delay: 0,
                    region: None,
                    raw: false,
                    pin: false,
                    accept_on_select: false,
                };
                wayland::run_gui(frame, config, gui_args)?;
            } else {
                let png_bytes = frame.to_png_bytes()?;
                if screen_args.clipboard {
                    copy_png_to_clipboard(&png_bytes)?;
                    if config.general.show_notifications {
                        send_clipboard_notification();
                    }
                }
                if screen_args.path.is_some() || !screen_args.clipboard {
                    save_image_to_file(&png_bytes, screen_args.path.as_deref(), &config)?;
                }
            }
        }

        Commands::Area(area_args) => {
            let frame = capture::capture_geometry(&area_args.geometry)?;
            let png_bytes = frame.to_png_bytes()?;
            if area_args.clipboard {
                copy_png_to_clipboard(&png_bytes)?;
                if config.general.show_notifications {
                    send_clipboard_notification();
                }
            }
            if area_args.path.is_some() || !area_args.clipboard {
                save_image_to_file(&png_bytes, area_args.path.as_deref(), &config)?;
            }
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
