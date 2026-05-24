//! Headless stipple raster for bake and atlas output.

use crate::Dot;

/// Fill `buf` (RGBA8 row-major, `width` × `height`) with dots.
///
/// Dot coordinates live in the same space as [`crate::Params::canvas`] width/height.
/// Radius follows the stipple convention used in repo sketch `10-curves-3d-demo`.
pub fn raster_dots(
    buf: &mut [u8],
    width: u32,
    height: u32,
    canvas_w: f32,
    canvas_h: f32,
    dots: &[Dot],
    background: [u8; 4],
) {
    assert_eq!(buf.len(), (width * height * 4) as usize);
    for px in buf.chunks_exact_mut(4) {
        px.copy_from_slice(&background);
    }
    for dot in dots {
        let dim = canvas_w.max(canvas_h);
        let sz = dot.r * dim * 2.0;
        let half = (sz * 0.5).max(0.5);
        let cx = (dot.x / canvas_w * width as f32).round() as i32;
        let cy = (dot.y / canvas_h * height as f32).round() as i32;
        let r = (half / canvas_w * width as f32).ceil().max(1.0) as i32;
        let v = (35.0 + dot.w * 210.0).min(255.0) as u8;
        let rgba = [v, v, v.saturating_add(6), 255];
        blit_rect(buf, width, height, cx - r, cy - r, cx + r, cy + r, rgba);
    }
}

fn blit_rect(
    buf: &mut [u8],
    width: u32,
    height: u32,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    rgba: [u8; 4],
) {
    let w = width as i32;
    let h = height as i32;
    let x0 = x0.max(0).min(w);
    let x1 = x1.max(0).min(w);
    let y0 = y0.max(0).min(h);
    let y1 = y1.max(0).min(h);
    for y in y0..y1 {
        let row = (y as u32 * width) as usize * 4;
        for x in x0..x1 {
            let i = row + x as usize * 4;
            buf[i..i + 4].copy_from_slice(&rgba);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{generate_dots, Params};

    #[test]
    fn raster_non_empty_for_default_params() {
        let mut p = Params::default();
        p.canvas.width = 256.0;
        p.canvas.height = 256.0;
        p.distribution.dot_count = 500;
        let dots = generate_dots(&p, 0.0);
        let mut buf = vec![0u8; 256 * 256 * 4];
        raster_dots(&mut buf, 256, 256, 256.0, 256.0, &dots, [0, 0, 0, 255]);
        assert!(buf.iter().any(|&b| b > 0));
    }
}
