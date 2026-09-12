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
    pub fn grayscale(&self, y: usize, height: usize) -> Vec<u8> {
        let start = self.offset + y * self.width * 3;
        let end = start + height * self.width * 3;
        grayscale(&self.bytes[start..end])
    }

    pub fn prepare(&self, y: usize, height: usize) -> Prepared {
        let mut gray = self.grayscale(y, height);
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

/// Fixed-point RGB conversion; every dispatch path produces identical bytes.
pub fn grayscale(rgb: &[u8]) -> Vec<u8> {
    let mut gray = vec![0; rgb.len() / 3];
    #[cfg(target_arch = "x86_64")]
    if std::arch::is_x86_feature_detected!("avx2") && std::arch::is_x86_feature_detected!("ssse3") {
        // SAFETY: runtime feature dispatch guards the target features; the
        // implementation only loads complete 48-byte blocks inside rgb.
        unsafe {
            grayscale_avx2(rgb, &mut gray);
        }
        return gray;
    }
    grayscale_scalar_into(rgb, &mut gray);
    gray
}

pub fn grayscale_scalar_into(rgb: &[u8], gray: &mut [u8]) {
    assert_eq!(rgb.len() / 3, gray.len());
    for (p, value) in rgb.as_chunks::<3>().0.iter().zip(gray) {
        *value = ((77 * u32::from(p[0]) + 150 * u32::from(p[1]) + 29 * u32::from(p[2])) >> 8) as u8;
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,ssse3")]
unsafe fn grayscale_avx2(rgb: &[u8], gray: &mut [u8]) {
    use std::arch::x86_64::*;
    let mut at = 0;
    // Shuffle three RGB blocks into planar channels, widening before the
    // weighted sum. 255*(77+150+29)=65280 fits exactly into unsigned u16.
    let r0 = _mm_setr_epi8(0, 3, 6, 9, 12, 15, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1);
    let r1 = _mm_setr_epi8(-1, -1, -1, -1, -1, -1, 2, 5, 8, 11, 14, -1, -1, -1, -1, -1);
    let r2 = _mm_setr_epi8(-1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, 1, 4, 7, 10, 13);
    let g0 = _mm_setr_epi8(1, 4, 7, 10, 13, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1);
    let g1 = _mm_setr_epi8(-1, -1, -1, -1, -1, 0, 3, 6, 9, 12, 15, -1, -1, -1, -1, -1);
    let g2 = _mm_setr_epi8(-1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, 2, 5, 8, 11, 14);
    let b0 = _mm_setr_epi8(2, 5, 8, 11, 14, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1);
    let b1 = _mm_setr_epi8(-1, -1, -1, -1, -1, 1, 4, 7, 10, 13, -1, -1, -1, -1, -1, -1);
    let b2 = _mm_setr_epi8(-1, -1, -1, -1, -1, -1, -1, -1, -1, -1, 0, 3, 6, 9, 12, 15);
    while at + 16 <= gray.len() {
        let ptr = rgb.as_ptr().add(at * 3);
        let a = _mm_loadu_si128(ptr.cast());
        let b = _mm_loadu_si128(ptr.add(16).cast());
        let c = _mm_loadu_si128(ptr.add(32).cast());
        let r = _mm256_cvtepu8_epi16(_mm_or_si128(
            _mm_or_si128(_mm_shuffle_epi8(a, r0), _mm_shuffle_epi8(b, r1)),
            _mm_shuffle_epi8(c, r2),
        ));
        let g = _mm256_cvtepu8_epi16(_mm_or_si128(
            _mm_or_si128(_mm_shuffle_epi8(a, g0), _mm_shuffle_epi8(b, g1)),
            _mm_shuffle_epi8(c, g2),
        ));
        let b = _mm256_cvtepu8_epi16(_mm_or_si128(
            _mm_or_si128(_mm_shuffle_epi8(a, b0), _mm_shuffle_epi8(b, b1)),
            _mm_shuffle_epi8(c, b2),
        ));
        let sum = _mm256_add_epi16(
            _mm256_add_epi16(
                _mm256_mullo_epi16(r, _mm256_set1_epi16(77)),
                _mm256_mullo_epi16(g, _mm256_set1_epi16(150)),
            ),
            _mm256_mullo_epi16(b, _mm256_set1_epi16(29)),
        );
        let value = _mm256_srli_epi16::<8>(sum);
        _mm_storeu_si128(
            gray.as_mut_ptr().add(at).cast(),
            _mm_packus_epi16(
                _mm256_castsi256_si128(value),
                _mm256_extracti128_si256::<1>(value),
            ),
        );
        at += 16;
    }
    grayscale_scalar_into(&rgb[at * 3..], &mut gray[at..]);
}

#[cfg(test)]
mod conversion_tests {
    use super::*;
    #[test]
    fn every_rgb_value_and_unaligned_tail_matches_scalar() {
        // Exhaustive RGB triples exercise every arithmetic value and channel
        // position; changing slice starts also covers unaligned SIMD loads.
        let mut rgb = Vec::with_capacity(256 * 256 * 256 * 3);
        for r in 0..=255u8 {
            for g in 0..=255u8 {
                for b in 0..=255u8 {
                    rgb.extend_from_slice(&[r, g, b]);
                }
            }
        }
        let mut expected = vec![0; rgb.len() / 3];
        grayscale_scalar_into(&rgb, &mut expected);
        assert_eq!(grayscale(&rgb), expected);
        for offset in 0..48 {
            for count in 0..65 {
                let slice = &rgb[offset..offset + count * 3];
                let mut expected = vec![0; count];
                grayscale_scalar_into(slice, &mut expected);
                assert_eq!(grayscale(slice), expected);
            }
        }
    }
}
