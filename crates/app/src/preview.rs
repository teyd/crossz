use std::sync::Arc;

use crossz_core::{Crosshair, Scale, render};
use gpui_kit::RenderImage;
use image::{Frame, RgbaImage};

const MAX_ZOOM: u32 = 8;
const DARK: [u8; 3] = [0x1c, 0x20, 0x26];
const LIGHT: [u8; 3] = [0x9a, 0xa0, 0xa8];

/// The crosshair exactly as the overlay draws it at `scale`, on a `size`-pixel square,
/// magnified by a whole number so every screen pixel stays a crisp square. The top
/// half shows it on a dark background and the bottom half on a light one.
pub fn crosshair(crosshair: &Crosshair, scale: Scale, size: u32) -> (Arc<RenderImage>, u32) {
    let image = render(crosshair, scale);
    let zoom = (size / image.width.max(image.height)).clamp(1, MAX_ZOOM);
    let origin_x = size as i64 / 2 - (image.width / 2 * zoom) as i64;
    let origin_y = size as i64 / 2 - (image.height / 2 * zoom) as i64;

    // RenderImage holds BGRA despite the RgbaImage type.
    let mut bgra = RgbaImage::new(size, size);
    for (x, y, pixel) in bgra.enumerate_pixels_mut() {
        let [r, g, b] = if y < size / 2 { DARK } else { LIGHT };
        let (cx, cy) = (x as i64 - origin_x, y as i64 - origin_y);
        let inside = (0..(image.width * zoom) as i64).contains(&cx)
            && (0..(image.height * zoom) as i64).contains(&cy);
        let [sb, sg, sr, sa] = if inside {
            image.pixel(cx as u32 / zoom, cy as u32 / zoom)
        } else {
            [0; 4]
        };
        // Source is premultiplied: out = src + bg * (1 - src_alpha).
        let over =
            |src: u8, bg: u8| src.saturating_add((bg as u32 * (255 - sa as u32) / 255) as u8);
        pixel.0 = [over(sb, b), over(sg, g), over(sr, r), 255];
    }
    (Arc::new(RenderImage::new([Frame::new(bgra)])), zoom)
}

/// A program icon from straight-alpha BGRA pixels.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn icon(width: u32, height: u32, bgra: Vec<u8>) -> Option<Arc<RenderImage>> {
    let buffer = RgbaImage::from_raw(width, height, bgra)?;
    Some(Arc::new(RenderImage::new([Frame::new(buffer)])))
}
