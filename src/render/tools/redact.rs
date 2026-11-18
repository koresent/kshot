use tiny_skia::Pixmap;

pub fn apply_pixelate(pixmap: &mut Pixmap, x: i32, y: i32, w: u32, h: u32, block_size: u32) {
    if w == 0 || h == 0 || block_size == 0 {
        return;
    }

    let px_w = pixmap.width();
    let px_h = pixmap.height();

    let start_x = x.clamp(0, px_w as i32) as u32;
    let start_y = y.clamp(0, px_h as i32) as u32;
    let end_x = (x + w as i32).clamp(0, px_w as i32) as u32;
    let end_y = (y + h as i32).clamp(0, px_h as i32) as u32;

    if start_x >= end_x || start_y >= end_y {
        return;
    }

    let data = pixmap.data_mut();

    let mut by = start_y;
    while by < end_y {
        let block_h = block_size.min(end_y - by);

        let mut bx = start_x;
        while bx < end_x {
            let block_w = block_size.min(end_x - bx);

            // Compute average color in block
            let mut sum_r: u64 = 0;
            let mut sum_g: u64 = 0;
            let mut sum_b: u64 = 0;
            let mut sum_a: u64 = 0;
            let count = (block_w * block_h) as u64;

            for dy in 0..block_h {
                for dx in 0..block_w {
                    let idx = (((by + dy) * px_w + (bx + dx)) * 4) as usize;
                    sum_r += data[idx] as u64;
                    sum_g += data[idx + 1] as u64;
                    sum_b += data[idx + 2] as u64;
                    sum_a += data[idx + 3] as u64;
                }
            }

            let avg_r = (sum_r / count) as u8;
            let avg_g = (sum_g / count) as u8;
            let avg_b = (sum_b / count) as u8;
            let avg_a = (sum_a / count) as u8;

            // Fill block with average color
            for dy in 0..block_h {
                for dx in 0..block_w {
                    let idx = (((by + dy) * px_w + (bx + dx)) * 4) as usize;
                    data[idx] = avg_r;
                    data[idx + 1] = avg_g;
                    data[idx + 2] = avg_b;
                    data[idx + 3] = avg_a;
                }
            }

            bx += block_size;
        }
        by += block_size;
    }
}

pub fn apply_blur(pixmap: &mut Pixmap, x: i32, y: i32, w: u32, h: u32, radius: usize) {
    if w == 0 || h == 0 || radius == 0 {
        return;
    }

    let px_w = pixmap.width();
    let px_h = pixmap.height();

    let start_x = x.clamp(0, px_w as i32) as u32;
    let start_y = y.clamp(0, px_h as i32) as u32;
    let end_x = (x + w as i32).clamp(0, px_w as i32) as u32;
    let end_y = (y + h as i32).clamp(0, px_h as i32) as u32;

    if start_x >= end_x || start_y >= end_y {
        return;
    }

    let sub_w = (end_x - start_x) as usize;
    let sub_h = (end_y - start_y) as usize;

    let data = pixmap.data_mut();

    // Extract subregion RGBA into temporary buffer
    let mut buffer = vec![0u8; sub_w * sub_h * 4];
    for row in 0..sub_h {
        let src_idx = ((start_y as usize + row) * px_w as usize + start_x as usize) * 4;
        let dst_idx = row * sub_w * 4;
        buffer[dst_idx..dst_idx + sub_w * 4].copy_from_slice(&data[src_idx..src_idx + sub_w * 4]);
    }

    // Horizontal pass
    let mut temp = buffer.clone();
    let r = radius as i32;

    for y in 0..sub_h {
        for x in 0..sub_w {
            let mut sum_r = 0;
            let mut sum_g = 0;
            let mut sum_b = 0;
            let mut count = 0;

            for kx in (x as i32 - r)..=(x as i32 + r) {
                if kx >= 0 && (kx as usize) < sub_w {
                    let idx = (y * sub_w + kx as usize) * 4;
                    sum_r += buffer[idx] as u32;
                    sum_g += buffer[idx + 1] as u32;
                    sum_b += buffer[idx + 2] as u32;
                    count += 1;
                }
            }

            let idx = (y * sub_w + x) * 4;
            temp[idx] = (sum_r / count) as u8;
            temp[idx + 1] = (sum_g / count) as u8;
            temp[idx + 2] = (sum_b / count) as u8;
        }
    }

    // Vertical pass
    for y in 0..sub_h {
        for x in 0..sub_w {
            let mut sum_r = 0;
            let mut sum_g = 0;
            let mut sum_b = 0;
            let mut count = 0;

            for ky in (y as i32 - r)..=(y as i32 + r) {
                if ky >= 0 && (ky as usize) < sub_h {
                    let idx = (ky as usize * sub_w + x) * 4;
                    sum_r += temp[idx] as u32;
                    sum_g += temp[idx + 1] as u32;
                    sum_b += temp[idx + 2] as u32;
                    count += 1;
                }
            }

            let idx = (y * sub_w + x) * 4;
            buffer[idx] = (sum_r / count) as u8;
            buffer[idx + 1] = (sum_g / count) as u8;
            buffer[idx + 2] = (sum_b / count) as u8;
        }
    }

    // Copy blurred pixels back to pixmap
    for row in 0..sub_h {
        let dst_idx = ((start_y as usize + row) * px_w as usize + start_x as usize) * 4;
        let src_idx = row * sub_w * 4;
        data[dst_idx..dst_idx + sub_w * 4].copy_from_slice(&buffer[src_idx..src_idx + sub_w * 4]);
    }
}

pub fn apply_invert(pixmap: &mut Pixmap, x: i32, y: i32, w: u32, h: u32) {
    if w == 0 || h == 0 {
        return;
    }

    let px_w = pixmap.width();
    let px_h = pixmap.height();

    let start_x = x.clamp(0, px_w as i32) as u32;
    let start_y = y.clamp(0, px_h as i32) as u32;
    let end_x = (x + w as i32).clamp(0, px_w as i32) as u32;
    let end_y = (y + h as i32).clamp(0, px_h as i32) as u32;

    let data = pixmap.data_mut();

    for row in start_y..end_y {
        for col in start_x..end_x {
            let idx = ((row * px_w + col) * 4) as usize;
            data[idx] = 255 - data[idx];
            data[idx + 1] = 255 - data[idx + 1];
            data[idx + 2] = 255 - data[idx + 2];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invert() {
        let mut p = Pixmap::new(2, 2).unwrap();
        p.fill(tiny_skia::Color::from_rgba8(255, 0, 0, 255));
        apply_invert(&mut p, 0, 0, 2, 2);
        assert_eq!(&p.data()[0..4], &[0, 255, 255, 255]);
    }
}
