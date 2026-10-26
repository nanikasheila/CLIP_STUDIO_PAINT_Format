//! Test file for the tone layer effect (トーン, `EffectTone`).
//!
//! `tone_test TEMPLATE OUT [b]`: from `effects_tmpl.clip` (a CSP-made file
//! whose first raster layer has an effect switched on in CSP), keep eight
//! raster layers in a 2 × 4 grid and hide the rest. Each cell holds a
//! pattern and its layer gets the tone switched on with the settings
//! listed on stdout. Exporting the merged image from CSP shows how each
//! setting is drawn.
//!
//! Round a: an opaque grey ramp (top half) and a black alpha ramp (bottom
//! half) under different lines, angles, dot shapes and densities.
//! Round b: grey under partial alpha, flat colours (for the luminance),
//! the unknown dot shape 4, another angle, a position, and fixed
//! densities.

use csp_format::render::BlendMode;
use csp_format::{ClipFile, LayerKind, Result, RgbaImage};
use rusqlite::types::Value;

/// The 76-byte `EffectTone` payload as 19 big-endian words; words 1–2 and
/// 6–7 are f64 (resolution, lines per inch).
struct Tone {
    pattern: Pattern,
    lines: f64,
    angle: u32,
    word3: u32,
    word4: u32,
    word5: u32,
    word11: u32,
    /// Words 13 and 14 (position, unconfirmed).
    position: (i32, i32),
}

#[derive(Clone, Copy, Debug)]
enum Pattern {
    /// Opaque grey ramp over a black alpha ramp.
    Ramps,
    /// Grey 128 with an alpha ramp over a grey ramp at alpha 128.
    PartialAlpha,
    /// Bands of flat red, green, blue and yellow.
    Colours,
}

const DEFAULT: Tone = Tone {
    pattern: Pattern::Ramps,
    lines: 20.0,
    angle: 45,
    word3: 1,
    word4: 0,
    word5: 20,
    word11: 30,
    position: (0, 0),
};

fn variants(round: &str) -> Vec<Tone> {
    if round == "b" {
        return vec![
            Tone { pattern: Pattern::PartialAlpha, ..DEFAULT },
            Tone { pattern: Pattern::Colours, lines: 60.0, ..DEFAULT },
            Tone { word3: 4, angle: 0, ..DEFAULT },
            Tone { angle: 30, ..DEFAULT },
            Tone { position: (5, 7), ..DEFAULT },
            Tone { word4: 2, word5: 20, ..DEFAULT },
            Tone { word4: 2, word5: 80, ..DEFAULT },
            Tone { word11: 100, ..DEFAULT },
        ];
    }
    vec![
        DEFAULT,
        Tone { lines: 10.0, ..DEFAULT },
        Tone { lines: 30.0, ..DEFAULT },
        Tone { lines: 60.0, ..DEFAULT },
        Tone { angle: 0, ..DEFAULT },
        Tone { word3: 4, ..DEFAULT },
        Tone { word3: 2, ..DEFAULT },
        Tone { word4: 2, word5: 51, ..DEFAULT },
    ]
}

fn payload(tone: &Tone, resolution: f64) -> Vec<u8> {
    let mut out = Vec::with_capacity(76);
    out.extend_from_slice(&1u32.to_be_bytes());
    out.extend_from_slice(&resolution.to_be_bytes());
    for word in [tone.word3, tone.word4, tone.word5] {
        out.extend_from_slice(&word.to_be_bytes());
    }
    out.extend_from_slice(&tone.lines.to_be_bytes());
    for word in [1, 0, tone.angle, tone.word11, 0] {
        out.extend_from_slice(&u32::to_be_bytes(word));
    }
    out.extend_from_slice(&tone.position.0.to_be_bytes());
    out.extend_from_slice(&tone.position.1.to_be_bytes());
    for word in [0u32; 4] {
        out.extend_from_slice(&word.to_be_bytes());
    }
    assert_eq!(out.len(), 76);
    out
}

fn draw(pattern: Pattern, w: u32, h: u32) -> RgbaImage {
    let mut image = RgbaImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let v = (x * 255 / (w - 1)) as u8;
            let top = y < h / 2;
            let pixel = match pattern {
                Pattern::Ramps if top => [v, v, v, 255],
                Pattern::Ramps => [0, 0, 0, 255 - v],
                Pattern::PartialAlpha if top => [128, 128, 128, v],
                Pattern::PartialAlpha => [v, v, v, 128],
                Pattern::Colours => {
                    [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255], [255, 255, 0, 255]][(y * 4 / h) as usize]
                }
            };
            image.set_pixel(x, y, pixel);
        }
    }
    image
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
    let (input, output, round) = match args.as_slice() {
        [input, output] => (input, output, "a"),
        [input, output, round] => (input, output, round.as_str()),
        _ => {
            eprintln!("usage: tone_test TEMPLATE OUT [b]");
            std::process::exit(2);
        }
    };
    let mut file = ClipFile::open(input)?;
    let resolution = file.canvas.resolution;
    let rasters: Vec<i64> = file
        .layers
        .flatten()
        .into_iter()
        .filter(|(layer, _)| layer.kind == LayerKind::Raster)
        .map(|(layer, _)| layer.main_id)
        .collect();
    let template = rasters
        .iter()
        .find_map(|id| {
            let layer = file.layers.get(*id)?;
            (layer.row.int("LayerEffectAttached") == 1).then(|| layer.row.clone())
        })
        .ok_or_else(|| csp_format::Error::format("no layer with an effect (turn one on in CSP first)"))?;
    let blob = template.blob("LayerEffectInfo").unwrap_or_default().to_vec();
    let edge = find_section(&blob, "EffectEdge").ok_or_else(|| csp_format::Error::format("no EffectEdge"))?;
    let tone_at = find_section(&blob, "EffectTone").ok_or_else(|| csp_format::Error::format("no EffectTone"))?;
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
    let (cell_w, cell_h) = (file.canvas.width_px / 2, file.canvas.height_px / 4);
    let tones = variants(round);
    for (index, &layer) in rasters.iter().enumerate() {
        let Some(tone) = tones.get(index) else {
            file.set_layer_visible(layer, false)?;
            continue;
        };
        let (cx, cy) = (index as u32 % 2 * cell_w, index as u32 / 2 * cell_h);
        file.set_layer_blend(layer, BlendMode::Normal)?;
        file.set_layer_pixels(layer, &draw(tone.pattern, cell_w - 8, cell_h - 8), ((cx + 4) as i64, (cy + 4) as i64))?;
        let mut info = blob.clone();
        info[edge..edge + 4].copy_from_slice(&0u32.to_be_bytes());
        info[tone_at..tone_at + 76].copy_from_slice(&payload(tone, resolution));
        file.set_layer_value(layer, "LayerEffectInfo", Value::Blob(info))?;
        for column in copy_columns {
            if let Some(value) = template.get(column) {
                file.set_layer_value(layer, column, value.clone())?;
            }
        }
        println!(
            "cell {index} layer {layer}: {:?} lines {} angle {} word3 {} word4 {} word5 {} word11 {} position {:?}",
            tone.pattern, tone.lines, tone.angle, tone.word3, tone.word4, tone.word5, tone.word11, tone.position
        );
    }
    file.update_preview()?;
    file.renew_file_id();
    file.save(output)
}
