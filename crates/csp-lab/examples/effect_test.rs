//! Test files for layer effects (border フチ, tone).
//!
//! `base IN OUT`: from a CSP-made file with raster layers (e.g. the blend
//! test source), make every raster layer Normal and give each one a grid
//! cell with shapes whose edges matter for effects: a hard disc, an
//! antialiased disc, a 1 px line, a single pixel, a soft alpha ramp and
//! a hard square. Turn on an effect for one layer in CSP and save; that
//! gives the `LayerEffectInfo` template.
//!
//! `vary IN OUT`: copy the template of the layer that has an effect to the
//! other layers with varied border widths and colours (listed on stdout),
//! so one CSP export shows many settings.

use csp_format::render::BlendMode;
use csp_format::{ClipFile, LayerKind, Result, RgbaImage};
use rusqlite::types::Value;

const COLUMNS: u32 = 6;

fn cells(file: &ClipFile) -> (Vec<i64>, u32, u32) {
    let rasters: Vec<i64> = file
        .layers
        .flatten()
        .into_iter()
        .filter(|(layer, _)| layer.kind == LayerKind::Raster)
        .map(|(layer, _)| layer.main_id)
        .collect();
    let rows = (rasters.len() as u32).div_ceil(COLUMNS);
    (rasters, file.canvas.width_px / COLUMNS, file.canvas.height_px / rows.max(1))
}

fn pattern(w: u32, h: u32, colour: [u8; 3]) -> RgbaImage {
    let mut image = RgbaImage::new(w, h);
    let [r, g, b] = colour;
    let (cx, cy, radius) = (w as f64 * 0.3, h as f64 * 0.3, w as f64 * 0.18);
    for y in 0..h {
        for x in 0..w {
            let (fx, fy) = (x as f64 + 0.5, y as f64 + 0.5);
            // Hard disc (top left).
            if (fx - cx).hypot(fy - cy) <= radius {
                image.set_pixel(x, y, [r, g, b, 255]);
            }
            // Antialiased disc (top right): coverage from the distance.
            let d = (fx - w as f64 * 0.72).hypot(fy - cy) - radius;
            let a = (0.5 - d).clamp(0.0, 1.0);
            if a > 0.0 {
                image.set_pixel(x, y, [r, g, b, (a * 255.0).round() as u8]);
            }
            // Soft alpha ramp (bottom band).
            if y >= h * 3 / 4 && y < h * 3 / 4 + 6 && x >= 4 && x < w - 4 {
                image.set_pixel(x, y, [r, g, b, ((x - 4) * 255 / (w - 9)) as u8]);
            }
        }
    }
    // 1 px vertical line, single pixel, hard square.
    for y in h * 55 / 100..h * 70 / 100 {
        image.set_pixel(w / 5, y, [r, g, b, 255]);
    }
    image.set_pixel(w * 2 / 5, h * 62 / 100, [r, g, b, 255]);
    for y in h * 55 / 100..h * 68 / 100 {
        for x in w * 60 / 100..w * 78 / 100 {
            image.set_pixel(x, y, [r, g, b, 255]);
        }
    }
    image
}

fn base(input: &str, output: &str) -> Result<()> {
    let mut file = ClipFile::open(input)?;
    let (layers, cell_w, cell_h) = cells(&file);
    for (index, &layer) in layers.iter().enumerate() {
        let (cx, cy) = (index as u32 % COLUMNS * cell_w, index as u32 / COLUMNS * cell_h);
        file.set_layer_blend(layer, BlendMode::Normal)?;
        let colour = [[40, 40, 40], [200, 40, 40], [40, 120, 220]][index % 3];
        file.set_layer_pixels(layer, &pattern(cell_w - 8, cell_h - 8, colour), ((cx + 4) as i64, (cy + 4) as i64))?;
    }
    file.update_preview()?;
    file.renew_file_id();
    file.save(output)
}

/// Border settings tried in `vary`: (width, RGB as 0..=255).
const BORDERS: [(f64, [u8; 3]); 14] = [
    (1.0, [0, 0, 0]),
    (2.0, [0, 0, 0]),
    (3.0, [0, 0, 0]),
    (5.0, [0, 0, 0]),
    (10.0, [0, 0, 0]),
    (20.0, [0, 0, 0]),
    (1.5, [0, 0, 0]),
    (2.5, [0, 0, 0]),
    (4.133858267716536, [0, 0, 0]),
    (5.0, [255, 255, 255]),
    (5.0, [255, 0, 0]),
    (5.0, [0, 128, 255]),
    (0.5, [0, 0, 0]),
    (7.0, [255, 200, 0]),
];

fn vary(input: &str, output: &str) -> Result<()> {
    let mut file = ClipFile::open(input)?;
    let (layers, _, _) = cells(&file);
    let template = layers
        .iter()
        .find_map(|id| {
            let layer = file.layers.get(*id)?;
            (layer.row.int("LayerEffectAttached") == 1).then(|| layer.row.clone())
        })
        .ok_or_else(|| csp_format::Error::format("no layer with an effect (turn one on in CSP first)"))?;
    let blob = template.blob("LayerEffectInfo").unwrap_or_default().to_vec();
    let edge = find_section(&blob, "EffectEdge").ok_or_else(|| csp_format::Error::format("no EffectEdge"))?;
    let copy_columns = [
        "LayerEffectAttached",
        "EffectLayerRenderType",
        "EffectRenderType",
        "EffectReferAreaType",
        "EffectSetUpdateRectType",
        "EffectOffscreenFixType",
        "EffectOffscreenMoveType",
        "LayerEffectEdgeNewRender",
    ];
    let mut index = 0;
    for &layer in &layers {
        if file.layers.get(layer).is_some_and(|l| l.row.int("LayerEffectAttached") == 1) {
            continue;
        }
        // Every setting twice: with the newer border renderer
        // (`LayerEffectEdgeNewRender` = 1, what CSP sets today) and without
        // it (most corpus layers).
        let Some(&(width, [r, g, b])) = BORDERS.get(index % BORDERS.len()) else { break };
        let new_render = index < BORDERS.len();
        let mut info = blob.clone();
        info[edge..edge + 4].copy_from_slice(&1u32.to_be_bytes());
        info[edge + 4..edge + 12].copy_from_slice(&width.to_be_bytes());
        for (k, c) in [r, g, b].into_iter().enumerate() {
            let v = u32::from_be_bytes([c; 4]);
            info[edge + 12 + 4 * k..edge + 16 + 4 * k].copy_from_slice(&v.to_be_bytes());
        }
        file.set_layer_value(layer, "LayerEffectInfo", Value::Blob(info))?;
        for column in copy_columns {
            if let Some(value) = template.get(column) {
                file.set_layer_value(layer, column, value.clone())?;
            }
        }
        if !new_render {
            file.set_layer_value(layer, "LayerEffectEdgeNewRender", Value::Null)?;
        }
        let cell = layers.iter().position(|&l| l == layer).unwrap_or(0);
        println!("cell {cell} layer {layer}: border width {width} colour {r},{g},{b} new_render {new_render}");
        index += 1;
    }
    file.renew_file_id();
    file.save(output)
}

/// Offset of a section's payload: the u32 character count, the UTF-16BE
/// name, then the payload.
fn find_section(blob: &[u8], name: &str) -> Option<usize> {
    let mut needle = (name.len() as u32).to_be_bytes().to_vec();
    for unit in name.encode_utf16() {
        needle.extend_from_slice(&unit.to_be_bytes());
    }
    blob.windows(needle.len()).position(|w| w == needle).map(|at| at + needle.len())
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["base", input, output] => base(input, output),
        ["vary", input, output] => vary(input, output),
        _ => {
            eprintln!("usage: effect_test base IN OUT | vary IN OUT");
            std::process::exit(2);
        }
    }
}
