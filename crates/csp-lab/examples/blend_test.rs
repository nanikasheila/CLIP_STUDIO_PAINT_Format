//! Fill a CSP-made "one layer per blending mode" file with test patterns.
//!
//! Input: a .clip whose bottom raster layer is Normal and whose other
//! raster layers each use one blending mode (made in CSP, see
//! `docs/validation/V6-made-files.md`). Output: the same file with
//! - the bottom layer: a backdrop of hue (x) × lightness (y) gradients,
//! - every other layer: its own grid cell with a source pattern (colours
//!   across, alpha 255 in the top half and 128 in the bottom half).
//!
//! Opening the output in CSP and saving it makes CSP render the preview,
//! which `cspf compare` then checks cell by cell against our renderer.
//!
//! Usage: cargo run --example blend_test -- IN.clip OUT.clip

use csp_format::{ClipFile, LayerKind, Result, RgbaImage};

const COLUMNS: u32 = 6;

fn hsl(h: f64, s: f64, l: f64) -> [u8; 3] {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    [r, g, b].map(|v| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8)
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [input, output] = args.as_slice() else {
        eprintln!("usage: blend_test IN.clip OUT.clip");
        std::process::exit(2);
    };
    let mut file = ClipFile::open(input)?;
    let (width, height) = (file.canvas.width_px, file.canvas.height_px);
    let rasters: Vec<i64> = file
        .layers
        .flatten()
        .into_iter()
        .filter(|(layer, _)| layer.kind == LayerKind::Raster)
        .map(|(layer, _)| layer.main_id)
        .collect();
    let Some((&bottom, modes)) = rasters.split_first() else {
        eprintln!("no raster layers");
        std::process::exit(1);
    };
    let mut backdrop = RgbaImage::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let [r, g, b] = hsl(x as f64 / width as f64 * 360.0, 0.8, 0.15 + 0.7 * y as f64 / height as f64);
            backdrop.set_pixel(x, y, [r, g, b, 255]);
        }
    }
    file.set_layer_pixels(bottom, &backdrop, (0, 0))?;
    let rows = (modes.len() as u32).div_ceil(COLUMNS);
    let (cell_w, cell_h) = (width / COLUMNS, height / rows.max(1));
    for (index, layer) in modes.iter().enumerate() {
        let (cx, cy) = (index as u32 % COLUMNS, index as u32 / COLUMNS);
        let mut cell = RgbaImage::new(cell_w - 4, cell_h - 4);
        for y in 0..cell.height {
            for x in 0..cell.width {
                let [r, g, b] = hsl(
                    (x as f64 / cell.width as f64 * 360.0 + 180.0) % 360.0,
                    0.9,
                    0.2 + 0.6 * (y % 16) as f64 / 15.0,
                );
                let alpha = if y < cell.height / 2 { 255 } else { 128 };
                cell.set_pixel(x, y, [r, g, b, alpha]);
            }
        }
        file.set_layer_pixels(*layer, &cell, ((cx * cell_w + 2) as i64, (cy * cell_h + 2) as i64))?;
    }
    file.update_preview()?;
    file.renew_file_id();
    file.save(output)?;
    let report = csp_format::verify::check_clip(std::fs::read(output)?, Default::default());
    eprintln!("wrote {output}; self-check ok = {}", report.ok());
    if !report.ok() {
        eprintln!("{}", serde_json::to_string_pretty(&report).unwrap_or_default());
        std::process::exit(1);
    }
    Ok(())
}
