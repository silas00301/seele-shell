use crate::Result;
use std::fs::File;
use std::io::BufWriter;

/// The tile the shell lays under its chrome. It is wider than the panels it
/// repeats across so the pattern never announces itself, and it is built from
/// two octaves rather than from raw noise: a fine one, drawn as the mean of
/// several samples so most pixels sit near mid grey and only a few sparkle,
/// and a coarse one that clumps those pixels the way film grain clumps. Both
/// wrap, so the tile is seamless in either direction.
const SIZE: usize = 192;
const CELL: usize = 3;
const FINE_SAMPLES: u32 = 3;
const FINE_WEIGHT: f32 = 0.72;
const COARSE_WEIGHT: f32 = 0.34;

struct Noise {
    state: u32,
}

impl Noise {
    fn new(seed: u32) -> Self {
        Noise { state: seed }
    }

    fn next(&mut self) -> u32 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 17;
        self.state ^= self.state << 5;
        self.state
    }

    fn sample(&mut self) -> f32 {
        (self.next() & 0xff) as f32
    }
}

fn coarse_field(noise: &mut Noise) -> Vec<f32> {
    let cells = SIZE / CELL;
    let mut field = vec![0.0_f32; cells * cells];
    for value in &mut field {
        *value = noise.sample();
    }
    // One wrapping box pass, so a clump fades into its neighbours instead of
    // ending on a cell boundary the eye can follow.
    let mut smoothed = vec![0.0_f32; cells * cells];
    for y in 0..cells {
        for x in 0..cells {
            let mut total = 0.0;
            for dy in 0..3 {
                for dx in 0..3 {
                    let sy = (y + cells + dy - 1) % cells;
                    let sx = (x + cells + dx - 1) % cells;
                    total += field[sy * cells + sx];
                }
            }
            smoothed[y * cells + x] = total / 9.0;
        }
    }
    smoothed
}

pub fn run(arguments: &[String]) -> Result {
    let target = arguments.first().ok_or("usage: seele-grain <output.png>")?;
    let mut noise = Noise::new(0x5ee1e0);
    let coarse = coarse_field(&mut noise);
    let cells = SIZE / CELL;
    let mut pixels = vec![0_u8; SIZE * SIZE];
    for y in 0..SIZE {
        for x in 0..SIZE {
            let mut fine = 0.0;
            for _ in 0..FINE_SAMPLES {
                fine += noise.sample();
            }
            fine /= FINE_SAMPLES as f32;
            let clump = coarse[(y / CELL) * cells + (x / CELL)];
            let value = 128.0 + (fine - 128.0) * FINE_WEIGHT + (clump - 128.0) * COARSE_WEIGHT;
            pixels[y * SIZE + x] = value.clamp(0.0, 255.0) as u8;
        }
    }
    let file = File::create(target)?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), SIZE as u32, SIZE as u32);
    encoder.set_color(png::ColorType::Grayscale);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Best);
    encoder.write_header()?.write_image_data(&pixels)?;
    Ok(())
}
