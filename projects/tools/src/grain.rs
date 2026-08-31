use crate::Result;
use std::fs::File;
use std::io::BufWriter;

pub fn run(arguments: &[String]) -> Result {
    let target = arguments.first().ok_or("usage: seele-grain <output.png>")?;
    const SIZE: usize = 128;
    let mut state: u32 = 0x5ee1e0;
    let mut pixels = vec![0_u8; SIZE * SIZE];
    for pixel in &mut pixels {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        *pixel = state as u8;
    }
    let file = File::create(target)?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), SIZE as u32, SIZE as u32);
    encoder.set_color(png::ColorType::Grayscale);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Best);
    encoder.write_header()?.write_image_data(&pixels)?;
    Ok(())
}
