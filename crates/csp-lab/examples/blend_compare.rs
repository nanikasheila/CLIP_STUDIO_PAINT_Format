//! Compare our render of a `blend_test` file with CSP's own merged export,
//! cell by cell (one cell per blending mode; see `blend_test.rs`).
//!
//! Usage: cargo run --example blend_compare -- TEST.clip CSP_EXPORT.png
//!
//! For each mode: the largest per-channel difference and the mean absolute
//! difference, separately for the opaque (top) and half-transparent
//! (bottom) half of the cell.

use csp_format::render::{BlendMode, RenderOptions, render};
use csp_format::{ClipFile, LayerKind, Result, RgbaImage};

const COLUMNS: u32 = 6;

fn stats(a: &RgbaImage, b: &RgbaImage, x0: u32, y0: u32, x1: u32, y1: u32) -> (u8, f64) {
    let (mut max, mut sum, mut n) = (0u8, 0u64, 0u64);
    for y in y0..y1 {
        for x in x0..x1 {
            let (p, q) = (a.pixel(x, y), b.pixel(x, y));
            for c in 0..3 {
                let d = p[c].abs_diff(q[c]);
                max = max.max(d);
                sum += d as u64;
                n += 1;
            }
        }
    }
    (max, sum as f64 / n.max(1) as f64)
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [clip, png] = args.as_slice() else {
        eprintln!("usage: blend_compare TEST.clip CSP_EXPORT.png");
        std::process::exit(2);
    };
    let file = ClipFile::open(clip)?;
    let expected = RgbaImage::from_png(&std::fs::read(png)?)?;
    let (ours, _) = render(&file, RenderOptions { scale: 1.0, size: Some((expected.width, expected.height)) })?;
    let (width, height) = (file.canvas.width_px, file.canvas.height_px);
    let rasters: Vec<_> = file
        .layers
        .flatten()
        .into_iter()
        .filter(|(layer, _)| layer.kind == LayerKind::Raster)
        .map(|(l, _)| l)
        .collect();
    let modes = &rasters[1..];
    let rows = (modes.len() as u32).div_ceil(COLUMNS);
    let (cell_w, cell_h) = (width / COLUMNS, height / rows.max(1));
    let (top, bottom) = stats(&ours, &expected, 0, 0, width, height);
    println!("whole image: max {top}, mean {bottom:.3}");
    println!("{:<4} {:<16} {:>8} {:>8} {:>8} {:>8}", "code", "mode", "max255", "mean255", "max128", "mean128");
    for (index, layer) in modes.iter().enumerate() {
        let (cx, cy) = (index as u32 % COLUMNS * cell_w + 2, index as u32 / COLUMNS * cell_h + 2);
        let (w, h) = (cell_w - 4, cell_h - 4);
        let code = layer.row.int("LayerComposite");
        let name = BlendMode::from_code(code).map(|m| format!("{m:?}")).unwrap_or_else(|| "?".into());
        let (max_a, mean_a) = stats(&ours, &expected, cx, cy, cx + w, cy + h / 2);
        let (max_b, mean_b) = stats(&ours, &expected, cx, cy + h / 2, cx + w, cy + h);
        println!("{code:<4} {name:<16} {max_a:>8} {mean_a:>8.3} {max_b:>8} {mean_b:>8.3}");
    }
    if args.len() > 2 {
        std::fs::write(&args[2], ours.to_png()?)?;
    }
    Ok(())
}
