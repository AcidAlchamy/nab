//! PNG and JPEG files from a shot.

use super::Shot;
use crate::config::Format;
use std::path::Path;

/// Bytes per pixel in the order the file wants (R, G, B[, A]).
fn rgb_bytes(px: &[u32], alpha: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(px.len() * if alpha { 4 } else { 3 });
    for &p in px {
        out.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, p as u8]);
        if alpha {
            out.push((p >> 24) as u8);
        }
    }
    out
}

/// A PNG. `fast` trades a little size for speed (the clipboard copy).
pub fn png(shot: &Shot, fast: bool) -> std::io::Result<Vec<u8>> {
    let alpha = shot.has_alpha();
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, shot.w as u32, shot.h as u32);
        enc.set_color(if alpha { png::ColorType::Rgba } else { png::ColorType::Rgb });
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(if fast { png::Compression::Fast } else { png::Compression::Balanced });
        let mut writer = enc.write_header().map_err(std::io::Error::other)?;
        writer.write_image_data(&rgb_bytes(&shot.px, alpha)).map_err(std::io::Error::other)?;
        writer.finish().map_err(std::io::Error::other)?;
    }
    Ok(out)
}

/// A JPEG. Transparent corners become white.
pub fn jpg(shot: &Shot, quality: u8) -> std::io::Result<Vec<u8>> {
    if shot.w > u16::MAX as i32 || shot.h > u16::MAX as i32 {
        return Err(std::io::Error::other("too big for a JPEG; use PNG"));
    }
    let mut out = Vec::new();
    let enc = jpeg_encoder::Encoder::new(&mut out, quality.clamp(50, 100));
    enc.encode(
        &rgb_bytes(&shot.flattened(0xFFFFFF), false),
        shot.w as u16,
        shot.h as u16,
        jpeg_encoder::ColorType::Rgb,
    )
    .map_err(std::io::Error::other)?;
    Ok(out)
}

pub fn encode(shot: &Shot, format: Format, quality: u8) -> std::io::Result<Vec<u8>> {
    match format {
        Format::Png => png(shot, false),
        Format::Jpg => jpg(shot, quality),
    }
}

/// Writes the file, making folders as needed. Written to a temporary name
/// first, so a half-written picture never sits under the real name.
pub fn write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("nab-partial");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

/// Reads a PNG into a shot (for the demo pictures).
#[cfg(test)]
pub fn load_png(path: &Path) -> Shot {
    let mut dec = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path).unwrap()));
    dec.set_transformations(png::Transformations::EXPAND);
    let mut reader = dec.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    let n = info.color_type.samples();
    let px = buf[..info.buffer_size()]
        .chunks(n)
        .map(|c| {
            let a = if n == 4 { c[3] as u32 } else { 255 };
            (a << 24) | (c[0] as u32) << 16 | (c[1] as u32) << 8 | c[2] as u32
        })
        .collect();
    Shot { w: info.width as i32, h: info.height as i32, px }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A nab as it comes out: the mock window with Windows 11's corners made clear.
    #[test]
    #[ignore = "writes README art; needs NAB_DEMO"]
    fn demo_result() {
        let dir = std::path::PathBuf::from(std::env::var("NAB_DEMO").expect("NAB_DEMO=<folder with mock-window.png>"));
        let mut shot = load_png(&dir.join("mock-window.png"));
        shot.round_corners(8.0);
        std::fs::write(dir.join("result.png"), png(&shot, false).unwrap()).unwrap();
    }

    fn checker(w: i32, h: i32) -> Shot {
        let px = (0..w * h).map(|i| if (i % w + i / w) % 2 == 0 { 0xFFFF_2E7E } else { 0xFF0D_0B14 }).collect();
        Shot { w, h, px }
    }

    #[test]
    fn png_round_trips_pixels() {
        let mut shot = checker(37, 21);
        shot.round_corners(6.0);
        let bytes = png(&shot, true).unwrap();
        let mut dec = png::Decoder::new(std::io::Cursor::new(bytes)).read_info().unwrap();
        let mut buf = vec![0; dec.output_buffer_size().unwrap()];
        let info = dec.next_frame(&mut buf).unwrap();
        assert_eq!((info.width, info.height), (37, 21));
        assert_eq!(info.color_type, png::ColorType::Rgba);
        let at = |x: usize, y: usize| &buf[(y * 37 + x) * 4..(y * 37 + x) * 4 + 4];
        assert_eq!(at(10, 10), [0xFF, 0x2E, 0x7E, 0xFF]);
        assert_eq!(at(11, 10), [0x0D, 0x0B, 0x14, 0xFF]);
        assert_eq!(at(0, 0)[3], 0);
    }

    #[test]
    fn opaque_png_is_rgb_and_jpeg_is_a_jpeg() {
        let shot = checker(64, 48);
        let bytes = png(&shot, false).unwrap();
        let dec = png::Decoder::new(std::io::Cursor::new(bytes)).read_info().unwrap();
        assert_eq!(dec.info().color_type, png::ColorType::Rgb);
        let j = jpg(&shot, 90).unwrap();
        assert_eq!(&j[..3], &[0xFF, 0xD8, 0xFF]);
    }

    #[test]
    fn write_makes_folders() {
        let dir = std::env::temp_dir().join(format!("nab-save-test-{}", std::process::id()));
        let path = dir.join("a").join("b.png");
        write(&path, b"hello").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"hello");
        assert!(!path.with_extension("nab-partial").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
