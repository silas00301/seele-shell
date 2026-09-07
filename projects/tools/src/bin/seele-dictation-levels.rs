//! Read Voxtype 0.7's native 16-byte audio frames without opening another mic.
use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;

fn level(frame: &[u8; 16]) -> f32 {
    let min = f32::from_ne_bytes(frame[4..8].try_into().unwrap());
    let max = f32::from_ne_bytes(frame[8..12].try_into().unwrap());
    if !min.is_finite() || !max.is_finite() {
        return 0.0;
    }
    min.abs().max(max.abs()).clamp(0.0, 1.0).sqrt()
}

fn run() -> io::Result<()> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")
        .ok_or_else(|| io::Error::other("XDG_RUNTIME_DIR is not set"))?;
    let path = std::path::PathBuf::from(runtime).join("voxtype/audio.sock");
    let alive = Arc::new(AtomicBool::new(true));
    let reader_alive = alive.clone();
    std::thread::spawn(move || {
        let _ = io::copy(&mut io::stdin(), &mut io::sink());
        reader_alive.store(false, Ordering::Relaxed);
    });
    let mut out = io::stdout().lock();
    while alive.load(Ordering::Relaxed) {
        let Ok(mut socket) = UnixStream::connect(&path) else {
            std::thread::sleep(Duration::from_millis(200));
            continue;
        };
        socket.set_read_timeout(Some(Duration::from_millis(250)))?;
        let mut frame = [0; 16];
        let (mut filled, mut count, mut peak) = (0, 0, 0.0_f32);
        while alive.load(Ordering::Relaxed) {
            match socket.read(&mut frame[filled..]) {
                Ok(0) => break,
                Ok(size) => {
                    filled += size;
                    if filled != frame.len() {
                        continue;
                    }
                    filled = 0;
                    peak = peak.max(level(&frame));
                    count += 1;
                    // Drain every frame but update QML at 20 Hz.
                    if count == 5 {
                        writeln!(out, "{peak}")?;
                        out.flush()?;
                        count = 0;
                        peak = 0.0;
                    }
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock
                            | io::ErrorKind::TimedOut
                            | io::ErrorKind::Interrupted
                    ) => {}
                Err(_) => break,
            }
        }
        writeln!(out, "0")?;
        out.flush()?;
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        if error.kind() != io::ErrorKind::BrokenPipe {
            eprintln!("seele-dictation-levels: {error}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decodes_native_frames_and_rejects_nonfinite_samples() {
        let mut frame = [0; 16];
        frame[4..8].copy_from_slice(&(-0.25_f32).to_ne_bytes());
        frame[8..12].copy_from_slice(&(0.1_f32).to_ne_bytes());
        assert_eq!(level(&frame), 0.5);
        frame[4..8].copy_from_slice(&f32::NAN.to_ne_bytes());
        assert_eq!(level(&frame), 0.0);
    }
}
