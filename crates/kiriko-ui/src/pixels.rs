//! Byte-level colour helpers shared by every path that hands sRGB pixels to
//! the GPU or a colour picker — deliberately ungated (the Project panel needs
//! a solid's swatch even in a media-free build).

pub fn srgb_encode(v: f32) -> u8 {
    let v = v.clamp(0.0, 1.0);
    let e = if v <= 0.003_130_8 {
        12.92 * v
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    };
    (e * 255.0).round() as u8
}

/// Inverse of [`srgb_encode`] (colour pickers hand back sRGB bytes).
pub fn srgb_decode(v: u8) -> f32 {
    let e = f32::from(v) / 255.0;
    if e <= 0.040_45 {
        e / 12.92
    } else {
        ((e + 0.055) / 1.055).powf(2.4)
    }
}

pub fn solid_rgba(c: kiriko_core::model::LinearColour) -> [u8; 4] {
    [
        srgb_encode(c.0[0]),
        srgb_encode(c.0[1]),
        srgb_encode(c.0[2]),
        (c.0[3].clamp(0.0, 1.0) * 255.0).round() as u8,
    ]
}

pub fn px_tile(px: &[u8; 4], w: u32, h: u32) -> Vec<u8> {
    std::iter::repeat_n(*px, (w * h) as usize)
        .flatten()
        .collect()
}

/// Contain-fit a `src_w × src_h` image inside `dst_w × dst_h`, keeping aspect
/// ratio: returns `(w, h, off_x, off_y)` — the scaled size and the top-left
/// offset that centres it (the black bars of a letterbox fill the rest).
pub fn fit_contain(src_w: u32, src_h: u32, dst_w: u32, dst_h: u32) -> (u32, u32, u32, u32) {
    if src_w == 0 || src_h == 0 || dst_w == 0 || dst_h == 0 {
        return (0, 0, 0, 0);
    }
    let scale = (f64::from(dst_w) / f64::from(src_w)).min(f64::from(dst_h) / f64::from(src_h));
    let w = ((f64::from(src_w) * scale).round() as u32).clamp(1, dst_w);
    let h = ((f64::from(src_h) * scale).round() as u32).clamp(1, dst_h);
    ((w), (h), (dst_w - w) / 2, (dst_h - h) / 2)
}

/// Bilinearly sample RGBA8 `src` (`w × h`) at continuous `(x, y)`, clamping to
/// the edges. Returns the four channels.
fn sample_bilinear(src: &[u8], w: u32, h: u32, x: f64, y: f64) -> [u8; 4] {
    let x = x.clamp(0.0, f64::from(w - 1));
    let y = y.clamp(0.0, f64::from(h - 1));
    let x0 = x.floor() as u32;
    let y0 = y.floor() as u32;
    let x1 = (x0 + 1).min(w - 1);
    let y1 = (y0 + 1).min(h - 1);
    let fx = x - f64::from(x0);
    let fy = y - f64::from(y0);
    let at = |px: u32, py: u32, c: usize| f64::from(src[((py * w + px) * 4) as usize + c]);
    let mut out = [0u8; 4];
    for (c, o) in out.iter_mut().enumerate() {
        let top = at(x0, y0, c) * (1.0 - fx) + at(x1, y0, c) * fx;
        let bot = at(x0, y1, c) * (1.0 - fx) + at(x1, y1, c) * fx;
        *o = (top * (1.0 - fy) + bot * fy).round().clamp(0.0, 255.0) as u8;
    }
    out
}

/// Resize RGBA8 `src` (`src_w × src_h`) into a fresh `dst_w × dst_h` RGBA8
/// frame, contain-fitted and centred on opaque black (letterbox). Used by the
/// export resolution presets; bilinear sampling, so it up- and down-scales.
/// Returns opaque black if `src` is too short for its stated size.
pub fn letterbox_resize(src: &[u8], src_w: u32, src_h: u32, dst_w: u32, dst_h: u32) -> Vec<u8> {
    let mut out = vec![0u8; (dst_w as usize) * (dst_h as usize) * 4];
    for px in out.chunks_exact_mut(4) {
        px[3] = 255; // opaque black background
    }
    let (w, h, ox, oy) = fit_contain(src_w, src_h, dst_w, dst_h);
    if w == 0 || h == 0 || src.len() < (src_w as usize) * (src_h as usize) * 4 {
        return out;
    }
    for y in 0..h {
        let sy = (f64::from(y) + 0.5) * f64::from(src_h) / f64::from(h) - 0.5;
        for x in 0..w {
            let sx = (f64::from(x) + 0.5) * f64::from(src_w) / f64::from(w) - 0.5;
            let px = sample_bilinear(src, src_w, src_h, sx, sy);
            let di = (((oy + y) * dst_w + (ox + x)) * 4) as usize;
            out[di..di + 4].copy_from_slice(&px);
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn fit_contain_letterboxes_and_pillarboxes() {
        // 16:9 into a tall 1080×1920 frame: full width, bars top and bottom.
        let (w, h, ox, oy) = fit_contain(1920, 1080, 1080, 1920);
        assert_eq!((w, ox), (1080, 0));
        assert_eq!(h, 608); // 1080 * 9/16 rounded
        assert_eq!(oy, (1920 - 608) / 2);
        // Exact multiple upscales cleanly, centred.
        assert_eq!(fit_contain(2, 2, 4, 4), (4, 4, 0, 0));
        // Degenerate inputs don't panic.
        assert_eq!(fit_contain(0, 0, 4, 4), (0, 0, 0, 0));
    }

    #[test]
    fn letterbox_puts_the_image_in_a_black_frame() {
        // A solid red 4×2 into a 2×2 target: contain scale 0.5 ⇒ 2×1, so the
        // top row is red and the bottom row is the black bar.
        let red = [255u8, 0, 0, 255];
        let src: Vec<u8> = red.iter().copied().cycle().take(4 * 2 * 4).collect();
        let out = letterbox_resize(&src, 4, 2, 2, 2);
        assert_eq!(&out[0..4], &red); // (0,0) red
        assert_eq!(&out[4..8], &red); // (1,0) red
        assert_eq!(&out[8..12], &[0, 0, 0, 255]); // (0,1) black bar
        assert_eq!(&out[12..16], &[0, 0, 0, 255]); // (1,1) black bar
    }

    #[test]
    fn letterbox_preserves_a_solid_colour() {
        let blue = [0u8, 0, 255, 255];
        let src: Vec<u8> = blue.iter().copied().cycle().take(2 * 2 * 4).collect();
        // Same aspect (square → square) fills the whole target with blue.
        let out = letterbox_resize(&src, 2, 2, 8, 8);
        for px in out.chunks_exact(4) {
            assert_eq!(px, &blue);
        }
    }
}
