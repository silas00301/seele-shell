#![allow(dead_code, unused_imports)]
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
#[path = "../src/uri/image.rs"]
mod image;
use std::{
    hint::black_box,
    time::{Duration, Instant},
};
fn measure(mut run: impl FnMut()) -> Duration {
    let mut values = Vec::new();
    for _ in 0..9 {
        let start = Instant::now();
        for _ in 0..40 {
            run();
        }
        values.push(start.elapsed() / 40);
    }
    values.sort();
    values[4]
}
fn main() {
    for (width, height) in [(1920, 640), (3840, 640), (3840, 2160)] {
        let rgb: Vec<u8> = (0..width * height * 3)
            .map(|i| ((i * 37 + i / 311) % 256) as u8)
            .collect();
        let scalar = measure(|| {
            let mut gray = vec![0; rgb.len() / 3];
            image::grayscale_scalar_into(black_box(&rgb), &mut gray);
            black_box(gray);
        });
        let dispatched = measure(|| {
            black_box(image::grayscale(black_box(&rgb)));
        });
        println!(
            "{width}x{height}: scalar={scalar:?}, dispatch={dispatched:?}, speedup={:.2}x",
            scalar.as_secs_f64() / dispatched.as_secs_f64()
        );
    }
}
