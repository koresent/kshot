use crate::error::{KshotError, Result};
use std::io::Write;
use std::process::{Command, Stdio};

pub fn copy_png_to_clipboard(png_bytes: &[u8]) -> Result<()> {
    let mut child = Command::new("wl-copy")
        .args(["-t", "image/png"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| KshotError::CaptureFailed(format!("Failed to spawn wl-copy: {e}")))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(png_bytes)
            .map_err(|e| KshotError::CaptureFailed(format!("Failed to write to wl-copy stdin: {e}")))?;
        drop(stdin);
    }

    let status = child
        .wait()
        .map_err(|e| KshotError::CaptureFailed(format!("Failed to wait for wl-copy: {e}")))?;

    if !status.success() {
        return Err(KshotError::CaptureFailed(
            "wl-copy exited with error".into(),
        ));
    }

    Ok(())
}
