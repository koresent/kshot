pub mod scroll;
pub mod window;

pub use scroll::run_scroll_capture;
pub use window::run_window_capture;

use crate::error::{KshotError, Result};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use tiny_skia::Pixmap;

pub fn detect_focused_output() -> Option<String> {
    let uid = unsafe { libc::getuid() };
    let preferred = PathBuf::from(format!("/run/user/{uid}/umbriel-wayland-0.sock"));
    let sock_path = if preferred.exists() {
        Some(preferred)
    } else {
        let user_run = format!("/run/user/{uid}");
        if let Ok(entries) = fs::read_dir(user_run) {
            entries
                .flatten()
                .map(|e| e.path())
                .find(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .map(|s| s.starts_with("umbriel-") && s.ends_with(".sock"))
                        .unwrap_or(false)
                })
        } else {
            None
        }
    };

    if let Some(path) = sock_path {
        if let Ok(mut stream) = UnixStream::connect(path) {
            let _ = stream.set_read_timeout(Some(std::time::Duration::from_millis(300)));
            let _ = stream.set_write_timeout(Some(std::time::Duration::from_millis(300)));
            if stream.write_all(b"{\"cmd\":\"outputs\"}\n").is_ok() {
                let mut reader = BufReader::new(stream);
                let mut line = String::new();
                if reader.read_line(&mut line).is_ok() {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&line) {
                        if let Some(outputs) = val.get("ok").and_then(|v| v.as_array()) {
                            for o in outputs {
                                if o.get("focused").and_then(|f| f.as_bool()).unwrap_or(false) {
                                    if let Some(name) = o.get("name").and_then(|n| n.as_str()) {
                                        return Some(name.to_string());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    None
}

#[derive(Clone)]
pub struct CaptureFrame {
    pub width: u32,
    pub height: u32,
    pub pixmap: Pixmap,
}

impl CaptureFrame {
    pub fn new(width: u32, height: u32, pixmap: Pixmap) -> Self {
        Self {
            width,
            height,
            pixmap,
        }
    }

    /// Crop a sub-region into a new CaptureFrame
    pub fn crop(&self, x: i32, y: i32, w: u32, h: u32) -> Result<CaptureFrame> {
        if w == 0 || h == 0 {
            return Err(KshotError::InvalidGeometry("Crop dimensions must be positive".into()));
        }

        let clamped_x = x.clamp(0, self.width as i32) as u32;
        let clamped_y = y.clamp(0, self.height as i32) as u32;

        let available_w = (self.width.saturating_sub(clamped_x)).min(w);
        let available_h = (self.height.saturating_sub(clamped_y)).min(h);

        if available_w == 0 || available_h == 0 {
            return Err(KshotError::InvalidGeometry("Crop area falls outside frame bounds".into()));
        }

        let mut cropped = Pixmap::new(available_w, available_h)
            .ok_or_else(|| KshotError::CaptureFailed("Failed to allocate cropped pixmap".into()))?;

        let src_data = self.pixmap.data();
        let dst_data = cropped.data_mut();

        for row in 0..available_h {
            let src_start = (((clamped_y + row) * self.width + clamped_x) * 4) as usize;
            let src_end = src_start + (available_w * 4) as usize;
            let dst_start = ((row * available_w) * 4) as usize;
            let dst_end = dst_start + (available_w * 4) as usize;

            dst_data[dst_start..dst_end].copy_from_slice(&src_data[src_start..src_end]);
        }

        Ok(CaptureFrame::new(available_w, available_h, cropped))
    }

    /// Beautify the frame by applying soft shadow, rounded corners, and padding
    pub fn beautify(&mut self, radius: f32, padding: u32) -> Result<()> {
        let beautified = crate::render::beautify_pixmap(&self.pixmap, radius, padding)
            .ok_or_else(|| KshotError::CaptureFailed("Failed to beautify capture frame".into()))?;
        self.width = beautified.width();
        self.height = beautified.height();
        self.pixmap = beautified;
        Ok(())
    }

    /// Encode the frame to PNG bytes
    pub fn to_png_bytes(&self) -> Result<Vec<u8>> {
        self.pixmap
            .encode_png()
            .map_err(|e| KshotError::CaptureFailed(format!("Failed to encode PNG: {e}")))
    }
}

pub fn capture_screen(output_name: Option<&str>) -> Result<(CaptureFrame, Option<String>)> {
    let target_out = output_name.map(|s| s.to_string()).or_else(detect_focused_output);
    let mut cmd = Command::new("grim");
    if let Some(ref out) = target_out {
        cmd.args(["-o", out]);
    }
    cmd.args(["-t", "ppm", "-"]);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let child = cmd
        .spawn()
        .map_err(|e| KshotError::CaptureFailed(format!("Failed to spawn grim: {e}")))?;

    let output = child
        .wait_with_output()
        .map_err(|e| KshotError::CaptureFailed(format!("Failed to wait for grim: {e}")))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(KshotError::CaptureFailed(format!("grim failed: {err}")));
    }

    let frame = parse_ppm_to_frame(&output.stdout)?;
    Ok((frame, target_out))
}

pub fn normalize_geometry_for_grim(geometry: &str) -> String {
    let s = geometry.trim();
    if s.contains('+') {
        let parts: Vec<&str> = s.split('+').collect();
        if parts.len() == 3 {
            return format!("{},{} {}", parts[1], parts[2], parts[0]);
        }
    }
    s.to_string()
}

pub fn capture_geometry(geometry: &str) -> Result<CaptureFrame> {
    let normalized = normalize_geometry_for_grim(geometry);
    let mut cmd = Command::new("grim");
    cmd.args(["-g", &normalized, "-t", "ppm", "-"]);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let child = cmd
        .spawn()
        .map_err(|e| KshotError::CaptureFailed(format!("Failed to spawn grim: {e}")))?;

    let output = child
        .wait_with_output()
        .map_err(|e| KshotError::CaptureFailed(format!("Failed to wait for grim: {e}")))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(KshotError::CaptureFailed(format!("grim failed: {err}")));
    }

    parse_ppm_to_frame(&output.stdout)
}

/// Parses binary PPM (P6) into a tiny_skia Pixmap
pub fn parse_ppm_to_frame(ppm_bytes: &[u8]) -> Result<CaptureFrame> {
    if ppm_bytes.len() < 15 || &ppm_bytes[0..2] != b"P6" {
        return Err(KshotError::CaptureFailed("Invalid PPM header from capture".into()));
    }

    // Parse header: P6\n<w> <h>\n<maxval>\n<data>
    let mut idx = 2;

    fn skip_whitespace_and_comments(bytes: &[u8], idx: &mut usize) {
        while *idx < bytes.len() {
            let b = bytes[*idx];
            if b == b'#' {
                // Skip comment line
                while *idx < bytes.len() && bytes[*idx] != b'\n' {
                    *idx += 1;
                }
            } else if b.is_ascii_whitespace() {
                *idx += 1;
            } else {
                break;
            }
        }
    }

    skip_whitespace_and_comments(ppm_bytes, &mut idx);

    // Read width
    let start_w = idx;
    while idx < ppm_bytes.len() && ppm_bytes[idx].is_ascii_digit() {
        idx += 1;
    }
    let w_str = std::str::from_utf8(&ppm_bytes[start_w..idx])
        .map_err(|_| KshotError::CaptureFailed("Invalid width in PPM".into()))?;
    let width: u32 = w_str
        .parse()
        .map_err(|_| KshotError::CaptureFailed("Failed to parse PPM width".into()))?;

    skip_whitespace_and_comments(ppm_bytes, &mut idx);

    // Read height
    let start_h = idx;
    while idx < ppm_bytes.len() && ppm_bytes[idx].is_ascii_digit() {
        idx += 1;
    }
    let h_str = std::str::from_utf8(&ppm_bytes[start_h..idx])
        .map_err(|_| KshotError::CaptureFailed("Invalid height in PPM".into()))?;
    let height: u32 = h_str
        .parse()
        .map_err(|_| KshotError::CaptureFailed("Failed to parse PPM height".into()))?;

    skip_whitespace_and_comments(ppm_bytes, &mut idx);

    // Read maxval (typically 255)
    let start_max = idx;
    while idx < ppm_bytes.len() && ppm_bytes[idx].is_ascii_digit() {
        idx += 1;
    }
    let max_str = std::str::from_utf8(&ppm_bytes[start_max..idx])
        .map_err(|_| KshotError::CaptureFailed("Invalid maxval in PPM".into()))?;
    let maxval: u32 = max_str
        .parse()
        .map_err(|_| KshotError::CaptureFailed("Failed to parse PPM maxval".into()))?;

    if maxval != 255 {
        return Err(KshotError::CaptureFailed(format!("Unsupported PPM maxval: {maxval}")));
    }

    // Skip exactly one single whitespace character (usually '\n')
    if idx < ppm_bytes.len() && ppm_bytes[idx].is_ascii_whitespace() {
        idx += 1;
    }

    let rgb_data = &ppm_bytes[idx..];
    let expected_len = (width as usize) * (height as usize) * 3;
    if rgb_data.len() < expected_len {
        return Err(KshotError::CaptureFailed(format!(
            "PPM payload truncated: expected {expected_len} bytes, got {}",
            rgb_data.len()
        )));
    }

    let mut pixmap = Pixmap::new(width, height)
        .ok_or_else(|| KshotError::CaptureFailed("Failed to create tiny_skia Pixmap".into()))?;

    let pixels = pixmap.data_mut();

    // Fast conversion from RGB to RGBA premultiplied (alpha = 255, so RGB is identical)
    let mut src_i = 0;
    let mut dst_i = 0;
    let total_pixels = (width as usize) * (height as usize);

    for _ in 0..total_pixels {
        let r = rgb_data[src_i];
        let g = rgb_data[src_i + 1];
        let b = rgb_data[src_i + 2];

        pixels[dst_i] = r;
        pixels[dst_i + 1] = g;
        pixels[dst_i + 2] = b;
        pixels[dst_i + 3] = 255;

        src_i += 3;
        dst_i += 4;
    }

    Ok(CaptureFrame::new(width, height, pixmap))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ppm() {
        let ppm = b"P6\n2 2\n255\n\xFF\x00\x00\x00\xFF\x00\x00\x00\xFF\xFF\xFF\xFF";
        let frame = parse_ppm_to_frame(ppm).expect("Failed to parse PPM");
        assert_eq!(frame.width, 2);
        assert_eq!(frame.height, 2);

        let data = frame.pixmap.data();
        // Pixel (0,0) Red
        assert_eq!(&data[0..4], &[255, 0, 0, 255]);
        // Pixel (1,0) Green
        assert_eq!(&data[4..8], &[0, 255, 0, 255]);
        // Pixel (0,1) Blue
        assert_eq!(&data[8..12], &[0, 0, 255, 255]);
        // Pixel (1,1) White
        assert_eq!(&data[12..16], &[255, 255, 255, 255]);
    }

    #[test]
    fn test_normalize_geometry() {
        assert_eq!(normalize_geometry_for_grim("800x600+100+200"), "100,200 800x600");
        assert_eq!(normalize_geometry_for_grim("100,200 800x600"), "100,200 800x600");
    }

    #[test]
    fn test_crop_frame() {
        let ppm = b"P6\n2 2\n255\n\xFF\x00\x00\x00\xFF\x00\x00\x00\xFF\xFF\xFF\xFF";
        let frame = parse_ppm_to_frame(ppm).expect("Failed to parse PPM");
        let cropped = frame.crop(1, 1, 1, 1).expect("Failed to crop");
        assert_eq!(cropped.width, 1);
        assert_eq!(cropped.height, 1);
        assert_eq!(cropped.pixmap.data(), &[255, 255, 255, 255]);
    }
}
