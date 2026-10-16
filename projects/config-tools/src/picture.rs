//! Whether a file will draw as a wallpaper.
//!
//! The desktop's Qt decodes PNG and JPEG without an image-format plugin, and
//! refuses to allocate more than 256 MiB for one decoded image. A file that is
//! anything else, is cut short or damaged, or is that large, draws as nothing,
//! so a theme folder naming one is refused when it registers instead.
//! Registration decodes a PNG completely and walks a JPEG's structure to its
//! end; a later catalog read only rechecks the signature of the registered
//! copy, which the store keeps as it was registered.
use std::{
    fs,
    io::{BufReader, Read},
    path::Path,
};

/// QImageReader's default allocation limit in Qt 6.
const DECODED_LIMIT: u64 = 256 * 1024 * 1024;
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n";
const JPEG: &[u8] = &[0xff, 0xd8, 0xff];

/// Whether `file` starts as a PNG or a JPEG does.
pub fn signed(file: &Path) -> bool {
    let mut head = Vec::with_capacity(PNG.len());
    fs::File::open(file)
        .and_then(|file| file.take(PNG.len() as u64).read_to_end(&mut head))
        .is_ok()
        && (head == PNG || head.starts_with(JPEG))
}

/// Why `file` would not draw as a wallpaper, or nothing if it would.
pub fn fault(file: &Path) -> Option<String> {
    let Ok(bytes) = fs::read(file) else {
        return Some("cannot be read".to_owned());
    };
    if bytes.starts_with(PNG) {
        png(file).err()
    } else if bytes.starts_with(JPEG) {
        jpeg(&bytes).err()
    } else {
        Some("is not a PNG or JPEG image".to_owned())
    }
}

/// Qt holds a decoded image at four bytes a pixel, eight for 16-bit PNG.
/// Grey images take less, so this errs towards refusing.
fn within_limit(width: u32, height: u32, bytes_per_pixel: u64) -> Result<(), String> {
    if width == 0 || height == 0 {
        return Err("is an image with no pixels".to_owned());
    }
    if u64::from(width) * u64::from(height) * bytes_per_pixel > DECODED_LIMIT {
        return Err(format!(
            "is {width}×{height} pixels, more than the 256 MiB Qt decodes one image into"
        ));
    }
    Ok(())
}

/// Decodes every row, which checks each chunk's CRC and the compressed data.
fn png(file: &Path) -> Result<(), String> {
    let damaged = |error: png::DecodingError| format!("is not a complete PNG image ({error})");
    let source = fs::File::open(file).map_err(|_| "cannot be read".to_owned())?;
    let mut reader = png::Decoder::new(BufReader::new(source))
        .read_info()
        .map_err(damaged)?;
    let info = reader.info();
    let depth = if info.bit_depth == png::BitDepth::Sixteen {
        8
    } else {
        4
    };
    within_limit(info.width, info.height, depth)?;
    while reader.next_row().map_err(damaged)?.is_some() {}
    reader.finish().map_err(damaged)
}

/// Walks the markers from SOI to EOI: every segment has to fit, a frame header
/// has to give the size before the first scan, and the entropy-coded data of
/// each scan has to end in a marker rather than at the end of the file.
fn jpeg(bytes: &[u8]) -> Result<(), String> {
    let cut = || Err("is not a complete JPEG image".to_owned());
    let mut index = 2;
    let mut frame = None;
    let mut scans = 0;
    loop {
        if bytes.get(index) != Some(&0xff) {
            return cut();
        }
        while bytes.get(index) == Some(&0xff) {
            index += 1;
        }
        let Some(&marker) = bytes.get(index) else {
            return cut();
        };
        index += 1;
        match marker {
            0xd9 => {
                return match frame {
                    Some((width, height)) if scans > 0 => within_limit(width, height, 4),
                    _ => cut(),
                }
            }
            0x01 | 0xd0..=0xd7 => continue,
            _ => {}
        }
        let Some(length) = bytes
            .get(index..index + 2)
            .map(|pair| usize::from(u16::from_be_bytes([pair[0], pair[1]])))
        else {
            return cut();
        };
        let Some(segment) = bytes.get(index + 2..index + length).filter(|_| length >= 2) else {
            return cut();
        };
        index += length;
        match marker {
            // Start of frame, every coding but the table and arithmetic
            // conditioning markers that share the range.
            0xc0..=0xcf if !matches!(marker, 0xc4 | 0xc8 | 0xcc) => {
                if segment.len() < 6 {
                    return cut();
                }
                let height = u32::from(u16::from_be_bytes([segment[1], segment[2]]));
                let width = u32::from(u16::from_be_bytes([segment[3], segment[4]]));
                within_limit(width, height, 4)?;
                frame = Some((width, height));
            }
            0xda => {
                if frame.is_none() {
                    return cut();
                }
                scans += 1;
                // Entropy-coded data runs to the next marker that is neither a
                // stuffed 0xff00 nor a restart.
                loop {
                    let Some(offset) = bytes[index..].iter().position(|byte| *byte == 0xff) else {
                        return cut();
                    };
                    index += offset;
                    match bytes.get(index + 1) {
                        Some(0x00 | 0xd0..=0xd7) => index += 2,
                        Some(0xff) => index += 1,
                        Some(_) => break,
                        None => return cut(),
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::Write;

    /// A complete RGB PNG of `width` by `height`, every row the same colour.
    pub fn png_image(width: u32, height: u32) -> Vec<u8> {
        png_chunks(width, height, &deflate_stored(width, height))
    }
    /// A PNG whose header says `width` by `height` around the given data.
    fn png_chunks(width: u32, height: u32, data: &[u8]) -> Vec<u8> {
        let mut out = PNG.to_vec();
        let mut chunk = |kind: &[u8], data: &[u8]| {
            out.extend_from_slice(&(data.len() as u32).to_be_bytes());
            let mut body = kind.to_vec();
            body.extend_from_slice(data);
            out.extend_from_slice(&body);
            out.extend_from_slice(&crc(&body).to_be_bytes());
        };
        let mut header = Vec::new();
        header.extend_from_slice(&width.to_be_bytes());
        header.extend_from_slice(&height.to_be_bytes());
        header.extend_from_slice(&[8, 2, 0, 0, 0]);
        chunk(b"IHDR", &header);
        chunk(b"IDAT", data);
        chunk(b"IEND", b"");
        out
    }
    fn crc(bytes: &[u8]) -> u32 {
        let mut crc = !0u32;
        for byte in bytes {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                crc = if crc & 1 == 1 {
                    (crc >> 1) ^ 0xedb8_8320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }
    /// The raw rows as a zlib stream of stored blocks, so no encoder is needed.
    fn deflate_stored(width: u32, height: u32) -> Vec<u8> {
        // Each row is filter type 0, then the same pixel across.
        let mut row = vec![0];
        row.extend([0x40, 0x80, 0xc0].repeat(width as usize));
        let raw = row.repeat(height as usize);
        let mut out = vec![0x78, 0x01];
        let blocks: Vec<_> = raw.chunks(65535).collect();
        for (index, block) in blocks.iter().enumerate() {
            out.push(u8::from(index + 1 == blocks.len()));
            let length = block.len() as u16;
            out.extend_from_slice(&length.to_le_bytes());
            out.extend_from_slice(&(!length).to_le_bytes());
            out.extend_from_slice(block);
        }
        let (mut a, mut b) = (1u32, 0u32);
        for byte in &raw {
            a = (a + u32::from(*byte)) % 65521;
            b = (b + a) % 65521;
        }
        out.extend_from_slice(&((b << 16) | a).to_be_bytes());
        out
    }
    /// A structurally complete baseline JPEG of `width` by `height`: frame
    /// header, one scan whose data holds a stuffed byte and a restart, EOI.
    pub fn jpeg_image(width: u16, height: u16) -> Vec<u8> {
        let mut out = vec![0xff, 0xd8, 0xff, 0xe0, 0, 16];
        out.extend_from_slice(b"JFIF\0\x01\x01\0\0\x01\0\x01\0\0");
        out.extend_from_slice(&[0xff, 0xc0, 0, 11, 8]);
        out.extend_from_slice(&height.to_be_bytes());
        out.extend_from_slice(&width.to_be_bytes());
        out.extend_from_slice(&[1, 1, 0x11, 0]);
        out.extend_from_slice(&[0xff, 0xda, 0, 8, 1, 1, 0, 0, 0x3f, 0]);
        out.extend_from_slice(&[0x12, 0xff, 0x00, 0x34, 0xff, 0xd0, 0x56]);
        out.extend_from_slice(&[0xff, 0xd9]);
        out
    }
    fn fault_of(bytes: &[u8]) -> Option<String> {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(bytes).unwrap();
        fault(file.path())
    }

    #[test]
    fn a_complete_png_or_jpeg_draws() {
        assert_eq!(fault_of(&png_image(16, 9)), None);
        assert_eq!(
            fault_of(&png_image(300, 300)),
            None,
            "several deflate blocks"
        );
        assert_eq!(fault_of(&jpeg_image(1920, 1080)), None);
    }

    #[test]
    fn a_png_cut_short_or_damaged_is_refused() {
        let image = png_image(32, 18);
        for cut in [9, 33, image.len() / 2, image.len() - 12, image.len() - 1] {
            let fault = fault_of(&image[..cut]).unwrap();
            assert!(
                fault.starts_with("is not a complete PNG image"),
                "{cut}: {fault}"
            );
        }
        let mut damaged = image.clone();
        let middle = damaged.len() / 2;
        damaged[middle] ^= 0x55;
        assert!(fault_of(&damaged)
            .unwrap()
            .starts_with("is not a complete PNG image"));
    }

    #[test]
    fn a_jpeg_cut_short_or_without_a_frame_is_refused() {
        let image = jpeg_image(64, 36);
        for cut in [
            3,
            8,
            22,
            30,
            image.len() - 6,
            image.len() - 2,
            image.len() - 1,
        ] {
            assert_eq!(
                fault_of(&image[..cut]).as_deref(),
                Some("is not a complete JPEG image"),
                "{cut}"
            );
        }
        let frame = image.iter().position(|byte| *byte == 0xc0).unwrap() - 1;
        let mut frameless = image[..frame].to_vec();
        frameless.extend_from_slice(&image[frame + 13..]);
        assert_eq!(
            fault_of(&frameless).as_deref(),
            Some("is not a complete JPEG image")
        );
        assert_eq!(
            fault_of(&jpeg_image(0, 36)).as_deref(),
            Some("is an image with no pixels")
        );
    }

    #[test]
    fn an_image_larger_than_qt_decodes_is_refused() {
        assert_eq!(fault_of(&jpeg_image(8192, 8192)), None, "exactly 256 MiB");
        assert_eq!(
            fault_of(&jpeg_image(8200, 8200)).as_deref(),
            Some("is 8200×8200 pixels, more than the 256 MiB Qt decodes one image into")
        );
        // The size is read from the header, before any row is decoded.
        assert_eq!(
            fault_of(&png_chunks(8200, 8200, &deflate_stored(1, 1))).as_deref(),
            Some("is 8200×8200 pixels, more than the 256 MiB Qt decodes one image into")
        );
    }

    #[test]
    fn anything_else_is_not_an_image() {
        for bytes in [
            &b"GIF89a"[..],
            b"RIFF\0\0\0\0WEBPVP8 ",
            b"\x89PN",
            b"",
            b"text\n",
        ] {
            assert_eq!(
                fault_of(bytes).as_deref(),
                Some("is not a PNG or JPEG image"),
                "{bytes:?}"
            );
            let mut file = tempfile::NamedTempFile::new().unwrap();
            file.write_all(bytes).unwrap();
            assert!(!signed(file.path()));
        }
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(&jpeg_image(4, 4)).unwrap();
        assert!(signed(file.path()));
    }
}
