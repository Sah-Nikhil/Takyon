//! Image clips as pixels: DIB in, PNG stored, DIB out (v0.14).
//!
//! Portable on purpose. The Win32 half only copies a `CF_DIB`/`CF_DIBV5` buffer
//! off the clipboard; parsing it, thumbnailing and PNG encoding live here so
//! every rule is testable without a clipboard. PNG is the stored form: lossless,
//! already a dependency, and what WebView2 draws.

/// Largest encoded image kept, in bytes. Past it the copy is skipped rather than
/// truncated, matching `watch::MAX_CHARS`: half a screenshot is worse than none.
pub const MAX_PNG_BYTES: usize = 32 * 1024 * 1024;

/// Largest image decoded at all, in pixels. 8K is 33 MP, so this refuses only
/// buffers no screen produces, before 4 bytes a pixel is allocated for them.
pub const MAX_PIXELS: u64 = 64 * 1024 * 1024;

/// Long edge of the thumbnail the list draws. Stored encrypted in the row, so
/// a list never decrypts a full blob.
pub const THUMB_EDGE: u32 = 256;

/// RGBA, 8 bits a channel, top row first, not premultiplied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

const BI_RGB: u32 = 0;
const BI_BITFIELDS: u32 = 3;

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// Parse a packed DIB: a `BITMAPINFOHEADER`, `V4` or `V5`, then pixels.
///
/// 24 and 32 bpp, `BI_RGB` or `BI_BITFIELDS`, either row order. Anything else
/// (palettes, 16 bpp, JPEG-in-DIB) is `None`: nothing that copies a screenshot
/// or a web image produces them. Reasoning for the alpha rule in [`opaque_if_blank`].
pub fn from_dib(dib: &[u8]) -> Option<Rgba> {
    let header = u32_at(dib, 0)? as usize;
    if header < 40 {
        return None;
    }
    let width = i32::from_le_bytes(dib.get(4..8)?.try_into().ok()?);
    let raw_height = i32::from_le_bytes(dib.get(8..12)?.try_into().ok()?);
    let bpp = u16_at(dib, 14)?;
    let compression = u32_at(dib, 16)?;
    let colours = u32_at(dib, 32)? as usize;

    let top_down = raw_height < 0;
    let height = raw_height.unsigned_abs();
    if width <= 0 || height == 0 || (width as u64) * (height as u64) > MAX_PIXELS {
        return None;
    }
    let width = width as u32;

    // Masks: inside a V4/V5 header, or three DWORDs after a 40-byte one.
    let (masks, after_masks) = match compression {
        BI_RGB => (None, header),
        BI_BITFIELDS if header >= 56 => (
            Some([
                u32_at(dib, 40)?,
                u32_at(dib, 44)?,
                u32_at(dib, 48)?,
                u32_at(dib, 52)?,
            ]),
            header,
        ),
        BI_BITFIELDS => (
            Some([u32_at(dib, 40)?, u32_at(dib, 44)?, u32_at(dib, 48)?, 0]),
            header + 12,
        ),
        _ => return None,
    };
    let bytes_per_pixel = match (bpp, masks) {
        (24, None) => 3,
        (32, _) => 4,
        _ => return None,
    };
    let start = after_masks + colours * 4;
    let stride = (width as usize * bytes_per_pixel).div_ceil(4) * 4;
    let data = dib.get(start..start + stride * height as usize)?;

    let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
    for y in 0..height as usize {
        let row = if top_down { y } else { height as usize - 1 - y };
        let line = &data[row * stride..row * stride + width as usize * bytes_per_pixel];
        for px in line.chunks_exact(bytes_per_pixel) {
            match masks {
                None => {
                    let a = if bytes_per_pixel == 4 { px[3] } else { 255 };
                    pixels.extend_from_slice(&[px[2], px[1], px[0], a]);
                }
                Some([r, g, b, a]) => {
                    let v = u32::from_le_bytes([px[0], px[1], px[2], px[3]]);
                    let alpha = if a == 0 { 255 } else { channel(v, a) };
                    pixels.extend_from_slice(&[channel(v, r), channel(v, g), channel(v, b), alpha]);
                }
            }
        }
    }
    let mut image = Rgba {
        width,
        height,
        pixels,
    };
    opaque_if_blank(&mut image);
    Some(image)
}

/// One channel out of a 32-bit pixel through its mask, scaled to 8 bits.
fn channel(v: u32, mask: u32) -> u8 {
    if mask == 0 {
        return 0;
    }
    let shifted = (v & mask) >> mask.trailing_zeros();
    let max = mask >> mask.trailing_zeros();
    ((shifted as u64 * 255) / max as u64) as u8
}

/// Every alpha zero means "no alpha", not "invisible".
///
/// `BI_RGB` 32 bpp leaves the fourth byte reserved and most writers zero it, so
/// a screenshot read literally is fully transparent. No real copy is wholly
/// transparent, so the all-zero case is the one safe to reinterpret.
fn opaque_if_blank(image: &mut Rgba) {
    if image.pixels.chunks_exact(4).all(|p| p[3] == 0) {
        for p in image.pixels.chunks_exact_mut(4) {
            p[3] = 255;
        }
    }
}

/// A packed `BITMAPINFOHEADER` DIB, 32 bpp `BI_RGB`, bottom-up.
///
/// The oldest shape there is, so every paste target reads it. Alpha rides in the
/// reserved byte; targets that want it read the `PNG` format written beside it.
pub fn to_dib(image: &Rgba) -> Vec<u8> {
    let (w, h) = (image.width as usize, image.height as usize);
    let mut out = Vec::with_capacity(40 + w * h * 4);
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(image.width as i32).to_le_bytes());
    out.extend_from_slice(&(image.height as i32).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&BI_RGB.to_le_bytes());
    out.extend_from_slice(&((w * h * 4) as u32).to_le_bytes());
    out.extend_from_slice(&[0u8; 16]);
    for y in (0..h).rev() {
        for p in image.pixels[y * w * 4..(y + 1) * w * 4].chunks_exact(4) {
            out.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
        }
    }
    out
}

/// PNG bytes, fast compression. A 4K screenshot is encoded on the watcher
/// thread, and `Fast` is several times quicker than default for a few % size.
pub fn encode_png(image: &Rgba) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, image.width, image.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Fast);
        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(&image.pixels).ok()?;
    }
    Some(out)
}

/// Decode a PNG this module wrote. Any 8-bit colour type is accepted, so an
/// older encoder setting still pastes back.
pub fn decode_png(bytes: &[u8]) -> Option<Rgba> {
    let mut decoder = png::Decoder::new(bytes);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    // Checked before allocating: a 100-byte PNG can claim 100k x 100k, and the
    // buffer below is sized from the header, not the data.
    let info = reader.info();
    if info.width == 0 || (info.width as u64) * (info.height as u64) > MAX_PIXELS {
        return None;
    }
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    buf.truncate(info.buffer_size());
    let pixels = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => buf
            .chunks_exact(2)
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Grayscale => buf.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return None,
    };
    Some(Rgba {
        width: info.width,
        height: info.height,
        pixels,
    })
}

/// Shrink so the long edge is at most `edge`, by area average. Never enlarges.
///
/// Colour is weighted by alpha, or a transparent pixel's hidden RGB bleeds into
/// its neighbours as a dark fringe.
pub fn thumbnail(image: &Rgba, edge: u32) -> Rgba {
    let long = image.width.max(image.height);
    if long <= edge {
        return image.clone();
    }
    let tw = ((image.width as u64 * edge as u64) / long as u64).max(1) as u32;
    let th = ((image.height as u64 * edge as u64) / long as u64).max(1) as u32;
    let (sw, sh) = (image.width as u64, image.height as u64);
    let mut pixels = Vec::with_capacity(tw as usize * th as usize * 4);
    for ty in 0..th as u64 {
        let (y0, y1) = (
            ty * sh / th as u64,
            ((ty + 1) * sh / th as u64).max(ty * sh / th as u64 + 1),
        );
        for tx in 0..tw as u64 {
            let (x0, x1) = (
                tx * sw / tw as u64,
                ((tx + 1) * sw / tw as u64).max(tx * sw / tw as u64 + 1),
            );
            let (mut r, mut g, mut b, mut a, mut n) = (0u64, 0u64, 0u64, 0u64, 0u64);
            for y in y0..y1 {
                for x in x0..x1 {
                    let i = ((y * sw + x) * 4) as usize;
                    let p = &image.pixels[i..i + 4];
                    let w = p[3] as u64;
                    r += p[0] as u64 * w;
                    g += p[1] as u64 * w;
                    b += p[2] as u64 * w;
                    a += w;
                    n += 1;
                }
            }
            let avg = |c: u64| c.checked_div(a).unwrap_or(0) as u8;
            pixels.extend_from_slice(&[avg(r), avg(g), avg(b), (a / n) as u8]);
        }
    }
    Rgba {
        width: tw,
        height: th,
        pixels,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checker(width: u32, height: u32) -> Rgba {
        let mut pixels = Vec::new();
        for y in 0..height {
            for x in 0..width {
                pixels.extend_from_slice(&[x as u8, y as u8, ((x + y) % 7) as u8 * 30, 200]);
            }
        }
        Rgba {
            width,
            height,
            pixels,
        }
    }

    /// Bottom-up DIB of `image` with a 40-byte header and 24 bpp, the shape
    /// Paint and older apps write.
    fn dib24(image: &Rgba) -> Vec<u8> {
        let stride = (image.width as usize * 3).div_ceil(4) * 4;
        let mut out = Vec::new();
        out.extend_from_slice(&40u32.to_le_bytes());
        out.extend_from_slice(&(image.width as i32).to_le_bytes());
        out.extend_from_slice(&(image.height as i32).to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&24u16.to_le_bytes());
        out.extend_from_slice(&[0u8; 24]);
        for y in (0..image.height as usize).rev() {
            let mut line = Vec::new();
            for x in 0..image.width as usize {
                let p = &image.pixels[(y * image.width as usize + x) * 4..][..4];
                line.extend_from_slice(&[p[2], p[1], p[0]]);
            }
            line.resize(stride, 0);
            out.extend_from_slice(&line);
        }
        out
    }

    #[test]
    fn v0_14_a_dib_round_trips_through_to_dib_and_from_dib() {
        let image = checker(5, 3);
        assert_eq!(from_dib(&to_dib(&image)), Some(image));
    }

    /// Odd width is what exercises the 4-byte row padding.
    #[test]
    fn v0_14_a_24_bit_dib_with_padded_rows_reads_opaque() {
        let mut image = checker(3, 2);
        for p in image.pixels.chunks_exact_mut(4) {
            p[3] = 255;
        }
        assert_eq!(from_dib(&dib24(&image)), Some(image));
    }

    /// The screenshot case: 32 bpp `BI_RGB` with every reserved byte zero. Read
    /// literally that is a fully transparent image.
    #[test]
    fn v0_14_a_32_bit_dib_with_zero_alpha_reads_opaque() {
        let mut image = checker(4, 4);
        for p in image.pixels.chunks_exact_mut(4) {
            p[3] = 0;
        }
        let read = from_dib(&to_dib(&image)).unwrap();
        assert!(read.pixels.chunks_exact(4).all(|p| p[3] == 255));
        assert_eq!(read.pixels[0..3], image.pixels[0..3]);
    }

    /// `CF_DIBV5`: 124-byte header, masks inside it, top-down rows.
    #[test]
    fn v0_14_a_v5_bitfields_top_down_dib_reads_its_masks() {
        let mut dib = vec![0u8; 124];
        dib[0..4].copy_from_slice(&124u32.to_le_bytes());
        dib[4..8].copy_from_slice(&2i32.to_le_bytes());
        dib[8..12].copy_from_slice(&(-1i32).to_le_bytes());
        dib[12..14].copy_from_slice(&1u16.to_le_bytes());
        dib[14..16].copy_from_slice(&32u16.to_le_bytes());
        dib[16..20].copy_from_slice(&BI_BITFIELDS.to_le_bytes());
        dib[40..44].copy_from_slice(&0x00FF_0000u32.to_le_bytes());
        dib[44..48].copy_from_slice(&0x0000_FF00u32.to_le_bytes());
        dib[48..52].copy_from_slice(&0x0000_00FFu32.to_le_bytes());
        dib[52..56].copy_from_slice(&0xFF00_0000u32.to_le_bytes());
        dib.extend_from_slice(&[10, 20, 30, 128, 1, 2, 3, 255]);
        let read = from_dib(&dib).unwrap();
        assert_eq!((read.width, read.height), (2, 1));
        assert_eq!(read.pixels, vec![30, 20, 10, 128, 3, 2, 1, 255]);
    }

    #[test]
    fn v0_14_truncated_or_unsupported_dibs_are_refused() {
        let dib = to_dib(&checker(4, 4));
        assert!(from_dib(&dib[..dib.len() - 1]).is_none());
        assert!(from_dib(&dib[..20]).is_none());
        let mut paletted = dib.clone();
        paletted[14..16].copy_from_slice(&8u16.to_le_bytes());
        assert!(from_dib(&paletted).is_none());
    }

    /// The pixel cap refuses before allocating. A header claiming 100k x 100k
    /// would otherwise ask for 40 GB.
    #[test]
    fn v0_14_a_dib_claiming_absurd_dimensions_is_refused() {
        let mut dib = to_dib(&checker(1, 1));
        dib[4..8].copy_from_slice(&100_000i32.to_le_bytes());
        dib[8..12].copy_from_slice(&100_000i32.to_le_bytes());
        assert!(from_dib(&dib).is_none());
    }

    /// A valid header claiming a huge image, with no data behind it, must be
    /// refused before the output buffer is allocated.
    #[test]
    fn v0_14_a_png_claiming_absurd_dimensions_is_refused() {
        let mut png = encode_png(&checker(1, 1)).unwrap();
        // IHDR width and height at bytes 16..24, CRC over 12..29 at 29..33.
        png[16..20].copy_from_slice(&100_000u32.to_be_bytes());
        png[20..24].copy_from_slice(&100_000u32.to_be_bytes());
        let crc = crc32(&png[12..29]);
        png[29..33].copy_from_slice(&crc.to_be_bytes());
        assert!(
            png::Decoder::new(&png[..]).read_info().is_ok(),
            "header must parse"
        );
        assert!(decode_png(&png).is_none());
    }

    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = !0u32;
        for &b in bytes {
            crc ^= b as u32;
            for _ in 0..8 {
                crc = if crc & 1 == 1 {
                    (crc >> 1) ^ 0xEDB8_8320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }

    #[test]
    fn v0_14_png_round_trips_losslessly() {
        let image = checker(17, 9);
        let png = encode_png(&image).unwrap();
        assert_eq!(decode_png(&png), Some(image));
    }

    #[test]
    fn v0_14_a_thumbnail_keeps_aspect_and_never_enlarges() {
        let thumb = thumbnail(&checker(1000, 500), THUMB_EDGE);
        assert_eq!((thumb.width, thumb.height), (256, 128));
        assert_eq!(thumb.pixels.len(), 256 * 128 * 4);
        let small = checker(10, 20);
        assert_eq!(thumbnail(&small, THUMB_EDGE), small);
    }

    /// A transparent pixel's hidden colour must not bleed into the average.
    #[test]
    fn v0_14_a_thumbnail_ignores_colour_under_transparent_pixels() {
        let image = Rgba {
            width: 2,
            height: 1,
            pixels: vec![255, 0, 0, 255, 0, 255, 0, 0],
        };
        let thumb = thumbnail(&image, 1);
        assert_eq!(thumb.pixels, vec![255, 0, 0, 127]);
    }
}
