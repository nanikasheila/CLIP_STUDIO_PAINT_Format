//! Compare our render of .clip files with PNG exports of the same files
//! from CSP (full size). Prints, per file, the largest and mean channel
//! difference, the 99th percentile, the PSNR and what the renderer drew.
//! Colours are compared over white, so the colour under a fully
//! transparent pixel does not count; the alpha is compared as a fourth
//! channel.
//!
//! Usage: render_compare FILE.clip EXPORT.png [FILE.clip EXPORT.png …]

use csp_format::image::decode_png;
use csp_format::render::{RenderOptions, psnr, render};
use csp_format::{ClipFile, Result};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || !args.len().is_multiple_of(2) {
        eprintln!("usage: render_compare FILE.clip EXPORT.png [FILE.clip EXPORT.png …]");
        std::process::exit(2);
    }
    for pair in args.chunks(2) {
        let name = std::path::Path::new(&pair[0]).file_stem().and_then(|s| s.to_str()).unwrap_or("?");
        let file = ClipFile::open(&pair[0])?;
        let export = decode_png(&std::fs::read(&pair[1])?)?;
        let (image, report) = render(&file, RenderOptions::default())?;
        if (image.width, image.height) != (export.width, export.height) {
            println!("{name}: size {}x{} vs export {}x{}", image.width, image.height, export.width, export.height);
            continue;
        }
        let mut diffs: Vec<u8> = image
            .pixels
            .chunks_exact(4)
            .zip(export.pixels.chunks_exact(4))
            .flat_map(|(a, b)| {
                let (x, y) = (over_white(a), over_white(b));
                (0..4).map(move |c| x[c].abs_diff(y[c]))
            })
            .collect();
        let mean = diffs.iter().map(|&d| d as f64).sum::<f64>() / diffs.len() as f64;
        diffs.sort_unstable();
        let p99 = diffs[diffs.len() * 99 / 100];
        println!(
            "{name}: max {} mean {mean:.2} p99 {p99} psnr {:.1} corrections {} (approx {}) gradients {} (approx {})",
            diffs.last().copied().unwrap_or(0),
            psnr(&image, &export),
            report.correction_layers,
            report.corrections_approximate,
            report.gradient_layers,
            report.gradients_approximate
        );
    }
    Ok(())
}

/// A pixel's colour drawn over white, with its alpha.
fn over_white(p: &[u8]) -> [u8; 4] {
    let a = p[3] as u32;
    let mix = |v: u8| ((v as u32 * a + 255 * (255 - a) + 127) / 255) as u8;
    [mix(p[0]), mix(p[1]), mix(p[2]), p[3]]
}
