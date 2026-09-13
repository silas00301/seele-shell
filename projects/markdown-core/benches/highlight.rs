use seele_markdown_core::highlight;
use std::{hint::black_box, time::Instant};

fn main() {
    for (name, text) in [
        ("typical", "# Heading **bold** [[🦀]] and `code`".to_owned()),
        ("backticks", format!("x{}", "`".repeat(131000))),
        ("stars", format!("x{}", "*".repeat(131000))),
        ("code", "`x` ".repeat(30000)),
        ("wiki", "[[x]] ".repeat(21000)),
        ("unclosed", "**x ".repeat(30000)),
    ] {
        let units: Vec<_> = text.encode_utf16().collect();
        let start = Instant::now();
        let output = highlight(black_box(&units), 0, false);
        println!(
            "{name}: {:?}, {} spans",
            start.elapsed(),
            black_box(output.spans.len())
        );
    }
}
