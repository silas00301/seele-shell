use super::Result;

/// Grim's P6 output avoids a PNG encode/decode round trip. Keep the RGB pixels
/// in one allocation shared by every OCR strip; Qt reads the same PPM file.
pub struct Image {
    pub bytes: Vec<u8>,
    pub offset: usize,
    pub width: usize,
    pub height: usize,
}

impl Image {
    pub fn parse(bytes: Vec<u8>) -> Result<Self> {
        let mut at = 0;
        let mut token = || -> Result<&[u8]> {
            loop {
                while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
                    at += 1;
                }
                if bytes.get(at) != Some(&b'#') {
                    break;
                }
                while bytes.get(at).is_some_and(|b| *b != b'\n') {
                    at += 1;
                }
            }
            let start = at;
            while bytes.get(at).is_some_and(|b| !b.is_ascii_whitespace()) {
                at += 1;
            }
            if start == at {
                return Err("incomplete PPM header".into());
            }
            Ok(&bytes[start..at])
        };
        if token()? != b"P6" {
            return Err("capture is not a binary RGB PPM".into());
        }
        let width = std::str::from_utf8(token()?)?.parse::<usize>()?;
        let height = std::str::from_utf8(token()?)?.parse::<usize>()?;
        if token()? != b"255" || !bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
            return Err("unsupported PPM sample depth".into());
        }
        // Exactly one delimiter: a first pixel can itself be whitespace.
        at += 1;
        let size = width.checked_mul(height).and_then(|n| n.checked_mul(3));
        if width == 0
            || height == 0
            || width > 32768
            || height > 32768
            || size != Some(bytes.len() - at)
        {
            return Err("invalid capture dimensions or pixel data".into());
        }
        Ok(Self {
            bytes,
            offset: at,
            width,
            height,
        })
    }
}

/// A modest enlargement makes small desktop glyphs legible to the LSTM.
/// Grayscale reduces copy traffic; normalizing dark backgrounds avoids an
/// extra inverted recognition pass. The original capture remains untouched.
pub struct Prepared {
    pub bytes: Vec<u8>,
    pub width: usize,
    pub height: usize,
}

impl Image {
    pub fn prepare(&self, y: usize, height: usize) -> Prepared {
        let start = self.offset + y * self.width * 3;
        let end = start + height * self.width * 3;
        let mut gray: Vec<u8> = self.bytes[start..end]
            .chunks_exact(3)
            .map(|p| {
                ((77 * u32::from(p[0]) + 150 * u32::from(p[1]) + 29 * u32::from(p[2])) >> 8) as u8
            })
            .collect();
        let samples = gray.len().div_ceil(32);
        if gray.iter().step_by(32).filter(|v| **v < 128).count() > samples / 2 {
            for pixel in &mut gray {
                *pixel = 255 - *pixel;
            }
        }
        let width = self.width * 3 / 2;
        let scaled_height = height * 3 / 2;
        let mut bytes = Vec::with_capacity(width * scaled_height);
        // Bilinear interpolation at pixel centers, using integer weights.
        // A 3:2 scale keeps the input small and maps boxes back exactly.
        for row in 0..scaled_height {
            let sy = (row * 4).saturating_sub(1);
            let top = sy / 6;
            let bottom = (top + 1).min(height - 1);
            let fy = (sy % 6) as u32;
            for column in 0..width {
                let sx = (column * 4).saturating_sub(1);
                let left = sx / 6;
                let right = (left + 1).min(self.width - 1);
                let fx = (sx % 6) as u32;
                let a = u32::from(gray[top * self.width + left]) * (6 - fx)
                    + u32::from(gray[top * self.width + right]) * fx;
                let b = u32::from(gray[bottom * self.width + left]) * (6 - fx)
                    + u32::from(gray[bottom * self.width + right]) * fx;
                bytes.push(((a * (6 - fy) + b * fy + 18) / 36) as u8);
            }
        }
        Prepared {
            bytes,
            width,
            height: scaled_height,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ppm_keeps_whitespace_valued_pixels() {
        let image = Image::parse(b"P6\n# grim\n2 1\n255\n\n \t\0\xff\x80".to_vec()).unwrap();
        assert_eq!((image.width, image.height), (2, 1));
        assert_eq!(&image.bytes[image.offset..], b"\n \t\0\xff\x80");
    }

    #[test]
    fn rejects_truncation_and_oversized_dimensions() {
        for data in [
            "P6\n1 1\n255\nx",
            "P6\n999999999999999999 2\n255\n",
            "P3\n1 1\n255\n",
        ] {
            assert!(Image::parse(data.as_bytes().to_vec()).is_err());
        }
    }
}
