//! Compare the vector approximation (`stroke.rs`) with the pixels CSP
//! stored for vector layers that have them (full-size level), to see
//! whether strokes come out too thin or too thick. Per layer: the header
//! layouts, the ink (sum of alpha) of the approximation over CSP's for
//! brush sizes scaled by 0.5 … 2, and the overlap (IoU of alpha > 127) at
//! scale 1. Numbers only (no names or paths).
//!
//! Usage: vector_fit FILE.clip...
//!
//! With `VFIT_DUMP=DIR`, a 512 × 512 crop of CSP's pixels and of the
//! approximation (scale 1) is written per layer, for looking at.

use std::collections::BTreeMap;

use csp_format::stroke::{Placement, brushes, draw_objects, layer_vectors};
use csp_format::{ClipFile, LayerKind, Result, RgbaImage};

const FACTORS: [f64; 5] = [0.5, 0.75, 1.0, 1.5, 2.0];
/// Layers larger than this (pixels) are skipped.
const PIXELS_MAX: u64 = 40_000_000;

fn ink(image: &RgbaImage) -> f64 {
    image.pixels.as_chunks::<4>().0.iter().map(|p| p[3] as f64 / 255.0).sum()
}

fn iou(a: &RgbaImage, b: &RgbaImage) -> f64 {
    let (mut both, mut either) = (0u64, 0u64);
    for (p, q) in a.pixels.as_chunks::<4>().0.iter().zip(b.pixels.as_chunks::<4>().0.iter()) {
        let (x, y) = (p[3] > 127, q[3] > 127);
        both += (x && y) as u64;
        either += (x || y) as u64;
    }
    if either == 0 { 1.0 } else { both as f64 / either as f64 }
}

/// Centre of the 512-pixel tile with the most ink.
fn densest(image: &RgbaImage) -> (u32, u32) {
    let tile = 512;
    let mut best = (0.0, image.width / 2, image.height / 2);
    for ty in (0..image.height).step_by(tile / 2) {
        for tx in (0..image.width).step_by(tile / 2) {
            let mut sum = 0.0;
            for y in (ty..(ty + tile as u32).min(image.height)).step_by(4) {
                for x in (tx..(tx + tile as u32).min(image.width)).step_by(4) {
                    sum += image.pixel(x, y)[3] as f64;
                }
            }
            if sum > best.0 {
                best = (sum, tx + tile as u32 / 2, ty + tile as u32 / 2);
            }
        }
    }
    (best.1, best.2)
}

/// A `size` square around (cx, cy), over white.
fn crop(image: &RgbaImage, cx: u32, cy: u32, size: u32) -> RgbaImage {
    let mut out = RgbaImage::new(size, size);
    out.fill([255; 4]);
    let (x0, y0) = (cx as i64 - size as i64 / 2, cy as i64 - size as i64 / 2);
    for y in 0..size {
        for x in 0..size {
            let (sx, sy) = (x0 + x as i64, y0 + y as i64);
            if sx < 0 || sy < 0 || sx >= image.width as i64 || sy >= image.height as i64 {
                continue;
            }
            let p = image.pixel(sx as u32, sy as u32);
            let a = p[3] as u32;
            let mix = |c: u8| ((c as u32 * a + 255 * (255 - a)) / 255) as u8;
            out.set_pixel(x, y, [mix(p[0]), mix(p[1]), mix(p[2]), 255]);
        }
    }
    out
}

/// Distance from (x, y) to the nearest pixel with alpha below 128, when
/// (x, y) is inside the ink and one is within `limit`.
fn half_width(image: &RgbaImage, x: f64, y: f64, limit: f64) -> Option<f64> {
    let (px, py) = (x.floor() as i64, y.floor() as i64);
    let inside = |px: i64, py: i64| {
        px >= 0
            && py >= 0
            && px < image.width as i64
            && py < image.height as i64
            && image.pixel(px as u32, py as u32)[3] >= 128
    };
    if !inside(px, py) {
        return None;
    }
    let reach = limit.ceil() as i64;
    let mut best = f64::MAX;
    for qy in py - reach..=py + reach {
        for qx in px - reach..=px + reach {
            if !inside(qx, qy) {
                let d = ((qx as f64 + 0.5 - x).powi(2) + (qy as f64 + 0.5 - y).powi(2)).sqrt();
                best = best.min(d);
            }
        }
    }
    (best <= limit).then_some(best)
}

/// Medians of each bucket ("-" for empty ones), with the counts.
fn medians(buckets: &mut [Vec<f64>]) -> String {
    buckets
        .iter_mut()
        .map(|values| {
            values.sort_by(f64::total_cmp);
            values.get(values.len() / 2).map(|v| format!("{v:.2}({})", values.len())).unwrap_or("-".into())
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn main() -> Result<()> {
    for (index, path) in std::env::args().skip(1).enumerate() {
        let Ok(file) = ClipFile::open(&path) else { continue };
        let brushes = brushes(&file)?;
        for (layer, _) in file.layers.flatten() {
            if layer.kind != LayerKind::Vector || layer.render_mipmap == 0 || !layer.visible {
                continue;
            }
            let Some(stored) = file.mipmap_base(layer.render_mipmap)? else { continue };
            if stored.data.is_none() {
                continue;
            }
            let stored = stored.to_rgba()?;
            if stored.width as u64 * stored.height as u64 > PIXELS_MAX || ink(&stored) == 0.0 {
                continue;
            }
            let vectors = layer_vectors(&file, layer)?;
            let mut headers: BTreeMap<usize, usize> = BTreeMap::new();
            let mut styles: BTreeMap<u32, usize> = BTreeMap::new();
            let (mut pressure, mut value1, mut points) = (0.0, 0.0, 0usize);
            // How far a point's segment box reaches beyond the point, per
            // unit of brush size (first points of objects).
            let mut reach = Vec::new();
            // The same for every point, by pressure tenth.
            let mut by_pressure: Vec<Vec<f64>> = vec![Vec::new(); 10];
            // CSP's own half width at each point (the distance to its nearest
            // pixel with alpha < 128) per unit of brush size, by pressure
            // tenth and by value-1 tenth.
            let mut width_p: Vec<Vec<f64>> = vec![Vec::new(); 10];
            let mut width_v: Vec<Vec<f64>> = vec![Vec::new(); 10];
            let origin = layer.pixel_origin();
            let shift = ((layer.offset.0 - origin.0) as f64, (layer.offset.1 - origin.1) as f64);
            for data in &vectors {
                for object in &data.objects {
                    *headers.entry(object.header_size()).or_default() += 1;
                    if let (Some(point), Some(size)) = (object.point(0), object.brush_size())
                        && size > 0.0
                    {
                        reach.push(((point.x - point.bbox[0] as f64).min(point.y - point.bbox[1] as f64)) / size);
                    }
                    for id in object.brush_styles() {
                        *styles.entry(id).or_default() += 1;
                    }
                    for i in 0..object.points.len() {
                        if let (Some(point), Some(size)) = (object.point(i), object.brush_size())
                            && size > 0.0
                        {
                            let b = point.bbox;
                            let r = (point.x - b[0] as f64)
                                .min(b[2] as f64 - point.x)
                                .min(point.y - b[1] as f64)
                                .min(b[3] as f64 - point.y);
                            let bucket = ((point.values[0] * 10.0) as usize).min(9);
                            by_pressure[bucket].push(r / size);
                        }
                        if let (Some(point), Some(size)) = (object.point(i), object.brush_size())
                            && size > 0.0
                            && let Some(r) = half_width(&stored, point.x + shift.0, point.y + shift.1, 3.0 * size + 4.0)
                        {
                            let bucket = |v: f32| ((v * 10.0) as usize).min(9);
                            width_p[bucket(point.values[0])].push(r / size);
                            width_v[bucket(point.values[1])].push(r / size);
                        }
                        if let Some(point) = object.point(i) {
                            pressure += point.values[0] as f64;
                            value1 += point.values[1] as f64;
                            points += 1;
                        }
                    }
                }
            }
            let style = styles.iter().max_by_key(|(_, n)| **n).map(|(id, _)| *id).unwrap_or(0);
            let effector = file
                .database()
                .query_row("SELECT SizeEffector FROM BrushStyle WHERE MainId = ?1", [style], |row| {
                    row.get::<_, Vec<u8>>(0)
                })
                .map(|blob| blob.iter().take(16).map(|b| format!("{b:02x}")).collect::<String>())
                .unwrap_or_default();
            let points = points.max(1) as f64;
            reach.sort_by(f64::total_cmp);
            let reach = reach.get(reach.len() / 2).copied().unwrap_or(0.0);
            if headers.is_empty() {
                continue;
            }
            let at = Placement { offset: shift, scale: 1.0 };
            let mut ratios = Vec::new();
            let mut overlap = 0.0;
            for factor in FACTORS {
                let mut ours = RgbaImage::new(stored.width, stored.height);
                for data in &vectors {
                    let mut objects = data.objects.clone();
                    for object in &mut objects {
                        if let Some(size) = object.brush_size() {
                            object.set_brush_size(size * factor);
                        }
                    }
                    draw_objects(&mut ours, &objects, &brushes, at);
                }
                ratios.push(format!("{factor}:{:.2}", ink(&ours) / ink(&stored)));
                if factor == 1.0 {
                    overlap = iou(&ours, &stored);
                    if let Ok(dir) = std::env::var("VFIT_DUMP") {
                        let (cx, cy) = densest(&stored);
                        for (name, image) in [("csp", &stored), ("ours", &ours)] {
                            let crop = crop(image, cx, cy, 512);
                            std::fs::write(format!("{dir}/vf_{index}_{}_{name}.png", layer.main_id), crop.to_png()?)?;
                        }
                    }
                }
            }
            let mut sizes: Vec<f64> =
                vectors.iter().flat_map(|d| d.objects.iter().filter_map(|o| o.brush_size())).collect();
            sizes.sort_by(f64::total_cmp);
            let curve = brushes
                .get(&(style as i64))
                .and_then(|b| b.size.pressure.as_ref())
                .map(|(m, c)| format!("min {m} {:?}", c.points));
            println!(
                "file {index} layer {} size median {:.1} curve {curve:?}",
                layer.main_id,
                sizes.get(sizes.len() / 2).copied().unwrap_or(0.0)
            );
            println!(
                "file {index} layer {} reach/size by pressure tenth: {}",
                layer.main_id,
                medians(&mut by_pressure)
            );
            println!(
                "file {index} layer {} csp half width/size by pressure tenth: {}",
                layer.main_id,
                medians(&mut width_p)
            );
            println!(
                "file {index} layer {} csp half width/size by value1 tenth: {}",
                layer.main_id,
                medians(&mut width_v)
            );
            println!(
                "file {index} layer {} headers {headers:?} ink ratio {} iou {overlap:.2} brush {style} size effector {effector} pressure {:.2} value1 {:.2} dpi {} reach/size {reach:.2}",
                layer.main_id,
                ratios.join(" "),
                pressure / points,
                value1 / points,
                file.canvas.resolution
            );
        }
    }
    Ok(())
}
