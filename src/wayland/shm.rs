use std::fs::File;
use std::os::fd::{AsFd, AsRawFd, FromRawFd};
use wayland_client::protocol::{wl_buffer, wl_shm, wl_shm_pool};
use wayland_client::{Dispatch, QueueHandle};

pub struct ShmBuffer {
    pub buffer: wl_buffer::WlBuffer,
    pub mmap_ptr: *mut u8,
    pub size: usize,
    #[allow(dead_code)]
    pub width: u32,
    #[allow(dead_code)]
    pub height: u32,
    #[allow(dead_code)]
    pub stride: u32,
}

unsafe impl Send for ShmBuffer {}
unsafe impl Sync for ShmBuffer {}

impl Drop for ShmBuffer {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(self.mmap_ptr as *mut libc::c_void, self.size);
        }
    }
}

pub fn create_shm_buffer<T>(
    shm: &wl_shm::WlShm,
    qh: &QueueHandle<T>,
    width: u32,
    height: u32,
) -> Result<ShmBuffer, String>
where
    T: Dispatch<wl_shm_pool::WlShmPool, ()> + Dispatch<wl_buffer::WlBuffer, ()> + 'static,
{
    let stride = width * 4;
    let size = (stride * height) as usize;

    let fd_raw = unsafe {
        let name = std::ffi::CString::new("kshot-shm").map_err(|e| e.to_string())?;
        libc::memfd_create(name.as_ptr(), libc::MFD_CLOEXEC)
    };

    if fd_raw < 0 {
        return Err("Failed to create memfd".into());
    }

    let file = unsafe { File::from_raw_fd(fd_raw) };
    file.set_len(size as u64)
        .map_err(|e| format!("Failed to truncate shm memfd: {e}"))?;

    let mmap_ptr = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            size,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_SHARED,
            file.as_raw_fd(),
            0,
        )
    };

    if mmap_ptr == libc::MAP_FAILED {
        return Err("mmap failed on shm memfd".into());
    }

    let pool = shm.create_pool(file.as_fd(), size as i32, qh, ());
    let buffer = pool.create_buffer(
        0,
        width as i32,
        height as i32,
        stride as i32,
        wl_shm::Format::Argb8888,
        qh,
        (),
    );

    pool.destroy();

    Ok(ShmBuffer {
        buffer,
        mmap_ptr: mmap_ptr as *mut u8,
        size,
        width,
        height,
        stride,
    })
}

/// Converts and copies tiny-skia RGBA bytes to Wayland wl_shm ARGB8888 (native little-endian BGRA).
#[inline]
pub fn copy_rgba_to_argb8888(src_rgba: &[u8], dst_ptr: *mut u8, num_pixels: usize) {
    let src_u32 = unsafe { std::slice::from_raw_parts(src_rgba.as_ptr() as *const u32, num_pixels) };
    let dst_u32 = unsafe { std::slice::from_raw_parts_mut(dst_ptr as *mut u32, num_pixels) };
    for (s, d) in src_u32.iter().zip(dst_u32.iter_mut()) {
        let px = *s;
        let r = px & 0xFF;
        let g = (px >> 8) & 0xFF;
        let b = (px >> 16) & 0xFF;
        let a = (px >> 24) & 0xFF;
        *d = (a << 24) | (r << 16) | (g << 8) | b;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_copy_rgba_to_argb8888() {
        // Red, Green, Blue, Alpha
        let rgba = [0xb8u8, 0xbbu8, 0x26u8, 0xffu8]; // Gruvbox green
        let mut dst = [0u8; 4];
        copy_rgba_to_argb8888(&rgba, dst.as_mut_ptr(), 1);

        // Wayland ARGB8888 little endian: [B, G, R, A]
        assert_eq!(dst[0], 0x26); // Blue
        assert_eq!(dst[1], 0xbb); // Green
        assert_eq!(dst[2], 0xb8); // Red
        assert_eq!(dst[3], 0xff); // Alpha
    }
}
