use crate::config::Config;
use crate::error::Result;
use chrono::Local;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn save_image_to_file(
    png_bytes: &[u8],
    custom_path: Option<&Path>,
    config: &Config,
) -> Result<PathBuf> {
    let target_path = if let Some(p) = custom_path {
        if p.is_dir() {
            let filename = Local::now().format(&config.general.filename_pattern).to_string();
            p.join(filename)
        } else {
            p.to_path_buf()
        }
    } else {
        let dir = config.expanded_save_path();
        let filename = Local::now().format(&config.general.filename_pattern).to_string();
        dir.join(filename)
    };

    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::write(&target_path, png_bytes)?;

    if config.general.show_notifications {
        send_save_notification(&target_path);
    }

    Ok(target_path)
}

fn send_save_notification(path: &Path) {
    let msg = format!("Saved to {}", path.display());
    let _ = Command::new("notify-send")
        .args([
            "-a",
            "kshot",
            "-i",
            "camera-photo",
            "Screenshot Saved",
            &msg,
        ])
        .status();
}

pub fn send_clipboard_notification() {
    let _ = Command::new("notify-send")
        .args([
            "-a",
            "kshot",
            "-i",
            "camera-photo",
            "Screenshot Copied",
            "Captured region copied to clipboard",
        ])
        .status();
}
