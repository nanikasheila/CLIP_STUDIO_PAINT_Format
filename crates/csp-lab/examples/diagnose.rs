//! Find which layers make a render differ from CSP's stored preview: hide
//! one drawn layer at a time and report how the PSNR changes. A layer
//! whose removal raises the PSNR is drawn wrongly (or CSP does not show
//! it). Prints structure only (ids, kinds, flags), never names or pixels.
//!
//! Usage: diagnose FILE.clip [MAX_LAYERS]

use csp_format::render::{psnr, render_like_preview};
use csp_format::{ClipFile, LayerKind, Result};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let path = args.first().expect("usage: diagnose FILE.clip [MAX_LAYERS]");
    let limit: usize = args.get(1).and_then(|v| v.parse().ok()).unwrap_or(200);
    let mut file = ClipFile::open(path)?;
    let Some((image, report, preview)) = render_like_preview(&file)? else {
        println!("no preview");
        return Ok(());
    };
    let base = psnr(&image, &preview);
    println!(
        "baseline {base:.2} dB at {}x{}; {}",
        image.width,
        image.height,
        serde_json::to_string(&report).unwrap_or_default()
    );
    // Where the difference is: signed mean (ours − CSP) per channel, and the
    // mean absolute difference on an 8×8 grid (rows top to bottom).
    let (mut bias, mut n) = ([0f64; 3], 0f64);
    let mut grid = [[0f64; 8]; 8];
    let mut counts = [[0f64; 8]; 8];
    for y in 0..image.height {
        for x in 0..image.width {
            let (a, b) = (image.pixel(x, y), preview.pixel(x, y));
            let flat = |p: [u8; 4], c: usize| p[c] as f64 * p[3] as f64 / 255.0 + 255.0 * (1.0 - p[3] as f64 / 255.0);
            let (gx, gy) = ((x * 8 / image.width) as usize, (y * 8 / image.height) as usize);
            for (c, sum) in bias.iter_mut().enumerate() {
                let d = flat(a, c) - flat(b, c);
                *sum += d;
                grid[gy][gx] += d.abs() / 3.0;
            }
            counts[gy][gx] += 1.0;
            n += 1.0;
        }
    }
    println!("bias (ours - csp) {:.1} {:.1} {:.1}", bias[0] / n, bias[1] / n, bias[2] / n);
    for (row, count) in grid.iter().zip(&counts) {
        println!(
            "grid {}",
            row.iter().zip(count).map(|(g, c)| format!("{:5.1}", g / c.max(1.0))).collect::<Vec<_>>().join(" ")
        );
    }
    let layers: Vec<_> = file
        .layers
        .flatten()
        .into_iter()
        .filter(|(layer, _)| layer.visible && !matches!(layer.kind, LayerKind::Root))
        .map(|(layer, depth)| (layer.clone(), depth))
        .collect();
    let mut rows = Vec::new();
    for (layer, depth) in layers.into_iter().take(limit) {
        file.set_layer_visible(layer.main_id, false)?;
        let (image, _, _) = render_like_preview(&file)?.expect("preview");
        file.set_layer_visible(layer.main_id, true)?;
        let delta = psnr(&image, &preview) - base;
        let effects = layer.row.truthy("LayerEffectAttached");
        // Stored pixels with any alpha (0 = nothing stored to draw).
        let covered = match file.layer_pixels(&layer) {
            Ok(Some((pixels, _))) => pixels.pixels.as_chunks::<4>().0.iter().filter(|p| p[3] > 0).count(),
            _ => 0,
        };
        rows.push((
            delta,
            layer.main_id,
            depth,
            layer.kind.name().to_owned(),
            layer.row.int("LayerType"),
            layer.blend,
            layer.opacity,
            layer.clipping,
            layer.mask_enabled,
            effects,
            layer.row.int("DrawToRenderOffscreenType"),
            covered,
        ));
    }
    rows.sort_by(|a, b| b.0.total_cmp(&a.0));
    println!("delta_db id depth kind type blend opacity clip mask effects draw_type covered_px");
    for r in rows {
        println!(
            "{:+.2} {} {} {} {} {} {} {} {} {} {} {}",
            r.0, r.1, r.2, r.3, r.4, r.5, r.6, r.7, r.8, r.9, r.10, r.11
        );
    }
    Ok(())
}
