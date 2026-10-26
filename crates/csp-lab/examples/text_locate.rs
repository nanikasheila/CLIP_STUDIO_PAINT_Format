//! Where CSP shows the text blocks of balloon layers: for each block whose
//! bbox is relative (record 45 = 1), slide its rendered image (shrunk to the
//! preview's scale) over the stored preview near the layer's balloons and
//! take the position where the preview is darkest under the text's ink.
//! Prints that position (canvas pixels, of the bbox origin) next to candidate
//! origins: the centre of all balloon objects' union, and of each object.
//! Numbers only.
//!
//! Usage: text_locate FILE.clip [MAX_BLOCKS]

use csp_format::stroke::layer_vectors;
use csp_format::text::read_blocks;
use csp_format::{ClipFile, LayerKind, Result, image::luma};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let file = ClipFile::open(&args[0])?;
    let max: usize = args.get(1).and_then(|v| v.parse().ok()).unwrap_or(20);
    let Some(preview) = file.preview()? else { return Ok(()) };
    let scale = preview.width as f64 / file.canvas.width_px as f64;
    let dark: Vec<f32> = (0..preview.height)
        .flat_map(|y| (0..preview.width).map(move |x| (x, y)))
        .map(|(x, y)| {
            let p = preview.pixel(x, y);
            let a = p[3] as f32 / 255.0;
            255.0 - (luma(p[0], p[1], p[2]) as f32 * a + 255.0 * (1.0 - a))
        })
        .collect();
    let mut shown = 0;
    for (layer, _) in file.layers.flatten() {
        if layer.kind != LayerKind::Text || !layer.visible {
            continue;
        }
        let objects: Vec<_> = layer_vectors(&file, layer)?.into_iter().flat_map(|d| d.objects).collect();
        if objects.is_empty() {
            continue;
        }
        let union = objects.iter().map(|o| o.bbox()).fold([i32::MAX, i32::MAX, i32::MIN, i32::MIN], |u, b| {
            [u[0].min(b[0]), u[1].min(b[1]), u[2].max(b[2]), u[3].max(b[3])]
        });
        let (ox, oy) = (layer.offset.0 as f64, layer.offset.1 as f64);
        for block in read_blocks(&layer.row)? {
            let a = &block.attributes;
            let relative = a.get(45).and_then(|d| d.first().copied()).unwrap_or(0);
            let (Some(id), Some(bbox)) = (a.render_offscreen(), a.bbox()) else { continue };
            if id == 0 {
                continue;
            }
            let Some(offscreen) = file.offscreen(id as i64)? else { continue };
            if offscreen.data.is_none() {
                continue;
            }
            shown += 1;
            if shown > max {
                return Ok(());
            }
            let image = offscreen.to_rgba()?;
            let (w, h) = (
                ((image.width as f64 * scale).round() as u32).max(1),
                ((image.height as f64 * scale).round() as u32).max(1),
            );
            let small = image.resize_area(w, h);
            let ink: Vec<(u32, u32, f32)> = (0..h)
                .flat_map(|y| (0..w).map(move |x| (x, y)))
                .filter_map(|(x, y)| {
                    let p = small.pixel(x, y);
                    let v = p[3] as f32 / 255.0 * (255.0 - luma(p[0], p[1], p[2]) as f32) / 255.0;
                    (v > 0.0).then_some((x, y, v))
                })
                .collect();
            if ink.is_empty() {
                continue;
            }
            // Search near the balloons (relative blocks) or the stored bbox.
            let area = if relative != 0 {
                let m = ((union[2] - union[0]).max(union[3] - union[1]) / 2) as f64;
                [union[0] as f64 + ox - m, union[1] as f64 + oy - m, union[2] as f64 + ox + m, union[3] as f64 + oy + m]
            } else {
                let m = 200.0;
                [bbox[0] as f64 + ox - m, bbox[1] as f64 + oy - m, bbox[2] as f64 + ox + m, bbox[3] as f64 + oy + m]
            };
            let to_p = |v: f64| (v * scale).floor() as i64;
            // Normalised cross-correlation of the ink with the darkness.
            let pw = preview.width as usize;
            let n = (w * h) as f64;
            let mean_t = ink.iter().map(|i| i.2 as f64).sum::<f64>() / n;
            let norm_t = (ink.iter().map(|i| (i.2 as f64).powi(2)).sum::<f64>() - n * mean_t * mean_t).sqrt();
            let mut best = (f32::MIN, 0i64, 0i64);
            for py in to_p(area[1]).max(0)..to_p(area[3]).min(preview.height as i64 - h as i64) {
                for px in to_p(area[0]).max(0)..to_p(area[2]).min(preview.width as i64 - w as i64) {
                    let (mut sum, mut sum2) = (0f64, 0f64);
                    for y in 0..h as usize {
                        let row = &dark[(py as usize + y) * pw + px as usize..][..w as usize];
                        for d in row {
                            sum += *d as f64;
                            sum2 += (*d as f64).powi(2);
                        }
                    }
                    let mut s = 0f64;
                    for (x, y, v) in &ink {
                        s += *v as f64 * dark[(py as usize + *y as usize) * pw + px as usize + *x as usize] as f64;
                    }
                    let var = (sum2 - sum * sum / n).max(1e-9).sqrt();
                    let ncc = ((s - mean_t * sum) / (norm_t * var).max(1e-9)) as f32;
                    if ncc > best.0 {
                        best = (ncc, px, py);
                    }
                }
            }
            let found = (best.1 as f64 / scale, best.2 as f64 / scale);
            let centre = |b: [i32; 4]| ((b[0] + b[2]) as f64 / 2.0 + ox, (b[1] + b[3]) as f64 / 2.0 + oy);
            let u = centre(union);
            let nearest = objects
                .iter()
                .map(|o| centre(o.bbox()))
                .map(|c| (c.0 + bbox[0] as f64 - found.0, c.1 + bbox[1] as f64 - found.1))
                .min_by(|a, b| a.0.hypot(a.1).total_cmp(&b.0.hypot(b.1)))
                .unwrap_or((f64::NAN, f64::NAN));
            let by_index = |i: usize| {
                objects
                    .get(i)
                    .map(|o| centre(o.bbox()))
                    .map(|c| (c.0 + bbox[0] as f64 - found.0, c.1 + bbox[1] as f64 - found.1))
            };
            let r = relative as usize;
            println!(
                "  obj[rel] err {:?} obj[rel-1] err {:?}",
                by_index(r).map(|e| (e.0.round(), e.1.round())),
                r.checked_sub(1).and_then(by_index).map(|e| (e.0.round(), e.1.round()))
            );
            println!(
                "layer {} rel {} bbox {:?} found_origin ({:.0},{:.0}) score {:.0} | stored {:?} | union_centre+bbox err ({:.0},{:.0}) | nearest_obj err ({:.0},{:.0}) objects {} px/canvas {:.3}",
                layer.main_id,
                relative,
                bbox,
                found.0,
                found.1,
                best.0 * 100.0,
                (bbox[0] as f64 + ox, bbox[1] as f64 + oy),
                u.0 + bbox[0] as f64 - found.0,
                u.1 + bbox[1] as f64 - found.1,
                nearest.0,
                nearest.1,
                objects.len(),
                scale
            );
        }
    }
    Ok(())
}
