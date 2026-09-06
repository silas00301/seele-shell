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
